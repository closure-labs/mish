use anyhow::Result;
use mish::{
    appliance::{EventStore, now},
    domain::{
        AcknowledgementSource, MessageText, NewRecord, Role, StaffName, TransmissionState,
        TransmissionUpdate,
    },
    recovery::restore_snapshot,
};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
};

fn message(text: &str) -> NewRecord {
    NewRecord::message(MessageText::new(text).unwrap(), "staff").unwrap()
}
fn assert_no_installed_state(path: &Path) -> Result<()> {
    assert!(!path.join("events.sqlite").exists());
    if path.exists() {
        assert_eq!(fs::read_dir(path)?.count(), 0);
    }
    Ok(())
}

#[test]
fn snapshot_preserves_credentials_public_and_ack_history_but_quarantines_pending_radio_work()
-> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path();
    let original = EventStore::new(&root.join("original"), 100, true)?;
    let staff = original.enroll("Staff", Role::Staff, false)?;
    let queued = original.create(message("Queued at snapshot time"))?;
    let sending = original.create(message("Interrupted send"))?;
    original.transition(&sending.id, &TransmissionUpdate::Claim)?;
    let alert = original.create(NewRecord::alert(
        MessageText::new("Private gate alert")?,
        "staff",
        StaffName::new("Gate lead")?,
        now() + 3600.0,
        true,
        now(),
    )?)?;
    original.transition(&alert.id, &TransmissionUpdate::Claim)?;
    original.transition(
        &alert.id,
        &TransmissionUpdate::Submitted {
            packet_id: "packet-alert".into(),
        },
    )?;
    original.transport_confirmation("packet-alert")?;
    original.acknowledge(
        &alert.id,
        &staff.id,
        AcknowledgementSource::Browser,
        None,
        None,
    )?;
    original.receive("rx-ack", "!radio", &format!("ACK {}", alert.id), 2)?;
    let published = original.publish("Water station open")?;
    let backup = root.join("backup.sqlite");
    original.backup(&backup)?;
    let bytes = fs::read(&backup)?;
    // The original sends this after capture, so the spare cannot safely replay its snapshot queue.
    original.transition(&queued.id, &TransmissionUpdate::Claim)?;
    original.transition(
        &queued.id,
        &TransmissionUpdate::Submitted {
            packet_id: "later-packet".into(),
        },
    )?;
    let spare = root.join("spare");
    let report = restore_snapshot(&backup, &spare)?;
    assert_eq!(report.quarantined, 2);
    assert_eq!(report.source_sha256, hex::encode(Sha256::digest(&bytes)));
    assert_eq!(fs::read(&backup)?, bytes);
    assert_eq!(
        fs::metadata(&report.restored)?.permissions().mode() & 0o777,
        0o600
    );
    let restored = EventStore::new(&spare, 100, true)?;
    assert_eq!(restored.authenticate(&staff.token)?.unwrap().id, staff.id);
    assert!(restored.pending()?.is_empty());
    for record in [&queued, &sending] {
        let record = restored.record(&record.id)?.unwrap();
        assert_eq!(record.state, TransmissionState::Uncertain);
        assert!(record.delivery.uncertain_at.is_some());
        assert!(record.delivery.radio_submitted_at.is_none());
        assert!(!restored.transition(&record.id, &TransmissionUpdate::Claim)?);
    }
    let alert = restored.record(&alert.id)?.unwrap();
    assert_eq!(alert.state, TransmissionState::Confirmed);
    assert!(alert.delivery.radio_submitted_at.is_some());
    assert!(alert.delivery.transport_confirmed_at.is_some());
    assert_eq!(alert.acknowledgements.len(), 2);
    assert!(
        alert
            .acknowledgements
            .iter()
            .any(|ack| ack.source == AcknowledgementSource::Radio
                && ack.node == "!radio"
                && ack.channel == 2)
    );
    assert_eq!(restored.public()?[0].id, published.id);
    assert!(!serde_json::to_string(&restored.public()?)?.contains(&staff.id));
    let provenance: (String, i64) = restored.connect()?.query_row(
        "SELECT source_sha256,quarantined FROM recovery_history",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert_eq!(provenance, (report.source_sha256, 2));
    let fresh = restored.create(message("New message after recovery"))?;
    let restarted = EventStore::new(&spare, 100, true)?;
    assert_eq!(restarted.pending()?.len(), 1);
    assert_eq!(restarted.pending()?[0].id, fresh.id);
    assert!(restarted.transition(&fresh.id, &TransmissionUpdate::Claim)?);
    Ok(())
}

