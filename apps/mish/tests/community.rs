use mish::{
    appliance::EventStore,
    community::{Action, Credentials, Manage},
    domain::{RecordId, Role, Text},
};
use serde_json::Value;
fn call(s: &EventStore, c: &Credentials, a: Action) -> Value {
    s.community(c.clone(), a).unwrap()
}
fn setup() -> (tempfile::TempDir, EventStore, Credentials, String) {
    let d = tempfile::tempdir().unwrap();
    let s = EventStore::new(d.path(), 100, false).unwrap();
    let op = s.enroll("Operator", Role::Operator, true).unwrap();
    let c = Credentials {
        staff: Some(op.token),
        guest: None,
    };
    call(
        &s,
        &c,
        Action::Manage(Manage::Configure {
            enabled: true,
            enroll_per_hour: 120,
            posts_per_minute: 600,
            queue_limit: 10,
        }),
    );
    let room = call(
        &s,
        &c,
        Action::Manage(Manage::Create {
            name: Text::new("Lobby").unwrap(),
            public: true,
            channel: None,
        }),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    (d, s, c, room)
}
fn guest(s: &EventStore) -> Credentials {
    let v = call(s, &Credentials::default(), Action::Enroll("<Guest>".into()));
    Credentials {
        guest: Some(v["token"].as_str().unwrap().into()),
        staff: None,
    }
}
#[test]
fn opt_in_membership_idempotency_moderation_and_revocation() {
    let (d, s, op, room) = setup();
    let g = guest(&s);
    assert!(s.authenticate(g.guest.as_ref().unwrap()).unwrap().is_none());
    assert!(
        s.community(
            g.clone(),
            Action::Post(room.clone(), "first".into(), "hello".into())
        )
        .is_err()
    );
    call(&s, &g, Action::Join(room.clone()));
    let accepted = call(
        &s,
        &g,
        Action::Post(room.clone(), "first".into(), "hello".into()),
    );
    assert_eq!(
        accepted,
        call(
            &s,
            &g,
            Action::Post(room.clone(), "first".into(), "hello".into())
        )
    );
    assert!(
        s.community(
            g.clone(),
            Action::Post(room.clone(), "first".into(), "changed".into())
        )
        .is_err()
    );
    assert!(s.pending().unwrap().is_empty());
    assert!(
        s.community(g.clone(), Action::Manage(Manage::List))
            .is_err()
    );
    call(
        &s,
        &op,
        Action::Manage(Manage::Hide {
            entry: accepted["cursor"].as_i64().unwrap(),
        }),
    );
    let messages = call(&s, &g, Action::Messages(room.clone(), 0, 0));
    assert!(messages["entries"][0]["text"].is_null());
    let backup = d.path().join("snapshot");
    s.backup(&backup).unwrap();
    let restored = d.path().join("restored");
    mish::recovery::restore_snapshot(&backup, &restored).unwrap();
    let recovered = EventStore::new(&restored, 100, true).unwrap();
    assert_eq!(
        accepted,
        call(
            &recovered,
            &g,
            Action::Post(room.clone(), "first".into(), "hello".into())
        )
    );
    let id = call(&s, &op, Action::Manage(Manage::List))["guests"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    call(
        &s,
        &op,
        Action::Manage(Manage::Revoke {
            guest: RecordId::new(id).unwrap(),
        }),
    );
    assert!(s.community(g, Action::Messages(room, 0, 0)).is_err());
    call(
        &s,
        &op,
        Action::Manage(Manage::Configure {
            enabled: false,
            enroll_per_hour: 120,
            posts_per_minute: 30,
            queue_limit: 10,
        }),
    );
    assert!(
        s.community(Credentials::default(), Action::Enroll("new".into()))
            .is_err()
    );
}
#[test]
fn fifty_sessions_durable_local_history() {
    let (_d, s, _op, room) = setup();
    let guests: Vec<_> = (0..50).map(|_| guest(&s)).collect();
    for g in &guests {
        call(&s, g, Action::Join(room.clone()));
    }
    for i in 0..500 {
        call(
            &s,
            &guests[i % 50],
            Action::Post(room.clone(), format!("key-{i}"), format!("message {i}")),
        );
    }
    let mut cursor = 0;
    let mut count = 0;
    loop {
        let page = call(&s, &guests[0], Action::Messages(room.clone(), cursor, 0));
        let n = page["entries"].as_array().unwrap().len();
        if n == 0 {
            break;
        }
        count += n;
        cursor = page["cursor"].as_i64().unwrap();
    }
    assert_eq!(count, 500);
    assert!(s.pending().unwrap().is_empty());
}
#[test]
fn mapped_rooms_are_scoped_atomic_bounded_and_do_not_echo() {
    use mish::{
        channel_store::{ChannelPlan, ChannelScope, PlannedChannel},
        channels,
        domain::{MessageText, NewRecord},
    };
    let (_d, s, op, _) = setup();
    let plan = ChannelPlan {
        channels: vec![
            PlannedChannel {
                id: "community".into(),
                scope: ChannelScope::Community,
                slot: 0,
                settings: channels::generated("Community").unwrap().settings,
            },
            PlannedChannel {
                id: "staff".into(),
                scope: ChannelScope::Staff,
                slot: 1,
                settings: channels::generated("Staff").unwrap().settings,
            },
        ],
        lora: Some(vec![8, 1]),
    };
    s.install_channel_plan(&plan).unwrap();
    let observed = s
        .radio_channels()
        .unwrap()
        .into_iter()
        .map(|c| (c.metadata.slot, c.metadata.fingerprint))
        .collect::<Vec<_>>();
    s.mark_deployment_verified(&observed).unwrap();
    // History predating a room must never be published by mapping that channel.
    s.receive("old", "node", "old history", 0).unwrap();
    let create = |public, channel: &str| {
        Action::Manage(Manage::Create {
            name: Text::new("Mapped").unwrap(),
            public,
            channel: Some(RecordId::new(channel).unwrap()),
        })
    };
    assert!(s.community(op.clone(), create(true, "staff")).is_err());
    let private = call(&s, &op, create(false, "staff"))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let room = call(&s, &op, create(true, "community"))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(s.community(op.clone(), create(true, "community")).is_err());
    let g = guest(&s);
    assert!(
        s.community(g.clone(), Action::Join(private.clone()))
            .is_err()
    );
    assert!(!call(&s, &g, Action::Rooms).to_string().contains(&private));
    call(&s, &g, Action::Join(room.clone()));
    assert!(
        call(&s, &g, Action::Messages(room.clone(), 0, 0))["entries"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        s.community(
            g.clone(),
            Action::Post(room.clone(), "long".into(), "a".repeat(160))
        )
        .is_err()
    );
    assert!(s.pending().unwrap().is_empty());
    for i in 0..10 {
        call(
            &s,
            &g,
            Action::Post(room.clone(), format!("mapped-{i}"), "hello".into()),
        );
    }
    assert!(
        s.community(
            g.clone(),
            Action::Post(room.clone(), "overflow".into(), "hello".into())
        )
        .is_err()
    );
    // A browser that previously enrolled as a guest can subsequently use staff authority.
    let mut mixed = g.clone();
    mixed.staff = op.staff.clone();
    call(&s, &mixed, Action::Join(room.clone()));
    call(
        &s,
        &mixed,
        Action::Post(
            room.clone(),
            "staff-with-cookie".into(),
            "staff capacity".into(),
        ),
    );
    let staff = s
        .create(NewRecord::message(MessageText::new("priority").unwrap(), "operator").unwrap())
        .unwrap();
    assert!(
        s.pending()
            .unwrap()
            .iter()
            .take(2)
            .any(|r| r.id == staff.id)
    );
    assert!(s.receive("old", "node2", "new history", 0).unwrap());
    assert!(!s.receive("old", "node2", "new history", 0).unwrap());
    assert_eq!(s.pending().unwrap().len(), 12);
    let page = call(&s, &g, Action::Messages(room.clone(), 0, 0));
    assert_eq!(page["entries"].as_array().unwrap().len(), 12);
    assert!(!page.to_string().contains("old history"));
    s.connect()
        .unwrap()
        .execute("UPDATE radio_channels SET verified=0", [])
        .unwrap();
    assert!(
        s.community(g, Action::Post(room, "stale".into(), "hello".into()))
            .is_err()
    );
}

#[test]
fn moderation_has_an_independent_bounded_cursor() {
    let (_d, s, op, room) = setup();
    let g = guest(&s);
    call(&s, &g, Action::Join(room.clone()));
    let accepted = call(
        &s,
        &g,
        Action::Post(room.clone(), "moderated".into(), "text".into()),
    );
    let entry = accepted["cursor"].as_i64().unwrap();
    for _ in 0..120 {
        call(&s, &op, Action::Manage(Manage::Hide { entry }));
    }
    let first = call(&s, &g, Action::Messages(room.clone(), entry, 0));
    assert_eq!(first["hidden"].as_array().unwrap().len(), 100);
    let second = call(
        &s,
        &g,
        Action::Messages(
            room.clone(),
            entry,
            first["moderation_cursor"].as_i64().unwrap(),
        ),
    );
    assert_eq!(second["hidden"].as_array().unwrap().len(), 20);
    assert!(
        call(
            &s,
            &g,
            Action::Messages(room, entry, second["moderation_cursor"].as_i64().unwrap())
        )["hidden"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
