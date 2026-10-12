//! Restore standalone snapshots without replacing state or replaying snapshot sends.

use crate::{
    appliance::{initialize_schema, now},
    invalid,
};
use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt},
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

const RECOVERY_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS recovery_history(restored REAL,source_sha256 TEXT,quarantined INTEGER)";
const SIDECARS: [&str; 3] = ["-wal", "-shm", "-journal"];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryReport {
    pub restored: PathBuf,
    pub source_sha256: String,
    pub quarantined: usize,
    pub restored_at: f64,
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn has_sidecars(path: &Path) -> Result<bool> {
    for suffix in SIDECARS {
        if exists(&with_suffix(path, suffix))? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn refuse_existing(destination: &Path) -> Result<()> {
    if exists(destination)? || has_sidecars(destination)? {
        return Err(invalid(
            "refusing to overwrite existing event state; preserve it while the service is stopped",
        ));
    }
    Ok(())
}

#[derive(Debug, Eq, PartialEq)]
struct Column {
    position: i64,
    name: String,
    kind: String,
    not_null: i64,
    default: Option<String>,
    primary_key: i64,
    hidden: i64,
}

fn columns(db: &Connection, table: &str) -> Result<Vec<Column>> {
    let mut statement = db.prepare("SELECT * FROM pragma_table_xinfo(?) ORDER BY cid")?;
    Ok(statement
        .query_map([table], |row| {
            Ok(Column {
                position: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                not_null: row.get(3)?,
                default: row.get(4)?,
                primary_key: row.get(5)?,
                hidden: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

// Include collation and expression/hidden index columns, not just index names.
type IndexSignature = (bool, bool, Vec<(i64, Option<String>, bool, String, bool)>);
fn indexes(db: &Connection, table: &str) -> Result<Vec<IndexSignature>> {
    let mut statement = db.prepare("SELECT name,\"unique\",partial FROM pragma_index_list(?)")?;
    let indexes = statement
        .query_map([table], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, bool>(1)?,
                row.get::<_, bool>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut signatures = Vec::new();
    for (name, unique, partial) in indexes {
        let mut statement = db.prepare(
            "SELECT cid,name,\"desc\",coll,\"key\" FROM pragma_index_xinfo(?) ORDER BY seqno",
        )?;
        let fields = statement
            .query_map([name], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        signatures.push((unique, partial, fields));
    }
    signatures.sort();
    Ok(signatures)
}

fn tables(db: &Connection) -> Result<BTreeSet<String>> {
    let mut statement = db.prepare("SELECT name FROM sqlite_schema WHERE type='table'")?;
    Ok(statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<BTreeSet<_>>>()?)
}

fn validate(db: &Connection) -> Result<()> {
    db.execute_batch("PRAGMA trusted_schema=OFF")?;
    let mut statement = db.prepare("PRAGMA integrity_check")?;
    let result = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if result != ["ok"] {
        return Err(invalid("backup failed SQLite integrity validation"));
    }
    if db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type IN ('trigger','view'))",
        [],
        |row| row.get::<_, bool>(0),
    )? {
        return Err(invalid("backup contains unsupported database objects"));
    }
    let actual = tables(db)?;
    let reference = Connection::open_in_memory()?;
    initialize_schema(&reference)?;
    if actual.contains("recovery_history") {
        reference.execute_batch(RECOVERY_SCHEMA)?;
    }
    // Accept the exact pre-room schema, then migrate only the private staged copy.
    if !actual.contains("community_settings") {
        reference.execute_batch("DROP TABLE community_settings; DROP TABLE guests; DROP TABLE guest_sessions; DROP TABLE rooms; DROP TABLE room_memberships; DROP TABLE room_entries; DROP TABLE room_acceptances; DROP TABLE room_moderation;")?;
    }
    let mut expected = tables(&reference)?;
    if !actual.contains("community_settings") {
        expected.remove("sqlite_sequence");
    }
    if actual != expected {
        return Err(invalid("backup is not a compatible event journal"));
    }
    for table in expected {
        if columns(db, &table)? != columns(&reference, &table)?
            || indexes(db, &table)? != indexes(&reference, &table)?
        {
            return Err(invalid(format!(
                "backup has an incompatible {table} schema"
            )));
        }
    }
    Ok(())
}

struct StagedSnapshot(NamedTempFile);
impl Drop for StagedSnapshot {
    fn drop(&mut self) {
        for suffix in SIDECARS {
            let _ = fs::remove_file(with_suffix(self.0.path(), suffix));
        }
    }
}

pub fn restore_snapshot(source: &Path, state: &Path) -> Result<RecoveryReport> {
    restore_with_hook(source, state, || Ok(()))
}

fn copy_snapshot(
    source: &Path,
    staged: &mut File,
    after_copy: impl FnOnce() -> Result<()>,
) -> Result<String> {
    let mut original = File::open(source)?;
    let before = original.metadata()?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let length = original.read(&mut buffer)?;
        if length == 0 {
            break;
        }
        staged.write_all(&buffer[..length])?;
        digest.update(&buffer[..length]);
    }
    after_copy()?;
    let after = original.metadata()?;
    if (
        before.len(),
        before.mtime(),
        before.mtime_nsec(),
        before.ctime(),
        before.ctime_nsec(),
    ) != (
        after.len(),
        after.mtime(),
        after.mtime_nsec(),
        after.ctime(),
        after.ctime_nsec(),
    ) {
        return Err(invalid("backup changed during restore"));
    }
    Ok(hex::encode(digest.finalize()))
}

fn restore_with_hook(
    source: &Path,
    state: &Path,
    before_install: impl FnOnce() -> Result<()>,
) -> Result<RecoveryReport> {
    let source = fs::canonicalize(source).map_err(|error| {
        invalid(format!(
            "restore input must be an existing standalone backup file: {error}"
        ))
    })?;
    if !source.is_file() {
        return Err(invalid(
            "restore input must be an existing standalone backup file",
        ));
    }
    if has_sidecars(&source)? {
        return Err(invalid(
            "restore requires a standalone backup, not a live SQLite database",
        ));
    }
    let destination = state.join("events.sqlite");
    refuse_existing(&destination)?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(state)?;
    let state = fs::canonicalize(state)?;
    let destination = state.join("events.sqlite");
    let mut staged = StagedSnapshot(
        tempfile::Builder::new()
            .prefix(".restore-")
            .suffix(".sqlite")
            .tempfile_in(&state)?,
    );
    let source_sha256 = copy_snapshot(&source, staged.0.as_file_mut(), || Ok(()))?;
    if has_sidecars(&source)? {
        return Err(invalid(
            "restore requires a standalone backup, not a live SQLite database",
        ));
    }
    let restored_at = now();
    let quarantined;
    {
        let mut db = Connection::open(staged.0.path()).context("cannot open staged snapshot")?;
        validate(&db)?;
        initialize_schema(&db)?;
        db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL")?;
        let transaction = db.transaction()?;
        quarantined = transaction.execute("UPDATE records SET state='uncertain',uncertain_at=? WHERE state IN ('queued','sending')", [restored_at])?;
        // A copied journal cannot certify the physical radio attached to its new owner.
        transaction.execute("UPDATE radio_channels SET verified=0", [])?;
        transaction.execute_batch(RECOVERY_SCHEMA)?;
        transaction.execute(
            "INSERT INTO recovery_history VALUES(?,?,?)",
            params![restored_at, source_sha256, quarantined as i64],
        )?;
        transaction.commit()?;
    }
    staged.0.as_file().sync_all()?;
    refuse_existing(&destination)?;
    before_install()?;
    // The same-directory link atomically publishes the complete file, and never replaces another inode.
    fs::hard_link(staged.0.path(), &destination)
        .context("cannot install snapshot without overwriting state")?;
    fs::remove_file(staged.0.path())?;
    File::open(&state)?.sync_all()?;
    Ok(RecoveryReport {
        restored: destination,
        source_sha256,
        quarantined,
        restored_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::appliance::EventStore;
    #[test]
    fn source_change_during_copy_is_rejected() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("snapshot.sqlite");
        fs::write(&source, b"snapshot bytes")?;
        let mut staged = tempfile::tempfile()?;
        let result = copy_snapshot(&source, &mut staged, || {
            fs::OpenOptions::new()
                .append(true)
                .open(&source)?
                .write_all(b"changed during copy")?;
            Ok(())
        });
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("changed during restore")
        );
        Ok(())
    }
    #[test]
    fn concurrently_created_destination_is_never_overwritten() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let original = EventStore::new(&directory.path().join("original"), 100, false)?;
        let source = directory.path().join("backup.sqlite");
        original.backup(&source)?;
        let spare = directory.path().join("spare");
        let result = restore_with_hook(&source, &spare, || {
            fs::write(spare.join("events.sqlite"), b"concurrent commissioning")?;
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(
            fs::read(spare.join("events.sqlite"))?,
            b"concurrent commissioning"
        );
        assert_eq!(fs::read_dir(&spare)?.count(), 1);
        Ok(())
    }

    #[test]
    fn failed_install_preserves_snapshot_and_leaves_no_database() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let original = EventStore::new(&directory.path().join("original"), 100, false)?;
        let source = directory.path().join("backup.sqlite");
        original.backup(&source)?;
        let original_bytes = fs::read(&source)?;
        let spare = directory.path().join("spare");
        let result = restore_with_hook(&source, &spare, || Err(anyhow::anyhow!("disk is full")));
        assert!(result.is_err());
        assert_eq!(fs::read(&source)?, original_bytes);
        assert_eq!(fs::read_dir(&spare)?.count(), 0);
        Ok(())
    }
}
