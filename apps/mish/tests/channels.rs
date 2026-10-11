use anyhow::Result;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use mish::{
    appliance::{EventStore, now},
    channel_store::{ChannelPlan, ChannelScope, PlannedChannel},
    channels,
    coordinator::Coordinator,
    domain::{AcknowledgementSource, MessageText, NewRecord, Role, StaffName, TransmissionState},
    recovery::restore_snapshot,
    server::{AppState, router},
    storage::{MishStore, default_config},
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

fn plan() -> Result<ChannelPlan> {
    let region = channels::mesh_name("Astral Valley")?;
    let definitions = [
        ("community", ChannelScope::Community, region.as_str()),
        ("staff", ChannelScope::Staff, "Staff"),
        ("medical", ChannelScope::Department, "Medical"),
        ("safety", ChannelScope::Department, "Safety"),
    ];
    let mut planned = Vec::new();
    for (slot, (id, scope, name)) in definitions.into_iter().enumerate() {
        let mut settings = channels::generated(name)?.settings;
        // An unknown protobuf field must survive scoped enrollment unchanged.
        settings.extend([0x98, 0x06, 0x01]);
        assert_eq!(channels::settings_view(&settings)?.psk.len(), 32);
        planned.push(PlannedChannel {
            id: id.into(),
            scope,
            slot: slot as u32,
            settings,
        });
    }
    let lora = vec![0x08, 0x01, 0x98, 0x06, 0x01];
    channels::validate_lora(&lora)?;
    Ok(ChannelPlan {
        channels: planned,
        lora: Some(lora),
    })
}

struct Fixture {
    directory: tempfile::TempDir,
    store: EventStore,
    app: Router,
    owner_token: String,
    owner_id: String,
    staff_token: String,
    medical_token: String,
    medical_id: String,
    plan: ChannelPlan,
}

fn fixture() -> Result<Fixture> {
    let directory = tempfile::tempdir()?;
    let store = EventStore::new(directory.path(), 100, false)?;
    let owner = store.enroll("Owner private identity", Role::Operator, true)?;
    let staff = store.enroll("General private staff", Role::Staff, false)?;
    let medical = store.enroll("Medical private staff", Role::Staff, false)?;
    let plan = plan()?;
    store.install_channel_plan(&plan)?;
    let observed = store
        .radio_channels()?
        .into_iter()
        .map(|channel| (channel.metadata.slot, channel.metadata.fingerprint))
        .collect::<Vec<_>>();
    store.mark_deployment_verified(&observed)?;
    let config = default_config(Some(directory.path()));
    let app = router(AppState {
        coordinator: Coordinator::start(store.clone(), 7),
        coordinator_id: "test-event".into(),
        radio_status: Arc::new(Mutex::new("disabled".into())),
        adapter: "disabled".into(),
        legacy: MishStore::new(directory.path(), config)?,
        legacy_slots: Arc::new(tokio::sync::Semaphore::new(4)),
    });
    Ok(Fixture {
        directory,
        store,
        app,
        plan,
        owner_token: owner.token,
        owner_id: owner.id,
        staff_token: staff.token,
        medical_token: medical.token,
        medical_id: medical.id,
    })
}

async fn raw_request(
    app: &Router,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: String,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    assert_eq!(response.headers()["cache-control"], "no-store");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn request(
    app: &Router,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    raw_request(app, method, path, token, body.to_string()).await
}

fn assert_scoped_export(
    export: &Value,
    plan: &ChannelPlan,
    selected_slot: Option<usize>,
) -> Result<()> {
    let decoded = channels::import_url(export["url"].as_str().unwrap())?;
    let mut expected = vec![plan.channels[0].settings.clone()];
    if let Some(slot) = selected_slot {
        expected.push(plan.channels[slot].settings.clone());
    }
    assert!(
        decoded.settings == expected,
        "enrollment contains an unauthorized channel or changes opaque settings"
    );
    assert!(
        decoded.lora == plan.lora,
        "enrollment changed RF configuration"
    );
    assert!(export["qr_svg"].as_str().unwrap().contains("<svg"));
    assert_eq!(export.as_object().unwrap().len(), 3);
    Ok(())
}

#[tokio::test]
async fn public_enrollment_contains_exactly_community_settings_and_no_private_identity()
-> Result<()> {
    let f = fixture()?;
    let (status, catalog) = request(&f.app, "GET", "/public/channels", None, json!({})).await;
    assert_eq!(status, StatusCode::OK);
    let entries = catalog["channels"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["id"], "community");
    assert_eq!(entries[0]["name"], "AstralMesh");
    assert_eq!(entries[0]["slot"], 0);
    for secret in [
        "Medical",
        "Safety",
        "Staff",
        f.owner_id.as_str(),
        f.medical_id.as_str(),
    ] {
        assert!(!catalog.to_string().contains(secret));
    }
    let (status, export) = request(
        &f.app,
        "GET",
        "/public/channels/community/enrollment",
        None,
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_scoped_export(&export, &f.plan, None)?;
    assert!(!export.to_string().contains(&f.owner_token));
    assert!(!export.to_string().contains(&f.medical_id));
    for id in ["staff", "medical", "safety"] {
        let (status, denied) = request(
            &f.app,
            "GET",
            &format!("/public/channels/{id}/enrollment"),
            None,
            json!({}),
        )
        .await;
        assert!(status.is_client_error());
        assert!(denied.get("url").is_none());
        assert!(denied.get("qr_svg").is_none());
        assert!(denied.get("channel").is_none());
    }
    assert_eq!(
        request(
            &f.app,
            "GET",
            "/api/channels/staff/enrollment",
            None,
            json!({})
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(
            &f.app,
            "POST",
            "/api/messages",
            None,
            json!({"text":"anonymous community post","channel":"community"})
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert!(f.store.records()?.is_empty());
    Ok(())
}

#[tokio::test]
async fn department_grant_and_revocation_scope_catalog_and_qr_independently_of_general_staff()
-> Result<()> {
    let f = fixture()?;
    let medical_path = "/api/channels/medical/enrollment";
    assert!(
        request(
            &f.app,
            "GET",
            medical_path,
            Some(&f.medical_token),
            json!({})
        )
        .await
        .0
        .is_client_error()
    );
    let access = json!({"staff_id":f.medical_id,"grant":true});
    assert_eq!(
        request(
            &f.app,
            "POST",
            "/api/channels/medical/access",
            Some(&f.staff_token),
            access.clone()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            &f.app,
            "POST",
            "/api/channels/medical/access",
            Some(&f.owner_token),
            access
        )
        .await
        .0,
        StatusCode::OK
    );
    let (status, export) = request(
        &f.app,
        "GET",
        medical_path,
        Some(&f.medical_token),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_scoped_export(&export, &f.plan, Some(2))?;
    assert_eq!(export["channel"]["id"], "medical");
    let (status, staff_export) = request(
        &f.app,
        "GET",
        "/api/channels/staff/enrollment",
        Some(&f.staff_token),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_scoped_export(&staff_export, &f.plan, Some(1))?;
    assert!(
        request(
            &f.app,
            "GET",
            "/api/channels/safety/enrollment",
            Some(&f.medical_token),
            json!({})
        )
        .await
        .0
        .is_client_error()
    );
    let (_, catalog) = request(
        &f.app,
        "GET",
        "/api/channels",
        Some(&f.medical_token),
        json!({}),
    )
    .await;
    let ids = catalog["channels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["community", "staff", "medical"]);
    assert_eq!(
        request(
            &f.app,
            "POST",
            "/api/channels/medical/access",
            Some(&f.owner_token),
            json!({"staff_id":f.medical_id,"grant":false})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert!(
        request(
            &f.app,
            "GET",
            medical_path,
            Some(&f.medical_token),
            json!({})
        )
        .await
        .0
        .is_client_error()
    );
    assert_eq!(
        request(
            &f.app,
            "GET",
            "/api/channels/staff/enrollment",
            Some(&f.medical_token),
            json!({})
        )
        .await
        .0,
        StatusCode::OK
    );
    Ok(())
}

#[tokio::test]
async fn message_alert_and_browser_ack_boundaries_enforce_department_membership() -> Result<()> {
    let f = fixture()?;
    f.store
        .grant_channel_access(&f.owner_id, &f.medical_id, "medical")?;
    for path in ["/api/messages", "/api/alerts"] {
        let body = if path.ends_with("alerts") {
            json!({"text":"Denied private alert","channel":"medical","responder":"Medical lead","expires":now()+3600.0})
        } else {
            json!({"text":"Denied private message","channel":"medical"})
        };
        assert!(
            request(&f.app, "POST", path, Some(&f.staff_token), body)
                .await
                .0
                .is_client_error()
        );
    }
    assert!(f.store.records()?.is_empty());
    let (status, message) = request(
        &f.app,
        "POST",
        "/api/messages",
        Some(&f.medical_token),
        json!({"text":"Private medical location","channel":"medical"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(message["channel_id"], "medical");
    let (status, alert) = request(&f.app, "POST", "/api/alerts", Some(&f.medical_token), json!({"text":"Medical operational alert","channel":"medical","responder":"Medical lead","expires":now()+3600.0})).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = alert["id"].as_str().unwrap();
    let ack_path = format!("/api/alerts/{id}/ack");
    assert!(
        request(&f.app, "POST", &ack_path, Some(&f.staff_token), json!({}))
            .await
            .0
            .is_client_error()
    );
    assert!(f.store.record(id)?.unwrap().acknowledgements.is_empty());
    let (status, ack) = request(&f.app, "POST", &ack_path, Some(&f.medical_token), json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ack["actor"], f.medical_id);
    let (_, staff_journal) = request(
        &f.app,
        "GET",
        "/api/messages",
        Some(&f.staff_token),
        json!({}),
    )
    .await;
    assert!(staff_journal["messages"].as_array().unwrap().is_empty());
    let (_, medical_journal) = request(
        &f.app,
        "GET",
        "/api/messages",
        Some(&f.medical_token),
        json!({}),
    )
    .await;
    assert_eq!(medical_journal["messages"].as_array().unwrap().len(), 2);
    f.store
        .revoke_channel_access(&f.owner_id, &f.medical_id, "medical")?;
    assert!(
        request(&f.app, "POST", &ack_path, Some(&f.medical_token), json!({}))
            .await
            .0
            .is_client_error()
    );
    let (_, revoked_journal) = request(
        &f.app,
        "GET",
        "/api/messages",
        Some(&f.medical_token),
        json!({}),
    )
    .await;
    assert!(revoked_journal["messages"].as_array().unwrap().is_empty());
    let (_, public) = request(&f.app, "GET", "/public/announcements", None, json!({})).await;
    assert!(public["announcements"].as_array().unwrap().is_empty());
    Ok(())
}

#[tokio::test]
async fn cross_channel_radio_acks_cannot_ack_staff_alerts_or_create_outbound_loops() -> Result<()> {
    let f = fixture()?;
    let owner = f.store.authenticate(&f.owner_token)?.unwrap();
    let alert = f.store.create_as(
        &owner,
        NewRecord::alert(
            MessageText::new("Staff alert")?,
            &owner.id,
            StaffName::new("Lead")?,
            now() + 3600.0,
            true,
            now(),
        )?
        .on_channel("staff")?,
    )?;
    for (packet, slot) in [("community-ack", 0), ("medical-ack", 2), ("safety-ack", 3)] {
        assert!(
            f.store
                .receive(packet, "!radio", &format!("ACK {}", alert.id), slot)?
        );
        assert!(
            f.store
                .record(&alert.id)?
                .unwrap()
                .acknowledgements
                .is_empty()
        );
    }
    assert!(
        f.store
            .receive("staff-ack", "!radio", &format!("ACK {}", alert.id), 1)?
    );
    assert!(
        !f.store
            .receive("staff-ack", "!radio", &format!("ACK {}", alert.id), 1)?
    );
    let restored = f.store.record(&alert.id)?.unwrap();
    assert_eq!(restored.acknowledgements.len(), 1);
    assert_eq!(
        restored.acknowledgements[0].source,
        AcknowledgementSource::Radio
    );
    assert_eq!(restored.acknowledgements[0].channel, 1);
    assert_eq!(restored.state, TransmissionState::Queued);
    assert!(restored.delivery.transport_confirmed_at.is_none());
    assert!(f.store.receive(
        "community-text",
        "!attendee",
        "Community radio conversation",
        0
    )?);
    assert_eq!(f.store.pending()?.len(), 1);
    assert!(f.store.public()?.is_empty());
    assert!(!f.store.receive("unknown-slot", "!radio", "unmapped", 7)?);
    Ok(())
}

#[tokio::test]
async fn queue_bindings_survive_restart_and_snapshot_recovery_requires_new_radio_verification()
-> Result<()> {
    let f = fixture()?;
    f.store
        .grant_channel_access(&f.owner_id, &f.medical_id, "medical")?;
    let actor = f.store.authenticate(&f.medical_token)?.unwrap();
    let queued = f.store.create_as(
        &actor,
        NewRecord::message(MessageText::new("Pending medical send")?, &actor.id)?
            .on_channel("medical")?,
    )?;
    let fingerprint = f.store.channel_metadata("medical")?.unwrap().fingerprint;
    assert_eq!(
        queued.channel_fingerprint.as_deref(),
        Some(fingerprint.as_str())
    );
    let restarted = EventStore::new(f.directory.path(), 100, true)?;
    let pending = restarted.pending()?;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, queued.id);
    assert_eq!(pending[0].channel_id, "medical");
    assert_eq!(
        pending[0].channel_fingerprint.as_deref(),
        Some(fingerprint.as_str())
    );
    assert!(restarted.validate_binding("medical", &fingerprint).is_ok());
    let before_export = restarted.enrollment("medical", Some(&actor))?.url;
    let backup = f.directory.path().join("snapshot.sqlite");
    restarted.backup(&backup)?;
    let spare = f.directory.path().join("spare");
    restore_snapshot(&backup, &spare)?;
    let restored = EventStore::new(&spare, 100, true)?;
    assert!(restored.authenticate(&f.medical_token)?.is_some());
    assert!(restored.can_access(&actor, "medical")?);
    assert!(restored.enrollment("medical", Some(&actor)).is_err());
    assert_eq!(
        restored.record(&queued.id)?.unwrap().state,
        TransmissionState::Uncertain
    );
    assert!(restored.pending()?.is_empty());
    assert!(
        restored
            .radio_channels()?
            .iter()
            .all(|channel| !channel.metadata.verified)
    );
    assert!(restored.validate_binding("medical", &fingerprint).is_err());
    assert!(!restored.receive(
        "unverified-radio",
        "!radio",
        "Not trusted before read-back",
        2
    )?);
    let observed = restored
        .radio_channels()?
        .into_iter()
        .map(|channel| (channel.metadata.slot, channel.metadata.fingerprint))
        .collect::<Vec<_>>();
    assert!(restored.mark_deployment_verified(&observed[..3]).is_err());
    let mut changed = observed.clone();
    changed[2].1 = "wrong-settings-fingerprint".into();
    assert!(restored.mark_deployment_verified(&changed).is_err());
    assert!(
        restored
            .radio_channels()?
            .iter()
            .all(|channel| !channel.metadata.verified)
    );
    restored.mark_deployment_verified(&observed)?;
    assert!(restored.validate_binding("medical", &fingerprint).is_ok());
    assert_eq!(
        restored.enrollment("medical", Some(&actor))?.url,
        before_export
    );
    let fresh = restored.create_as(
        &actor,
        NewRecord::message(MessageText::new("Fresh after read-back")?, &actor.id)?
            .on_channel("medical")?,
    )?;
    assert_eq!(restored.pending()?[0].id, fresh.id);
    assert_eq!(
        restored.record(&queued.id)?.unwrap().state,
        TransmissionState::Uncertain
    );
    Ok(())
}

#[tokio::test]
async fn malformed_and_oversized_channel_requests_fail_before_mutating_membership_or_journal()
-> Result<()> {
    let f = fixture()?;
    for body in [
        "{".to_owned(),
        json!({"text":"Message","channel":"../medical"}).to_string(),
        json!({"text":"Message","channel":1}).to_string(),
        json!({"text":"Message","channel":"medical","settings":[1,2,3]}).to_string(),
    ] {
        let (status, response) =
            raw_request(&f.app, "POST", "/api/messages", Some(&f.owner_token), body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(response.get("channel").is_none());
        assert!(response.get("url").is_none());
    }
    for body in [
        json!({"staff_id":"../outside","grant":true}),
        json!({"staff_id":f.medical_id,"grant":"true"}),
        json!({"staff_id":f.medical_id,"grant":true,"scope":"community"}),
    ] {
        assert_eq!(
            request(
                &f.app,
                "POST",
                "/api/channels/medical/access",
                Some(&f.owner_token),
                body
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    let oversized = json!({"staff_id":"x".repeat(20000),"grant":true});
    let (status, _) = request(
        &f.app,
        "POST",
        "/api/channels/medical/access",
        Some(&f.owner_token),
        oversized,
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    let identity = f.store.authenticate(&f.medical_token)?.unwrap();
    assert!(!f.store.can_access(&identity, "medical")?);
    assert!(f.store.records()?.is_empty());
    let (status, public) = request(&f.app, "GET", "/public/channels", None, json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(public["channels"].as_array().unwrap().len(), 1);
    Ok(())
}

fn insert_received_flood(store: &EventStore, channel: &str, count: usize) -> Result<()> {
    let fingerprint = store.channel_metadata(channel)?.unwrap().fingerprint;
    let mut db = store.connect()?;
    let transaction = db.transaction()?;
    let created = now() + 100.0;
    for index in 0..count {
        transaction.execute(
            "INSERT INTO records(id,text,kind,author,created,state,channel_id,channel_fingerprint) VALUES(?,?,'message','radio:!flood',?,'received',?,?)",
            rusqlite::params![format!("FLOOD-{channel}-{index}"), format!("{channel} flood {index}"), created + index as f64, channel, fingerprint],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

#[tokio::test]
async fn inaccessible_channel_flood_cannot_hide_authorized_department_history() -> Result<()> {
    let f = fixture()?;
    f.store
        .grant_channel_access(&f.owner_id, &f.medical_id, "medical")?;
    let actor = f.store.authenticate(&f.medical_token)?.unwrap();
    let older = f.store.create_as(
        &actor,
        NewRecord::message(MessageText::new("Older medical conversation")?, &actor.id)?
            .on_channel("medical")?,
    )?;
    insert_received_flood(&f.store, "safety", 501)?;
    let records = f.store.records_for(&actor)?;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].id, older.id);
    let (status, journal) = request(
        &f.app,
        "GET",
        "/api/messages",
        Some(&f.medical_token),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(journal["messages"].as_array().unwrap().len(), 1);
    assert_eq!(journal["messages"][0]["id"], older.id);
    assert!(!journal.to_string().contains("safety flood"));
    Ok(())
}

#[tokio::test]
async fn community_flood_preserves_private_alert_visibility_and_selected_channel_queries()
-> Result<()> {
    let f = fixture()?;
    f.store
        .grant_channel_access(&f.owner_id, &f.medical_id, "medical")?;
    let actor = f.store.authenticate(&f.medical_token)?.unwrap();
    let alert = f.store.create_as(
        &actor,
        NewRecord::alert(
            MessageText::new("Private medical alert survives community storm")?,
            &actor.id,
            StaffName::new("Medical lead")?,
            now() + 3600.0,
            true,
            now(),
        )?
        .on_channel("medical")?,
    )?;
    insert_received_flood(&f.store, "community", 501)?;
    let records = f.store.records_for(&actor)?;
    assert!(records.len() <= 500);
    assert!(records.iter().any(|record| record.id == alert.id));
    let (status, journal) = request(
        &f.app,
        "GET",
        "/api/messages",
        Some(&f.medical_token),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        journal["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|record| record["id"] == alert.id)
    );
    let (status, selected) = request(
        &f.app,
        "GET",
        "/api/messages?channel=medical",
        Some(&f.medical_token),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(selected["messages"].as_array().unwrap().len(), 1);
    assert_eq!(selected["messages"][0]["id"], alert.id);
    let (status, denied) = request(
        &f.app,
        "GET",
        "/api/messages?channel=safety",
        Some(&f.medical_token),
        json!({}),
    )
    .await;
    assert!(status.is_client_error());
    assert!(denied.get("messages").is_none());
    let (status, community) = request(
        &f.app,
        "GET",
        "/api/messages?channel=community",
        Some(&f.medical_token),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(community["messages"].as_array().unwrap().len(), 500);
    assert!(
        community["messages"]
            .as_array()
            .unwrap()
            .iter()
            .all(|record| record["channel_id"] == "community")
    );
    Ok(())
}
