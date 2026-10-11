//! Durable, single-coordinator event journal. No connection is shared between threads.

use crate::{
    domain::{
        Acknowledgement, AcknowledgementSource, Announcement, AnnouncementText, Delivery,
        Enrollment, MessageKind, NewRecord, PendingRecord, RadioText, Record, RecordId, Role,
        StaffIdentity, StaffName, TransmissionState, TransmissionUpdate,
    },
    invalid,
};
use anyhow::Result;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{TryRng, rngs::SysRng};
use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

fn random_hex(bytes: usize) -> Result<String> {
    let mut data = vec![0; bytes];
    SysRng.try_fill_bytes(&mut data)?;
    Ok(hex::encode(data))
}

pub fn initialize_schema(db: &Connection) -> Result<()> {
    db.execute_batch("PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS staff(id TEXT PRIMARY KEY,name TEXT,role TEXT,hash TEXT UNIQUE);
        CREATE TABLE IF NOT EXISTS records(id TEXT PRIMARY KEY,text TEXT,kind TEXT,author TEXT,
            created REAL,expires REAL,responder TEXT,state TEXT,packet_id TEXT);
        CREATE TABLE IF NOT EXISTS acks(record_id TEXT,actor TEXT,source TEXT,node TEXT,channel INTEGER,
            created REAL, UNIQUE(record_id,actor,source,node,channel));
        CREATE TABLE IF NOT EXISTS received(packet_id TEXT,node TEXT,channel INTEGER,created REAL,
            UNIQUE(packet_id,node,channel));
        CREATE TABLE IF NOT EXISTS announcements(id TEXT PRIMARY KEY,text TEXT,published REAL);")?;
    let mut statement = db.prepare("SELECT name FROM pragma_table_info('records')")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for column in ["submitted_at", "confirmed_at", "uncertain_at"] {
        if !columns.iter().any(|existing| existing == column) {
            db.execute_batch(&format!("ALTER TABLE records ADD COLUMN {column} REAL"))?;
        }
    }
    if !db.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('received') WHERE name='created')",
        [],
        |row| row.get::<_, bool>(0),
    )? {
        db.execute_batch("ALTER TABLE received ADD COLUMN created REAL DEFAULT 0")?;
    }
    Ok(())
}

fn enum_column<T: std::str::FromStr<Err = anyhow::Error>>(
    row: &Row<'_>,
    column: &str,
) -> rusqlite::Result<T> {
    let index = row.as_ref().column_index(column)?;
    row.get::<_, String>(index)?.parse::<T>().map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, error.into())
    })
}

fn acknowledgement_row(row: &Row<'_>) -> rusqlite::Result<Acknowledgement> {
    Ok(Acknowledgement {
        record_id: row.get("record_id")?,
        actor: row.get("actor")?,
        source: enum_column(row, "source")?,
        node: row.get("node")?,
        channel: row.get("channel")?,
        created: row.get("created")?,
    })
}

fn announcement_row(row: &Row<'_>) -> rusqlite::Result<Announcement> {
    Ok(Announcement {
        id: row.get("id")?,
        text: row.get("text")?,
        published: row.get("published")?,
    })
}

fn record_row(row: &Row<'_>) -> rusqlite::Result<Record> {
    let state = enum_column(row, "state")?;
    let submitted_at = row.get("submitted_at")?;
    let confirmed_at = row.get("confirmed_at")?;
    let uncertain_at = row.get("uncertain_at")?;
    let expires: Option<f64> = row.get("expires")?;
    Ok(Record {
        id: row.get("id")?,
        text: row.get("text")?,
        kind: enum_column(row, "kind")?,
        author: row.get("author")?,
        created: row.get("created")?,
        expires,
        responder: row.get("responder")?,
        state,
        packet_id: row.get("packet_id")?,
        submitted_at,
        confirmed_at,
        uncertain_at,
        author_name: row.get("author_name")?,
        acknowledgements: vec![],
        expired: expires.is_some_and(|expires| expires <= now()),
        delivery: Delivery {
            persisted: true,
            queue: match state {
                TransmissionState::Queued => "pending",
                TransmissionState::Sending => "processing",
                _ => "closed",
            }
            .to_owned(),
            radio_submitted_at: submitted_at,
            transport_confirmed_at: confirmed_at,
            uncertain_at,
        },
    })
}

#[derive(Clone, Debug)]
pub struct EventStore {
    pub path: PathBuf,
    pub queue_limit: i64,
}

