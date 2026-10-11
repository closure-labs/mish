use anyhow::Result;
use mish::{
    appliance::{EventStore, now},
    domain::{
        AcknowledgementSource, MessageText, NewRecord, Role, StaffName, TransmissionState,
        TransmissionUpdate,
    },
};
use rusqlite::params;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    sync::{Arc, Barrier},
    thread,
};

fn message(text: &str) -> NewRecord {
    NewRecord::message(MessageText::new(text).unwrap(), "staff").unwrap()
}
fn alert(text: &str) -> NewRecord {
    NewRecord::alert(
        MessageText::new(text).unwrap(),
        "staff",
        StaffName::new("Gate lead").unwrap(),
        now() + 3600.0,
        true,
        now(),
    )
    .unwrap()
}

#[test]
fn identities_are_private_hashed_revocable_and_last_operator_is_retained() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let store = EventStore::new(temporary.path(), 100, true)?;
    let owner = store.enroll(" Owner ", Role::Operator, true)?;
    let staff = store.enroll("Staff", Role::Staff, false)?;
    assert_eq!(owner.name, "Owner");
    assert_eq!(owner.token.len(), 43);
    assert_eq!(
        store.authenticate(&owner.token)?.unwrap().role,
        Role::Operator
    );
    assert!(store.authenticate("invalid")?.is_none());
    let hash: String =
        store
            .connect()?
            .query_row("SELECT hash FROM staff WHERE id=?", [&staff.id], |row| {
                row.get(0)
            })?;
    assert_eq!(hash.len(), 64);
    assert!(
        !fs::read(&store.path)?
            .windows(staff.token.len())
            .any(|window| window == staff.token.as_bytes())
    );
    assert!(store.enroll("Other", Role::Operator, true).is_err());
    assert!(store.revoke(&owner.id).is_err());
    store.revoke(&staff.id)?;
    assert!(store.authenticate(&staff.token)?.is_none());
    let replacement = store.enroll("Replacement", Role::Operator, false)?;
    store.revoke(&owner.id)?;
    assert!(store.authenticate(&replacement.token)?.is_some());
    assert_eq!(
        fs::metadata(&store.path)?.permissions().mode() & 0o777,
        0o600
    );
    Ok(())
}

#[test]
fn simultaneous_commissioning_and_queue_acceptance_are_serialized() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let store = EventStore::new(temporary.path(), 1, true)?;
    let barrier = Arc::new(Barrier::new(8));
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let store = store.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                store.enroll("Operator", Role::Operator, true).is_ok()
            })
        })
        .collect();
    assert_eq!(
        workers
            .into_iter()
            .filter_map(|worker| worker.join().ok())
            .filter(|accepted| *accepted)
            .count(),
        1
    );
    let barrier = Arc::new(Barrier::new(8));
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let store = store.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                store.create(message("only one durable slot")).is_ok()
            })
        })
        .collect();
    assert_eq!(
        workers
            .into_iter()
            .filter_map(|worker| worker.join().ok())
            .filter(|accepted| *accepted)
            .count(),
        1
    );
    assert_eq!(store.pending()?.len(), 1);
    Ok(())
}

