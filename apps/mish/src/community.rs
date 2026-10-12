//! Opt-in rooms. All acceptance, membership and moderation decisions run on the journal owner.
use crate::{
    appliance::{EventStore, now, random_hex},
    domain::{MessageText, RecordId, Role, Text},
    invalid,
};
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub fn initialize(db: &Connection) -> Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS community_settings(singleton INTEGER PRIMARY KEY,enabled INTEGER NOT NULL,enroll_per_hour INTEGER NOT NULL,posts_per_minute INTEGER NOT NULL,queue_limit INTEGER NOT NULL);
    INSERT OR IGNORE INTO community_settings VALUES(1,0,120,30,10);
    CREATE TABLE IF NOT EXISTS guests(id TEXT PRIMARY KEY,nickname TEXT NOT NULL,created REAL NOT NULL);
    CREATE TABLE IF NOT EXISTS guest_sessions(hash TEXT PRIMARY KEY,guest_id TEXT NOT NULL,expires REAL NOT NULL,revoked INTEGER NOT NULL DEFAULT 0);
    CREATE TABLE IF NOT EXISTS rooms(id TEXT PRIMARY KEY,name TEXT NOT NULL,public INTEGER NOT NULL,channel_id TEXT,fingerprint TEXT,archived INTEGER NOT NULL DEFAULT 0);
    CREATE UNIQUE INDEX IF NOT EXISTS room_channel ON rooms(channel_id) WHERE archived=0;
    CREATE TABLE IF NOT EXISTS room_memberships(room_id TEXT NOT NULL,actor TEXT NOT NULL,PRIMARY KEY(room_id,actor));
    CREATE TABLE IF NOT EXISTS room_entries(seq INTEGER PRIMARY KEY AUTOINCREMENT,room_id TEXT NOT NULL,actor TEXT NOT NULL,label TEXT NOT NULL,text TEXT,record_id TEXT UNIQUE,created REAL NOT NULL,hidden INTEGER NOT NULL DEFAULT 0);
    CREATE INDEX IF NOT EXISTS room_cursor ON room_entries(room_id,seq);
    CREATE INDEX IF NOT EXISTS room_post_rate ON room_entries(actor,created);
    CREATE TABLE IF NOT EXISTS room_acceptances(actor TEXT NOT NULL,key TEXT NOT NULL,room_id TEXT NOT NULL,text TEXT NOT NULL,result TEXT NOT NULL,created REAL NOT NULL,PRIMARY KEY(actor,key));
    CREATE TABLE IF NOT EXISTS room_moderation(seq INTEGER PRIMARY KEY AUTOINCREMENT,operator TEXT NOT NULL,entry INTEGER NOT NULL,created REAL NOT NULL);")?;
    Ok(())
}
#[derive(Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Manage {
    Configure {
        enabled: bool,
        enroll_per_hour: u32,
        posts_per_minute: u32,
        queue_limit: u32,
    },
    Create {
        name: Text<80>,
        public: bool,
        channel: Option<RecordId>,
    },
    Archive {
        room: RecordId,
    },
    Revoke {
        guest: RecordId,
    },
    Hide {
        entry: i64,
    },
    List,
}
#[derive(Clone)]
pub enum Action {
    Enroll(String),
    Session,
    Rooms,
    Join(String),
    Leave(String),
    Messages(String, i64, i64),
    Post(String, String, String),
    Manage(Manage),
}
#[derive(Clone, Default)]
pub struct Credentials {
    pub guest: Option<String>,
    pub staff: Option<String>,
}
#[derive(Serialize)]
struct Room {
    id: String,
    name: String,
    public: bool,
    radio: bool,
    archived: bool,
}
fn rooms(db: &Connection) -> Result<Vec<Room>> {
    Ok(db
        .prepare("SELECT id,name,public,channel_id IS NOT NULL,archived FROM rooms ORDER BY name")?
        .query_map([], |r| {
            Ok(Room {
                id: r.get(0)?,
                name: r.get(1)?,
                public: r.get(2)?,
                radio: r.get(3)?,
                archived: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}
fn digest(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}
impl EventStore {
    pub fn community(&self, credentials: Credentials, action: Action) -> Result<Value> {
        let mut db = self.connect()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let (enabled,enroll_limit,post_limit,queue_limit): (bool,i64,i64,i64) = tx.query_row("SELECT enabled,enroll_per_hour,posts_per_minute,queue_limit FROM community_settings", [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
        if let Action::Enroll(nickname) = action {
            let nickname = Text::<24>::new(nickname)?;
            if nickname.as_str().chars().any(char::is_control) {
                return Err(invalid("nickname contains control characters"));
            }
            if !enabled {
                return Err(invalid("guest enrollment is disabled"));
            }
            let count: i64 = tx.query_row(
                "SELECT count(*) FROM guests WHERE created>?",
                [now() - 3600.0],
                |r| r.get(0),
            )?;
            if count >= enroll_limit {
                return Err(invalid("enrollment limit reached; try later"));
            }
            let id = random_hex(8)?;
            let token = random_hex(32)?;
            tx.execute(
                "INSERT INTO guests VALUES(?,?,?)",
                params![id, nickname.as_str(), now()],
            )?;
            tx.execute(
                "INSERT INTO guest_sessions VALUES(?,?,?,0)",
                params![digest(&token), id, now() + 86400.0],
            )?;
            tx.commit()?;
            return Ok(json!({"token":token,"label":format!("{}#{}",nickname.as_str(),&id[..8])}));
        }
        let staff = credentials
            .staff
            .as_deref()
            .map(|t| self.authenticate(t))
            .transpose()?
            .flatten();
        let guest: Option<(String, String)> = if let Some(token) = credentials.guest {
            tx.query_row("SELECT g.id,g.nickname FROM guest_sessions s JOIN guests g ON g.id=s.guest_id WHERE s.hash=? AND s.revoked=0 AND s.expires>?",params![digest(&token),now()],|r|Ok((r.get(0)?,r.get(1)?))).optional()?
        } else {
            None
        };
        let (actor, label) = if let Some(staff) = &staff {
            (
                format!("staff:{}", staff.id),
                format!("{}#{}", staff.name, &staff.id[..8]),
            )
        } else if let Some((id, name)) = &guest {
            (format!("guest:{id}"), format!("{name}#{}", &id[..8]))
        } else {
            return Err(invalid("valid guest session or staff credential required"));
        };
        if let Action::Manage(command) = action {
            let operator = staff
                .as_ref()
                .filter(|s| s.role == Role::Operator)
                .ok_or_else(|| invalid("operator required"))?;
            let result = match command {
                Manage::Configure {
                    enabled,
                    enroll_per_hour,
                    posts_per_minute,
                    queue_limit,
                } => {
                    if enroll_per_hour > 10000
                        || posts_per_minute > 600
                        || queue_limit as i64 >= self.queue_limit
                    {
                        return Err(invalid(
                            "invalid community limits; reserve staff queue capacity",
                        ));
                    }
                    tx.execute("UPDATE community_settings SET enabled=?,enroll_per_hour=?,posts_per_minute=?,queue_limit=?",params![enabled,enroll_per_hour,posts_per_minute,queue_limit])?;
                    json!({"updated":true})
                }
                Manage::Create {
                    name,
                    public,
                    channel,
                } => {
                    let binding = channel
                        .as_ref()
                        .map(|id| self.channel_metadata(id.as_str()))
                        .transpose()?
                        .flatten();
                    if channel.is_some() && binding.is_none() {
                        return Err(invalid("unknown channel"));
                    }
                    if let Some(b) = &binding {
                        if !b.verified
                            || (public && b.scope != crate::channel_store::ChannelScope::Community)
                        {
                            return Err(invalid(
                                "public rooms require a verified community channel",
                            ));
                        }
                    } else if !public {
                        return Err(invalid("private rooms require a channel"));
                    }
                    let id = random_hex(8)?;
                    tx.execute(
                        "INSERT INTO rooms VALUES(?,?,?,?,?,0)",
                        params![
                            id,
                            name.as_str(),
                            public,
                            channel.as_ref().map(RecordId::as_str),
                            binding.map(|b| b.fingerprint)
                        ],
                    )?;
                    json!({"id":id})
                }
                Manage::Archive { room } => {
                    tx.execute("UPDATE rooms SET archived=1 WHERE id=?", [room.as_str()])?;
                    json!({"updated":true})
                }
                Manage::Revoke { guest } => {
                    tx.execute(
                        "UPDATE guest_sessions SET revoked=1 WHERE guest_id=?",
                        [guest.as_str()],
                    )?;
                    json!({"updated":true})
                }
                Manage::Hide { entry } => {
                    tx.execute("UPDATE room_entries SET hidden=1 WHERE seq=?", [entry])?;
                    tx.execute(
                        "INSERT INTO room_moderation(operator,entry,created) VALUES(?,?,?)",
                        params![operator.id, entry, now()],
                    )?;
                    json!({"updated":true})
                }
                Manage::List => {
                    let guests = tx
                        .prepare("SELECT id,nickname FROM guests ORDER BY created DESC LIMIT 500")?
                        .query_map([], |r| {
                            Ok(json!({"id":r.get::<_,String>(0)?,"nickname":r.get::<_,String>(1)?}))
                        })?
                        .collect::<rusqlite::Result<Vec<_>>>()?;
                    json!({"rooms":rooms(&tx)?,"guests":guests,"enabled":enabled})
                }
            };
            tx.commit()?;
            return Ok(result);
        }
        let accessible = |room: &Room| -> Result<bool> {
            if room.public {
                return Ok(true);
            }
            let channel: String =
                tx.query_row("SELECT channel_id FROM rooms WHERE id=?", [&room.id], |r| {
                    r.get(0)
                })?;
            Ok(if let Some(staff) = &staff {
                self.can_access(staff, &channel)?
            } else {
                false
            })
        };
        if matches!(action, Action::Session) {
            return Ok(json!({"label":label}));
        }
        if matches!(action, Action::Rooms) {
            let mut visible = Vec::new();
            for room in rooms(&tx)? {
                if !room.archived && accessible(&room)? {
                    visible.push(room);
                }
            }
            return Ok(json!({"rooms":visible}));
        }
        let room_id = match &action {
            Action::Join(id)
            | Action::Leave(id)
            | Action::Messages(id, _, _)
            | Action::Post(id, _, _) => id,
            _ => unreachable!(),
        };
        let room = rooms(&tx)?
            .into_iter()
            .find(|r| r.id == *room_id)
            .ok_or_else(|| invalid("room unavailable"))?;
        if !accessible(&room)? {
            return Err(invalid("room unavailable"));
        }
        let result = match action {
            Action::Join(_) => {
                if room.archived {
                    return Err(invalid("room archived"));
                }
                tx.execute(
                    "INSERT OR IGNORE INTO room_memberships VALUES(?,?)",
                    params![room.id, actor],
                )?;
                json!({"joined":true})
            }
            Action::Leave(_) => {
                tx.execute(
                    "DELETE FROM room_memberships WHERE room_id=? AND actor=?",
                    params![room.id, actor],
                )?;
                json!({"joined":false})
            }
            action => {
                let member: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM room_memberships WHERE room_id=? AND actor=?)",
                    params![room.id, actor],
                    |r| r.get(0),
                )?;
                if !member {
                    return Err(invalid("join the room first"));
                }
                match action {
                    Action::Messages(_, after, moderation_after) => {
                        if after < 0 || moderation_after < 0 {
                            return Err(invalid("invalid cursor"));
                        }
                        let entries = tx.prepare("SELECT e.seq,e.label,CASE WHEN e.hidden=1 THEN NULL ELSE COALESCE(e.text,r.text) END,e.created,COALESCE(r.state,'local'),e.hidden FROM room_entries e LEFT JOIN records r ON r.id=e.record_id WHERE e.room_id=? AND e.seq>? ORDER BY e.seq LIMIT 100")?
                            .query_map(params![room.id,after],|r|Ok(json!({"cursor":r.get::<_,i64>(0)?,"author":r.get::<_,String>(1)?,"text":r.get::<_,Option<String>>(2)?,"created":r.get::<_,f64>(3)?,"delivery":r.get::<_,String>(4)?,"hidden":r.get::<_,bool>(5)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
                        let next = entries
                            .last()
                            .and_then(|e| e["cursor"].as_i64())
                            .unwrap_or(after);
                        let changes = tx.prepare("SELECT m.seq,m.entry FROM room_moderation m JOIN room_entries e ON e.seq=m.entry WHERE e.room_id=? AND m.seq>? ORDER BY m.seq LIMIT 100")?
                            .query_map(params![room.id,moderation_after], |r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
                        let moderation_cursor =
                            changes.last().map(|c| c.0).unwrap_or(moderation_after);
                        let hidden: Vec<_> = changes.into_iter().map(|c| c.1).collect();
                        let delivery = tx.prepare("SELECT e.seq,r.state FROM room_entries e JOIN records r ON r.id=e.record_id WHERE e.room_id=? ORDER BY e.seq DESC LIMIT 100")?.query_map([&room.id],|r|Ok(json!({"cursor":r.get::<_,i64>(0)?,"delivery":r.get::<_,String>(1)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
                        json!({"entries":entries,"cursor":next,"hidden":hidden,"moderation_cursor":moderation_cursor,"delivery":delivery})
                    }
                    Action::Post(_, key, text) => {
                        RecordId::new(&key)?;
                        let text = MessageText::new(text)?;
                        let old: Option<(String,String,String)> = tx.query_row("SELECT room_id,text,result FROM room_acceptances WHERE actor=? AND key=?",params![actor,key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
                        if let Some((id, body, result)) = old {
                            if id != room.id || body != text.as_str() {
                                return Err(invalid(
                                    "idempotency key already used for another message",
                                ));
                            }
                            return Ok(serde_json::from_str(&result)?);
                        }
                        if room.archived {
                            return Err(invalid("room archived"));
                        }
                        let count: i64 = tx.query_row(
                            "SELECT count(*) FROM room_entries WHERE actor=? AND created>?",
                            params![actor, now() - 60.0],
                            |r| r.get(0),
                        )?;
                        if count >= post_limit {
                            return Err(invalid("posting limit reached; try later"));
                        }
                        let (channel, fingerprint): (Option<String>, Option<String>) = tx
                            .query_row(
                                "SELECT channel_id,fingerprint FROM rooms WHERE id=?",
                                [&room.id],
                                |r| Ok((r.get(0)?, r.get(1)?)),
                            )?;
                        let mut record = None;
                        if let Some(channel) = channel {
                            self.validate_binding(
                                &channel,
                                fingerprint.as_deref().unwrap_or_default(),
                            )?;
                            let wire = MessageText::new(format!("{label}: {}", text.as_str()))?;
                            let (total,public): (i64,i64) = tx.query_row("SELECT count(*),COALESCE(sum(author LIKE 'guest:%'),0) FROM records WHERE state IN ('queued','sending')",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
                            if total >= self.queue_limit
                                || (staff.is_none()
                                    && (public >= queue_limit || total >= self.queue_limit / 2))
                            {
                                return Err(invalid("radio queue full; message not accepted"));
                            }
                            let id = random_hex(16)?;
                            tx.execute("INSERT INTO records(id,text,kind,author,created,state,channel_id,channel_fingerprint) VALUES(?,?,'message',?,?,'queued',?,?)",params![id,wire.as_str(),actor,now(),channel,fingerprint])?;
                            record = Some(id);
                        }
                        tx.execute("INSERT INTO room_entries(room_id,actor,label,text,record_id,created) VALUES(?,?,?,?,?,?)",params![room.id,actor,label,if record.is_none(){Some(text.as_str())}else{None},record,now()])?;
                        let result = json!({"cursor":tx.last_insert_rowid(),"accepted":true,"delivery":if record.is_some(){"queued"}else{"local"}});
                        tx.execute(
                            "INSERT INTO room_acceptances VALUES(?,?,?,?,?,?)",
                            params![
                                actor,
                                key,
                                room.id,
                                text.as_str(),
                                result.to_string(),
                                now()
                            ],
                        )?;
                        result
                    }
                    _ => unreachable!(),
                }
            }
        };
        tx.commit()?;
        Ok(result)
    }
}