impl EventStore {
    pub fn new(state_dir: &Path, queue_limit: i64, recover: bool) -> Result<Self> {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true).mode(0o700).create(state_dir)?;
        let store = Self {
            path: state_dir.join("events.sqlite"),
            queue_limit,
        };
        // Create privately before SQLite opens the file, including during commissioning.
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&store.path)
        {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        let db = store.connect()?;
        initialize_schema(&db)?;
        fs::set_permissions(&store.path, fs::Permissions::from_mode(0o600))?;
        if recover {
            db.execute(
                "UPDATE records SET state='uncertain',uncertain_at=? WHERE state='sending'",
                [now()],
            )?;
        }
        Ok(store)
    }

    pub fn connect(&self) -> Result<Connection> {
        let db = Connection::open(&self.path)?;
        db.busy_timeout(Duration::from_secs(10))?;
        db.execute_batch("PRAGMA synchronous=FULL; PRAGMA trusted_schema=OFF;")?;
        Ok(db)
    }

    pub fn enroll(&self, name: &str, role: Role, first: bool) -> Result<Enrollment> {
        let name = StaffName::new(name)?.as_str().to_owned();
        let mut token_bytes = [0u8; 32];
        SysRng.try_fill_bytes(&mut token_bytes)?;
        let token = URL_SAFE_NO_PAD.encode(token_bytes);
        let id = random_hex(8)?;
        let mut db = self.connect()?;
        let transaction = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if first
            && transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM staff WHERE role='operator')",
                [],
                |row| row.get::<_, bool>(0),
            )?
        {
            return Err(invalid(
                "already commissioned; enroll using the existing operator",
            ));
        }
        transaction.execute(
            "INSERT INTO staff VALUES(?,?,?,?)",
            params![
                id,
                name,
                role.as_str(),
                hex::encode(Sha256::digest(token.as_bytes()))
            ],
        )?;
        transaction.commit()?;
        Ok(Enrollment {
            id,
            name,
            role,
            token,
        })
    }

    pub fn revoke(&self, id: &str) -> Result<()> {
        RecordId::new(id)?;
        let mut db = self.connect()?;
        let transaction = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let role = transaction
            .query_row("SELECT role FROM staff WHERE id=?", [id], |row| {
                row.get::<_, String>(0)
            })
            .optional()?
            .ok_or_else(|| invalid("staff not found"))?;
        if role == "operator"
            && transaction.query_row(
                "SELECT count(*) FROM staff WHERE role='operator'",
                [],
                |row| row.get::<_, i64>(0),
            )? <= 1
        {
            return Err(invalid("cannot revoke the last operator"));
        }
        transaction.execute("DELETE FROM staff WHERE id=?", [id])?;
        transaction.commit()?;
        Ok(())
    }

    pub fn authenticate(&self, token: &str) -> Result<Option<StaffIdentity>> {
        Ok(self
            .connect()?
            .query_row(
                "SELECT id,name,role FROM staff WHERE hash=?",
                [hex::encode(Sha256::digest(token.as_bytes()))],
                |row| {
                    Ok(StaffIdentity {
                        id: row.get("id")?,
                        name: row.get("name")?,
                        role: enum_column(row, "role")?,
                    })
                },
            )
            .optional()?)
    }

    pub fn create(&self, record: NewRecord) -> Result<Record> {
        // Validate time again at durable acceptance, after a command may have waited in the actor queue.
        if record.kind != MessageKind::Message {
            let timestamp = now();
            if !record.expires.is_some_and(|value| {
                value.is_finite() && value > timestamp && value <= timestamp + 86400.0
            }) {
                return Err(invalid("alerts require expiration within 24 hours"));
            }
        }
        let mut db = self.connect()?;
        let transaction = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if transaction.query_row(
            "SELECT count(*) FROM records WHERE state IN ('queued','sending')",
            [],
            |row| row.get::<_, i64>(0),
        )? >= self.queue_limit
        {
            return Err(invalid("outbound queue full; message was not accepted"));
        }
        let id = loop {
            let candidate = random_hex(4)?.to_uppercase();
            if !transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM records WHERE id=?)",
                [&candidate],
                |row| row.get::<_, bool>(0),
            )? {
                break candidate;
            }
        };
        transaction.execute("INSERT INTO records(id,text,kind,author,created,expires,responder,state,packet_id) VALUES(?,?,?,?,?,?,?,'queued',NULL)", params![id, record.text.as_str(), record.kind.as_str(), record.author, now(), record.expires, record.responder.as_ref().map(StaffName::as_str)])?;
        transaction.commit()?;
        self.record(&id)?
            .ok_or_else(|| anyhow::anyhow!("accepted record disappeared"))
    }

    pub fn record(&self, id: &str) -> Result<Option<Record>> {
        RecordId::new(id)?;
        let db = self.connect()?;
        let Some(mut record) = db.query_row("SELECT r.*,COALESCE(s.name,r.author) AS author_name FROM records r LEFT JOIN staff s ON r.author=s.id WHERE r.id=?", [id], record_row).optional()? else { return Ok(None); };
        let mut statement = db.prepare("SELECT * FROM acks WHERE record_id=?")?;
        record.acknowledgements = statement
            .query_map([id], acknowledgement_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Some(record))
    }

    pub fn records(&self) -> Result<Vec<Record>> {
        let mut db = self.connect()?;
        // The journal and its ACKs are one read snapshot, using one connection
        // and two queries regardless of the number of returned records.
        let transaction = db.transaction()?;
        let mut records = {
            let mut statement = transaction.prepare("SELECT r.*,COALESCE(s.name,r.author) AS author_name FROM records r LEFT JOIN staff s ON r.author=s.id ORDER BY r.created DESC LIMIT 500")?;
            statement
                .query_map([], record_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let positions: std::collections::HashMap<_, _> = records
            .iter()
            .enumerate()
            .map(|(index, record)| (record.id.clone(), index))
            .collect();
        if !records.is_empty() {
            let query = format!(
                "SELECT * FROM acks WHERE record_id IN ({})",
                vec!["?"; records.len()].join(",")
            );
            let mut statement = transaction.prepare(&query)?;
            for acknowledgement in statement.query_map(
                rusqlite::params_from_iter(positions.keys()),
                acknowledgement_row,
            )? {
                let acknowledgement = acknowledgement?;
                if let Some(index) = positions.get(&acknowledgement.record_id) {
                    records[*index].acknowledgements.push(acknowledgement);
                }
            }
        }
        transaction.commit()?;
        Ok(records)
    }

    pub fn acknowledge(
        &self,
        id: &str,
        actor: &str,
        source: AcknowledgementSource,
        node: Option<&str>,
        channel: Option<i64>,
    ) -> Result<Acknowledgement> {
        RecordId::new(id)?;
        let mut db = self.connect()?;
        let transaction = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let record = transaction
            .query_row("SELECT kind,expires FROM records WHERE id=?", [id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<f64>>(1)?))
            })
            .optional()?;
        if !record.is_some_and(|(kind, expires)| {
            kind != "message" && expires.is_some_and(|expires| expires > now())
        }) {
            return Err(invalid("only active alerts can be acknowledged"));
        }
        let node = node.unwrap_or("");
        let channel = channel.unwrap_or(-1);
        transaction.execute(
            "INSERT OR IGNORE INTO acks VALUES(?,?,?,?,?,?)",
            params![id, actor, source.as_str(), node, channel, now()],
        )?;
        let acknowledgement = transaction.query_row("SELECT * FROM acks WHERE record_id=? AND actor=? AND source=? AND node=? AND channel=?", params![id, actor, source.as_str(), node, channel], acknowledgement_row)?;
        transaction.commit()?;
        Ok(acknowledgement)
    }

    pub fn publish(&self, text: &str) -> Result<Announcement> {
        let text = AnnouncementText::new(text)?.as_str().to_owned();
        let id = random_hex(8)?;
        let published = now();
        self.connect()?.execute(
            "INSERT INTO announcements VALUES(?,?,?)",
            params![id, text, published],
        )?;
        Ok(Announcement {
            id,
            text,
            published,
        })
    }

    pub fn public(&self) -> Result<Vec<Announcement>> {
        let db = self.connect()?;
        let mut statement = db.prepare(
            "SELECT id,text,published FROM announcements ORDER BY published DESC LIMIT 500",
        )?;
        Ok(statement
            .query_map([], announcement_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn pending(&self) -> Result<Vec<PendingRecord>> {
        let mut db = self.connect()?;
        let transaction = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "UPDATE records SET state='expired' WHERE state='queued' AND expires<=?",
            [now()],
        )?;
        let records = {
            let mut statement = transaction
                .prepare("SELECT * FROM records WHERE state='queued' ORDER BY created LIMIT 100")?;
            statement
                .query_map([], |row| {
                    Ok(PendingRecord {
                        id: row.get("id")?,
                        text: row.get("text")?,
                        kind: enum_column(row, "kind")?,
                        expires: row.get("expires")?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        transaction.commit()?;
        Ok(records)
    }

    pub fn transition(&self, id: &str, update: &TransmissionUpdate) -> Result<bool> {
        RecordId::new(id)?;
        let mut db = self.connect()?;
        let transaction = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let record = transaction
            .query_row(
                "SELECT state,expires FROM records WHERE id=?",
                [id],
                |row| {
                    Ok((
                        enum_column::<TransmissionState>(row, "state")?,
                        row.get::<_, Option<f64>>(1)?,
                    ))
                },
            )
            .optional()?;
        let Some((previous, expires)) = record else {
            return Ok(false);
        };
        let Some(state) = previous.transition(update) else {
            return Ok(false);
        };
        if matches!(update, TransmissionUpdate::Claim)
            && expires.is_some_and(|expiration| expiration <= now())
        {
            transaction.execute("UPDATE records SET state='expired' WHERE id=?", [id])?;
            transaction.commit()?;
            return Ok(false);
        }
        let packet_id = match update {
            TransmissionUpdate::Submitted { packet_id } => Some(packet_id.as_str()),
            _ => None,
        };
        transaction.execute(
            "UPDATE records SET state=?,packet_id=COALESCE(?,packet_id) WHERE id=?",
            params![state.as_str(), packet_id, id],
        )?;
        let timestamp_column = match state {
            TransmissionState::Submitted => Some("submitted_at"),
            TransmissionState::Uncertain => Some("uncertain_at"),
            _ => None,
        };
        if let Some(column) = timestamp_column {
            transaction.execute(
                &format!("UPDATE records SET {column}=? WHERE id=?"),
                params![now(), id],
            )?;
        }
        transaction.commit()?;
        Ok(true)
    }

    pub fn transport_confirmation(&self, packet_id: &str) -> Result<bool> {
        Ok(self.connect()?.execute("UPDATE records SET state='confirmed',confirmed_at=? WHERE packet_id=? AND state IN ('submitted','uncertain')", params![now(), packet_id])? > 0)
    }

    pub fn receive(&self, packet_id: &str, node: &str, text: &str, channel: i64) -> Result<bool> {
        let text = RadioText::new(text)?;
        let text = text.as_str();
        let mut db = self.connect()?;
        let transaction = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if transaction.execute(
            "INSERT OR IGNORE INTO received VALUES(?,?,?,?)",
            params![packet_id, node, channel, now()],
        )? == 0
        {
            return Ok(false);
        }
        if let Some(identity) = text.strip_prefix("ACK ") {
            let id = identity.trim().to_uppercase();
            let record = transaction
                .query_row(
                    "SELECT kind,expires FROM records WHERE id=?",
                    [&id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<f64>>(1)?)),
                )
                .optional()?;
            if record.is_some_and(|(kind, expires)| {
                kind != "message" && expires.is_some_and(|expires| expires > now())
            }) {
                transaction.execute(
                    "INSERT OR IGNORE INTO acks VALUES(?,?,?,?,?,?)",
                    params![id, node, "radio", node, channel, now()],
                )?;
            }
        } else {
            transaction.execute("INSERT INTO records(id,text,kind,author,created,expires,responder,state,packet_id) VALUES(?,?,'message',?,?,NULL,NULL,'received',?)", params![format!("RF-{}", random_hex(8)?), text, format!("radio:{node}:channel:{channel}"), now(), packet_id])?;
        }
        transaction.commit()?;
        Ok(true)
    }

    pub fn backup(&self, destination: &Path) -> Result<()> {
        let output = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(destination)
        {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(invalid("backup destination already exists"));
            }
            Err(error) => return Err(error.into()),
        };
        let source = self.connect()?;
        let mut target = Connection::open(destination)?;
        target.execute_batch("PRAGMA synchronous=FULL")?;
        {
            let backup = rusqlite::backup::Backup::new(&source, &mut target)?;
            backup.run_to_completion(100, Duration::from_millis(10), None)?;
        }
        // Close into a standalone snapshot, with no outstanding WAL sidecar.
        target.execute_batch("PRAGMA journal_mode=DELETE")?;
        drop(target);
        output.sync_all()?;
        if let Some(parent) = destination
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::File::open(parent)?.sync_all()?;
        }
        Ok(())
    }

    pub fn prune(&self, days: i64) -> Result<usize> {
        if days < 1 {
            return Err(invalid("retention days must be positive"));
        }
        let cutoff = now() - days as f64 * 86400.0;
        let mut db = self.connect()?;
        let transaction = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut removed = transaction.execute(
            "DELETE FROM acks WHERE record_id IN (SELECT id FROM records WHERE created<?)",
            [cutoff],
        )?;
        removed += transaction.execute("DELETE FROM records WHERE created<?", [cutoff])?;
        removed += transaction.execute("DELETE FROM announcements WHERE published<?", [cutoff])?;
        removed += transaction.execute("DELETE FROM received WHERE created<?", [cutoff])?;
        transaction.commit()?;
        Ok(removed)
    }
}