#[test]
fn staff_journal_and_ack_history_survive_restart_and_never_enter_public_projection() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let store = EventStore::new(temporary.path(), 100, true)?;
    let staff = store.enroll("Staff", Role::Staff, false)?;
    let outgoing = store.create(NewRecord::message(
        MessageText::new("private staff location")?,
        &staff.id,
    )?)?;
    let urgent = store.create(alert("Private gate alert"))?;
    let ack = store.acknowledge(
        &urgent.id,
        &staff.id,
        AcknowledgementSource::Browser,
        None,
        None,
    )?;
    let duplicate = store.acknowledge(
        &urgent.id,
        &staff.id,
        AcknowledgementSource::Browser,
        None,
        None,
    )?;
    assert_eq!(ack.created, duplicate.created);
    assert_eq!(ack.channel, -1);
    assert_eq!(ack.node, "");
    assert!(store.public()?.is_empty());
    let published = store.publish("Water station open")?;
    let restarted = EventStore::new(temporary.path(), 100, true)?;
    assert_eq!(restarted.pending()?.len(), 2);
    assert!(restarted.authenticate(&staff.token)?.is_some());
    assert_eq!(
        restarted
            .record(&urgent.id)?
            .unwrap()
            .acknowledgements
            .len(),
        1
    );
    assert_eq!(
        restarted.record(&outgoing.id)?.unwrap().author_name,
        "Staff"
    );
    let public = serde_json::to_value(restarted.public()?)?;
    assert_eq!(public[0]["id"], published.id);
    assert_eq!(public[0].as_object().unwrap().len(), 3);
    assert!(!public.to_string().contains(&staff.id));
    assert!(!public.to_string().contains("private"));
    let serialized = serde_json::to_value(restarted.record(&urgent.id)?.unwrap())?;
    assert!(serialized["delivery"]["persisted"].as_bool().unwrap());
    for field in ["radioSubmittedAt", "transportConfirmedAt", "uncertainAt"] {
        assert!(serialized["delivery"].get(field).is_some());
    }
    Ok(())
}

#[test]
fn atomic_claims_and_typed_transitions_do_not_report_false_success() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let store = EventStore::new(temporary.path(), 100, true)?;
    let item = store.create(message("Outgoing"))?;
    assert!(!store.transport_confirmation("not-yet-submitted")?);
    assert!(!store.transition(
        &item.id,
        &TransmissionUpdate::Submitted {
            packet_id: "invalid".into()
        }
    )?);
    let barrier = Arc::new(Barrier::new(8));
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let store = store.clone();
            let id = item.id.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                store.transition(&id, &TransmissionUpdate::Claim).unwrap()
            })
        })
        .collect();
    assert_eq!(
        workers
            .into_iter()
            .filter_map(|worker| worker.join().ok())
            .filter(|claimed| *claimed)
            .count(),
        1
    );
    assert!(store.transition(
        &item.id,
        &TransmissionUpdate::Submitted {
            packet_id: "packet-1".into()
        }
    )?);
    let submitted = store.record(&item.id)?.unwrap();
    assert_eq!(submitted.state, TransmissionState::Submitted);
    assert!(submitted.delivery.radio_submitted_at.is_some());
    assert!(submitted.delivery.transport_confirmed_at.is_none());
    assert!(submitted.acknowledgements.is_empty());
    assert!(store.transport_confirmation("packet-1")?);
    assert!(!store.transport_confirmation("packet-1")?);
    assert!(
        store
            .record(&item.id)?
            .unwrap()
            .delivery
            .transport_confirmed_at
            .is_some()
    );
    assert!(!store.transition(&item.id, &TransmissionUpdate::Claim)?);
    assert!(!store.transition("NOTFOUND", &TransmissionUpdate::Claim)?);
    Ok(())
}

#[test]
fn ordinary_local_handles_do_not_recover_active_sends_but_startup_quarantines_crashes() -> Result<()>
{
    let temporary = tempfile::tempdir()?;
    let store = EventStore::new(temporary.path(), 100, true)?;
    let sending = store.create(message("Interrupted send"))?;
    let queued = store.create(message("Safe pending"))?;
    store.transition(&sending.id, &TransmissionUpdate::Claim)?;
    let local = EventStore::new(temporary.path(), 100, false)?;
    assert_eq!(
        local.record(&sending.id)?.unwrap().state,
        TransmissionState::Sending
    );
    let restarted = EventStore::new(temporary.path(), 100, true)?;
    let unknown = restarted.record(&sending.id)?.unwrap();
    assert_eq!(unknown.state, TransmissionState::Uncertain);
    assert!(unknown.delivery.uncertain_at.is_some());
    assert!(unknown.delivery.radio_submitted_at.is_none());
    assert!(!restarted.transition(&sending.id, &TransmissionUpdate::Claim)?);
    assert_eq!(restarted.pending()?.len(), 1);
    assert_eq!(restarted.pending()?[0].id, queued.id);
    Ok(())
}