#[test]
fn destination_database_sidecars_and_dangling_symlinks_are_preserved() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path();
    let store = EventStore::new(&root.join("original"), 100, false)?;
    let backup = root.join("backup.sqlite");
    store.backup(&backup)?;
    for (index, name) in [
        "events.sqlite",
        "events.sqlite-wal",
        "events.sqlite-shm",
        "events.sqlite-journal",
    ]
    .iter()
    .enumerate()
    {
        let destination = root.join(format!("spare-{index}"));
        fs::create_dir(&destination)?;
        let existing = destination.join(name);
        fs::write(&existing, b"keep this state")?;
        assert!(restore_snapshot(&backup, &destination).is_err());
        assert_eq!(fs::read(existing)?, b"keep this state");
        assert_eq!(fs::read_dir(destination)?.count(), 1);
    }
    let spare = root.join("symlink-spare");
    fs::create_dir(&spare)?;
    symlink("missing-target", spare.join("events.sqlite"))?;
    assert!(restore_snapshot(&backup, &spare).is_err());
    assert!(
        fs::symlink_metadata(spare.join("events.sqlite"))?
            .file_type()
            .is_symlink()
    );
    Ok(())
}

#[test]
fn invalid_missing_and_unrelated_files_do_not_initialize_any_event_database() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path();
    let corrupt = root.join("corrupt.sqlite");
    fs::write(&corrupt, b"not a database")?;
    let unrelated = root.join("unrelated.sqlite");
    Connection::open(&unrelated)?.execute_batch("CREATE TABLE other(value TEXT)")?;
    for (index, source) in [corrupt, unrelated, root.join("missing.sqlite")]
        .iter()
        .enumerate()
    {
        let spare = root.join(format!("spare-{index}"));
        assert!(restore_snapshot(source, &spare).is_err());
        assert_no_installed_state(&spare)?;
    }
    Ok(())
}

#[test]
fn live_source_sidecars_are_rejected() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path();
    let store = EventStore::new(&root.join("original"), 100, false)?;
    let backup = root.join("backup.sqlite");
    store.backup(&backup)?;
    for suffix in ["-wal", "-shm", "-journal"] {
        let sidecar = root.join(format!("backup.sqlite{suffix}"));
        fs::write(&sidecar, b"pending changes")?;
        let spare = root.join("spare");
        assert!(restore_snapshot(&backup, &spare).is_err());
        assert_no_installed_state(&spare)?;
        fs::remove_file(sidecar)?;
    }
    Ok(())
}

#[test]
fn incompatible_ordered_columns_indexes_generated_fields_and_schema_objects_are_rejected()
-> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path();
    let store = EventStore::new(&root.join("original"), 100, false)?;
    let backup = root.join("backup.sqlite");
    store.backup(&backup)?;
    for (index, mutation) in [
        "ALTER TABLE records DROP COLUMN uncertain_at",
        "ALTER TABLE staff ADD COLUMN unexpected TEXT",
        "ALTER TABLE staff ADD COLUMN generated TEXT GENERATED ALWAYS AS (name) VIRTUAL",
        "ALTER TABLE staff RENAME TO old_staff; CREATE TABLE staff(name TEXT,id TEXT PRIMARY KEY,role TEXT,hash TEXT UNIQUE); DROP TABLE old_staff;",
        "ALTER TABLE received RENAME TO old_received; CREATE TABLE received(packet_id TEXT,node TEXT,channel INTEGER,created REAL); DROP TABLE old_received;",
        "CREATE TRIGGER replay AFTER UPDATE ON records BEGIN UPDATE records SET state='queued'; END",
        "CREATE VIEW public_leak AS SELECT * FROM staff",
        "CREATE TABLE extra(value TEXT)",
        "CREATE INDEX unexpected ON records(author)",
        "CREATE UNIQUE INDEX duplicate_hash ON staff(hash)",
        "CREATE TABLE recovery_history(source_sha256 TEXT,restored REAL,quarantined INTEGER)",
    ].iter().enumerate() {
        let modified = root.join(format!("modified-{index}.sqlite"));
        fs::copy(&backup, &modified)?;
        { let db = Connection::open(&modified)?; db.execute_batch(mutation)?; }
        let spare = root.join(format!("spare-{index}"));
        assert!(restore_snapshot(&modified, &spare).is_err(), "accepted incompatible schema: {mutation}");
        assert_no_installed_state(&spare)?;
    }
    Ok(())
}

#[test]
fn repeated_recovery_keeps_provenance_and_does_not_overwrite_an_installed_database() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path();
    let store = EventStore::new(&root.join("original"), 100, false)?;
    store.create(message("Pending"))?;
    let first_backup = root.join("first.sqlite");
    store.backup(&first_backup)?;
    assert!(store.backup(&first_backup).is_err());
    let spare = root.join("spare");
    assert_eq!(restore_snapshot(&first_backup, &spare)?.quarantined, 1);
    let first_bytes = fs::read(spare.join("events.sqlite"))?;
    assert!(restore_snapshot(&first_backup, &spare).is_err());
    assert_eq!(fs::read(spare.join("events.sqlite"))?, first_bytes);
    let restored = EventStore::new(&spare, 100, true)?;
    let second_backup = root.join("second.sqlite");
    restored.backup(&second_backup)?;
    let second_spare = root.join("second-spare");
    assert_eq!(
        restore_snapshot(&second_backup, &second_spare)?.quarantined,
        0
    );
    assert_eq!(
        EventStore::new(&second_spare, 100, true)?
            .connect()?
            .query_row("SELECT count(*) FROM recovery_history", [], |row| row
                .get::<_, i64>(0))?,
        2
    );
    Ok(())
}