#[test]
fn duplicate_radio_packets_and_radio_acks_retain_provenance_without_outbox_loops() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let store = EventStore::new(temporary.path(), 100, true)?;
    let urgent = store.create(alert("Check the gate"))?;
    assert!(store.receive("rx-1", "!radio", "hello from radio", 2)?);
    assert!(!store.receive("rx-1", "!radio", "duplicate", 2)?);
    assert!(store.receive("rx-1", "!another", "same packet from another node", 2)?);
    assert_eq!(store.pending()?.len(), 1);
    for item in store
        .records()?
        .iter()
        .filter(|record| record.state == TransmissionState::Received)
    {
        assert!(!store.transition(&item.id, &TransmissionUpdate::Claim)?);
    }
    let text = format!("ACK {}", urgent.id.to_lowercase());
    assert!(store.receive("rx-ack", "!radio", &text, 2)?);
    assert!(!store.receive("rx-ack", "!radio", &text, 2)?);
    assert!(store.receive("rx-ack2", "!radio", &text, 2)?);
    let record = store.record(&urgent.id)?.unwrap();
    assert_eq!(record.acknowledgements.len(), 1);
    assert_eq!(
        record.acknowledgements[0].source,
        AcknowledgementSource::Radio
    );
    assert_eq!(record.acknowledgements[0].actor, "!radio");
    assert_eq!(record.acknowledgements[0].node, "!radio");
    assert_eq!(record.acknowledgements[0].channel, 2);
    assert_eq!(record.state, TransmissionState::Queued);
    assert!(record.delivery.transport_confirmed_at.is_none());
    assert!(store.public()?.is_empty());
    Ok(())
}

#[test]
fn expired_alerts_cannot_be_claimed_or_newly_acknowledged() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let store = EventStore::new(temporary.path(), 100, true)?;
    let expired = store.create(alert("Expires"))?;
    store
        .connect()?
        .execute("UPDATE records SET expires=0 WHERE id=?", [&expired.id])?;
    assert!(!store.transition(&expired.id, &TransmissionUpdate::Claim)?);
    assert_eq!(
        store.record(&expired.id)?.unwrap().state,
        TransmissionState::Expired
    );
    assert!(
        store
            .acknowledge(
                &expired.id,
                "staff",
                AcknowledgementSource::Browser,
                None,
                None
            )
            .is_err()
    );
    store.receive("late-ack", "!radio", &format!("ACK {}", expired.id), 0)?;
    assert!(
        store
            .record(&expired.id)?
            .unwrap()
            .acknowledgements
            .is_empty()
    );
    let another = store.create(alert("Pending expiry"))?;
    store
        .connect()?
        .execute("UPDATE records SET expires=0 WHERE id=?", [&another.id])?;
    assert!(store.pending()?.is_empty());
    let message = store.create(message("Plain message"))?;
    assert!(
        store
            .acknowledge(
                &message.id,
                "staff",
                AcknowledgementSource::Browser,
                None,
                None
            )
            .is_err()
    );
    for expiration in [f64::NAN, f64::INFINITY, now() - 1.0, now() + 86401.0] {
        assert!(
            NewRecord::alert(
                MessageText::new("Invalid")?,
                "staff",
                StaffName::new("Lead")?,
                expiration,
                false,
                now()
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn retention_removes_private_public_dedup_and_ack_rows_consistently() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let store = EventStore::new(temporary.path(), 100, true)?;
    let urgent = store.create(alert("Old"))?;
    store.acknowledge(
        &urgent.id,
        "staff",
        AcknowledgementSource::Browser,
        None,
        None,
    )?;
    store.publish("Old public")?;
    store.receive("old-rx", "!radio", "Old radio", 0)?;
    store.connect()?.execute_batch("UPDATE records SET created=0; UPDATE announcements SET published=0; UPDATE received SET created=0;")?;
    assert_eq!(store.prune(7)?, 5);
    assert!(store.records()?.is_empty());
    assert!(store.public()?.is_empty());
    assert_eq!(
        store
            .connect()?
            .query_row("SELECT count(*) FROM acks", [], |row| row.get::<_, i64>(0))?,
        0
    );
    assert!(store.prune(0).is_err());
    Ok(())
}

#[test]
fn preexisting_python_schema_hashes_and_rows_remain_compatible() -> Result<()> {
    use sha2::{Digest, Sha256};
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("events.sqlite");
    let db = rusqlite::Connection::open(&path)?;
    // Legacy schema intentionally lacks the delivery timestamp columns: startup migrates in their original order.
    db.execute_batch("CREATE TABLE staff(id TEXT PRIMARY KEY,name TEXT,role TEXT,hash TEXT UNIQUE);
        CREATE TABLE records(id TEXT PRIMARY KEY,text TEXT,kind TEXT,author TEXT,created REAL,expires REAL,responder TEXT,state TEXT,packet_id TEXT);
        CREATE TABLE acks(record_id TEXT,actor TEXT,source TEXT,node TEXT,channel INTEGER,created REAL,UNIQUE(record_id,actor,source,node,channel));
        CREATE TABLE received(packet_id TEXT,node TEXT,channel INTEGER,created REAL,UNIQUE(packet_id,node,channel));
        CREATE TABLE announcements(id TEXT PRIMARY KEY,text TEXT,published REAL);")?;
    db.execute(
        "INSERT INTO staff VALUES('legacy-id','Old operator','operator',?)",
        [hex::encode(Sha256::digest(b"legacy-token"))],
    )?;
    db.execute("INSERT INTO records VALUES('DEADBEEF','Legacy message','message','legacy-id',?,NULL,NULL,'queued',NULL)", [now()])?;
    db.execute(
        "INSERT INTO announcements VALUES('legacy-public','Gates open',?)",
        [now()],
    )?;
    drop(db);
    let store = EventStore::new(temporary.path(), 100, true)?;
    assert_eq!(store.authenticate("legacy-token")?.unwrap().id, "legacy-id");
    let record = store.record("DEADBEEF")?.unwrap();
    assert_eq!(record.author_name, "Old operator");
    assert_eq!(record.state, TransmissionState::Queued);
    assert!(record.submitted_at.is_none());
    assert_eq!(store.public()?[0].text, "Gates open");
    store.connect()?.execute(
        "INSERT INTO acks VALUES(?,?,?,?,?,?)",
        params![record.id, "legacy-id", "browser", "", -1, now()],
    )?;
    assert_eq!(store.record("DEADBEEF")?.unwrap().acknowledgements.len(), 1);
    Ok(())
}

#[test]
fn bounded_journal_listing_batches_matching_acknowledgements_and_author_names() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let store = EventStore::new(temporary.path(), 1000, false)?;
    let staff = store.enroll("Named staff", Role::Staff, false)?;
    let mut db = store.connect()?;
    let transaction = db.transaction()?;
    for index in 0..501 {
        let id = format!("ALERT{index:04}");
        transaction.execute("INSERT INTO records(id,text,kind,author,created,expires,responder,state) VALUES(?,'Alert','alert',?,?,?,'Lead','queued')", params![id, staff.id, index, now()+3600.0])?;
        transaction.execute(
            "INSERT INTO acks VALUES(?,?,'browser','',-1,?)",
            params![id, format!("responder-{index}"), now()],
        )?;
    }
    transaction.commit()?;
    let journal = store.records()?;
    assert_eq!(journal.len(), 500);
    assert_eq!(journal[0].id, "ALERT0500");
    assert_eq!(journal[499].id, "ALERT0001");
    for (offset, record) in journal.iter().enumerate() {
        assert_eq!(record.author_name, "Named staff");
        assert_eq!(record.acknowledgements.len(), 1);
        assert_eq!(record.acknowledgements[0].record_id, record.id);
        assert_eq!(
            record.acknowledgements[0].actor,
            format!("responder-{}", 500 - offset)
        );
        assert_eq!(record.state, TransmissionState::Queued);
    }
    Ok(())
}
