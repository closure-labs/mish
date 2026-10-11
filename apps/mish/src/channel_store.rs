//! Private channel material and membership live in the journal's SQLite backup.
//! Only explicitly scoped enrollment exports serialize secret-bearing data.

use crate::{
    appliance::{EventStore, now},
    channels,
    domain::{RecordId, Role, StaffIdentity},
    invalid,
};
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, str::FromStr};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelScope {
    Community,
    Staff,
    Department,
}
impl ChannelScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Community => "community",
            Self::Staff => "staff",
            Self::Department => "department",
        }
    }
}
impl FromStr for ChannelScope {
    type Err = anyhow::Error;
    fn from_str(value: &str) -> Result<Self> {
        match value {
            "community" => Ok(Self::Community),
            "staff" => Ok(Self::Staff),
            "department" => Ok(Self::Department),
            _ => Err(invalid("invalid channel scope")),
        }
    }
}

// No Debug or Serialize: a plan contains raw 32-byte channel keys.
#[derive(Clone)]
pub struct PlannedChannel {
    pub id: String,
    pub scope: ChannelScope,
    pub slot: u32,
    pub settings: Vec<u8>,
}
#[derive(Clone)]
pub struct ChannelPlan {
    pub channels: Vec<PlannedChannel>,
    pub lora: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelMetadata {
    pub id: String,
    pub name: String,
    pub scope: ChannelScope,
    pub slot: u32,
    pub generation: i64,
    pub fingerprint: String,
    pub verified: bool,
}
pub type ChannelBinding = ChannelMetadata;

#[derive(Clone)]
pub struct RadioChannel {
    pub metadata: ChannelMetadata,
    pub settings: Vec<u8>,
    pub lora: Option<Vec<u8>>,
}

#[derive(Serialize)]
pub struct EnrollmentExport {
    pub channel: ChannelMetadata,
    pub url: String,
    pub qr_svg: String,
}

pub fn initialize_channel_schema(db: &Connection) -> Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS radio_plan(singleton INTEGER PRIMARY KEY,generation INTEGER,lora BLOB);
        CREATE TABLE IF NOT EXISTS radio_channels(id TEXT PRIMARY KEY,slot INTEGER UNIQUE,scope TEXT,name TEXT,
            settings BLOB,key_digest TEXT UNIQUE,fingerprint TEXT,generation INTEGER,verified INTEGER,created REAL);
        CREATE TABLE IF NOT EXISTS channel_access(channel_id TEXT,staff_id TEXT,PRIMARY KEY(channel_id,staff_id));")?;
    Ok(())
}

/// Bind queued work to exact channel settings, RF configuration and device slot.
/// Raw protobufs are preserved; normalization must never discard unknown privacy/encryption fields.
pub fn channel_fingerprint(settings: &[u8], lora: Option<&[u8]>, slot: u32) -> String {
    let mut digest = Sha256::new();
    digest.update(b"mish-meshtastic-channel-v1\0");
    digest.update(slot.to_be_bytes());
    digest.update((settings.len() as u64).to_be_bytes());
    digest.update(settings);
    match lora {
        Some(lora) => {
            digest.update([1]);
            digest.update((lora.len() as u64).to_be_bytes());
            digest.update(lora);
        }
        None => digest.update([0]),
    }
    hex::encode(digest.finalize())
}

fn metadata_row(row: &Row<'_>) -> rusqlite::Result<ChannelMetadata> {
    let scope = row
        .get::<_, String>("scope")?
        .parse::<ChannelScope>()
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, error.into())
        })?;
    Ok(ChannelMetadata {
        id: row.get("id")?,
        name: row.get("name")?,
        scope,
        slot: row.get("slot")?,
        generation: row.get("generation")?,
        fingerprint: row.get("fingerprint")?,
        verified: row.get("verified")?,
    })
}

fn metadata_on(db: &Connection, id: &str) -> Result<Option<ChannelMetadata>> {
    RecordId::new(id)?;
    Ok(db.query_row("SELECT id,slot,scope,name,fingerprint,generation,verified FROM radio_channels WHERE id=?", [id], metadata_row).optional()?)
}

fn all_metadata(db: &Connection) -> Result<Vec<ChannelMetadata>> {
    let mut query = db.prepare("SELECT id,slot,scope,name,fingerprint,generation,verified FROM radio_channels ORDER BY slot")?;
    Ok(query
        .query_map([], metadata_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

fn persisted_role(db: &Connection, id: &str) -> Result<Option<Role>> {
    let role = db
        .query_row("SELECT role FROM staff WHERE id=?", [id], |row| {
            row.get::<_, String>(0)
        })
        .optional()?;
    role.map(|role| role.parse()).transpose()
}

fn access_on(db: &Connection, actor: &StaffIdentity, channel: &ChannelMetadata) -> Result<bool> {
    // Re-read membership/role so stale or fabricated HTTP identity values cannot grant access.
    let Some(role) = persisted_role(db, &actor.id)? else {
        return Ok(false);
    };
    Ok(match channel.scope {
        ChannelScope::Community => true,
        ChannelScope::Staff => matches!(role, Role::Staff | Role::Operator),
        ChannelScope::Department if role == Role::Operator => true,
        ChannelScope::Department => db.query_row(
            "SELECT EXISTS(SELECT 1 FROM channel_access WHERE channel_id=? AND staff_id=?)",
            params![channel.id, actor.id],
            |row| row.get(0),
        )?,
    })
}

fn require_operator(db: &Connection, id: &str) -> Result<()> {
    RecordId::new(id)?;
    if persisted_role(db, id)? != Some(Role::Operator) {
        return Err(invalid("operator required for channel management"));
    }
    Ok(())
}

fn legacy_staff() -> ChannelBinding {
    ChannelMetadata {
        id: "staff".into(),
        name: "Legacy staff".into(),
        scope: ChannelScope::Staff,
        slot: 0,
        generation: 0,
        fingerprint: String::new(),
        verified: false,
    }
}

impl EventStore {
    /// Install a complete initial plan atomically. Occupied slots/keys are never replaced.
    /// Plan/key rotation requires a later explicit operation with its own queue quarantine.
    pub fn install_channel_plan(&self, plan: &ChannelPlan) -> Result<usize> {
        if !(2..=8).contains(&plan.channels.len())
            || plan.lora.as_ref().is_some_and(|lora| lora.len() > 512)
        {
            return Err(invalid(
                "channel plan requires 2–8 channels and bounded LoRa settings",
            ));
        }
        if let Some(lora) = &plan.lora {
            channels::validate_lora(lora)?;
        }
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        let mut keys = BTreeSet::new();
        let mut slots = BTreeSet::new();
        let mut decoded = Vec::new();
        for channel in &plan.channels {
            RecordId::new(&channel.id)?;
            if !ids.insert(channel.id.clone())
                || !slots.insert(channel.slot)
                || channel.settings.len() > 512
            {
                return Err(invalid(
                    "duplicate channel identity/slot or oversized channel settings",
                ));
            }
            let view = channels::settings_view(&channel.settings)?;
            if (view.name.is_empty() && channel.scope != ChannelScope::Community)
                || view.name.len() >= 12
                || view.name.eq_ignore_ascii_case("admin")
                || !names.insert(view.name.clone())
            {
                return Err(invalid(
                    "channel names must be distinct, below 12 UTF-8 bytes, and not admin; private names must be nonempty",
                ));
            }
            if (channel.scope != ChannelScope::Community
                && (view.psk.len() != 32 || view.psk.iter().all(|byte| *byte == 0)))
                || !keys.insert(hex::encode(Sha256::digest(&view.psk)))
            {
                return Err(invalid(
                    "private channels require independent nonzero 32-byte PSKs; keys must not be reused across scopes",
                ));
            }
            if view.uplink_enabled || view.downlink_enabled {
                return Err(invalid(
                    "offline channel plans prohibit MQTT uplink/downlink",
                ));
            }
            match (channel.scope, channel.slot, channel.id.as_str()) {
                (ChannelScope::Community, 0, "community")
                | (ChannelScope::Staff, 1..=7, "staff") => {}
                (ChannelScope::Department, 1..=7, id) if id != "community" && id != "staff" => {}
                _ => {
                    return Err(invalid(
                        "community must occupy slot 0; private channels require distinct secondary slots 1–7",
                    ));
                }
            }
            decoded.push(view);
        }
        if !ids.contains("community") || !ids.contains("staff") {
            return Err(invalid(
                "channel plan requires one community and one staff channel",
            ));
        }
        let mut db = self.connect()?;
        let transaction = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM radio_channels) OR EXISTS(SELECT 1 FROM radio_plan)",
            [],
            |row| row.get::<_, bool>(0),
        )? {
            return Err(invalid(
                "channel plan already exists; refusing to replace occupied slots or keys",
            ));
        }
        transaction.execute(
            "INSERT INTO radio_plan VALUES(1,1,?)",
            [plan.lora.as_deref()],
        )?;
        for (channel, view) in plan.channels.iter().zip(decoded.iter()) {
            transaction.execute(
                "INSERT INTO radio_channels VALUES(?,?,?,?,?,?,?,?,0,?)",
                params![
                    channel.id,
                    channel.slot,
                    channel.scope.as_str(),
                    view.name,
                    channel.settings,
                    hex::encode(Sha256::digest(&view.psk)),
                    channel_fingerprint(&channel.settings, plan.lora.as_deref(), channel.slot),
                    1,
                    now()
                ],
            )?;
        }
        // The previous single-channel coordinator accepted work without a durable key/slot binding.
        let has_binding: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('records') WHERE name='channel_fingerprint')", [], |row| row.get(0))?;
        let quarantine = if has_binding {
            "UPDATE records SET state='uncertain',uncertain_at=? WHERE state IN ('queued','sending') AND channel_fingerprint IS NULL"
        } else {
            "UPDATE records SET state='uncertain',uncertain_at=? WHERE state IN ('queued','sending')"
        };
        let quarantined = transaction.execute(quarantine, [now()])?;
        transaction.commit()?;
        Ok(quarantined)
    }

    pub fn channel_metadata(&self, channel_id: &str) -> Result<Option<ChannelMetadata>> {
        metadata_on(&self.connect()?, channel_id)
    }

    pub fn channel_catalog(&self, actor: Option<&StaffIdentity>) -> Result<Vec<ChannelMetadata>> {
        let db = self.connect()?;
        let mut channels = Vec::new();
        for channel in all_metadata(&db)? {
            if channel.scope == ChannelScope::Community
                || match actor {
                    Some(actor) => access_on(&db, actor, &channel)?,
                    None => false,
                }
            {
                channels.push(channel);
            }
        }
        Ok(channels)
    }

    pub fn public_channel_catalog(&self) -> Result<Vec<ChannelMetadata>> {
        self.channel_catalog(None)
    }

    pub fn can_access(&self, actor: &StaffIdentity, channel_id: &str) -> Result<bool> {
        let db = self.connect()?;
        if let Some(channel) = metadata_on(&db, channel_id)? {
            return access_on(&db, actor, &channel);
        }
        Ok(channel_id == "staff"
            && all_metadata(&db)?.is_empty()
            && matches!(
                persisted_role(&db, &actor.id)?,
                Some(Role::Staff | Role::Operator)
            ))
    }

    pub fn authorize_channel(
        &self,
        actor: &StaffIdentity,
        channel_id: &str,
    ) -> Result<ChannelBinding> {
        let db = self.connect()?;
        if let Some(channel) = metadata_on(&db, channel_id)? {
            if access_on(&db, actor, &channel)? {
                return Ok(channel);
            }
            return Err(invalid("channel membership required"));
        }
        if channel_id == "staff"
            && all_metadata(&db)?.is_empty()
            && matches!(
                persisted_role(&db, &actor.id)?,
                Some(Role::Staff | Role::Operator)
            )
        {
            return Ok(legacy_staff());
        }
        Err(invalid("channel not found or access denied"))
    }

    pub fn public_binding(&self, channel_id: &str) -> Result<ChannelBinding> {
        let channel = self
            .channel_metadata(channel_id)?
            .ok_or_else(|| invalid("community channel not found"))?;
        if channel.scope != ChannelScope::Community {
            return Err(invalid(
                "public access is restricted to the community channel",
            ));
        }
        Ok(channel)
    }

    /// Raw settings are exposed only to trusted local deployment code, never public HTTP responses.
    pub fn radio_channels(&self) -> Result<Vec<RadioChannel>> {
        let db = self.connect()?;
        let lora = db
            .query_row("SELECT lora FROM radio_plan WHERE singleton=1", [], |row| {
                row.get::<_, Option<Vec<u8>>>(0)
            })
            .optional()?
            .flatten();
        let mut query = db.prepare("SELECT id,slot,scope,name,fingerprint,generation,verified,settings FROM radio_channels ORDER BY slot")?;
        Ok(query
            .query_map([], |row| {
                Ok(RadioChannel {
                    metadata: metadata_row(row)?,
                    settings: row.get("settings")?,
                    lora: lora.clone(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn route_radio_channel(&self, slot: u32) -> Result<Option<ChannelBinding>> {
        Ok(self.connect()?.query_row("SELECT id,slot,scope,name,fingerprint,generation,verified FROM radio_channels WHERE slot=?", [slot], metadata_row).optional()?)
    }

    /// Automatic send requires the exact accepted identity/key/slot generation to have been verified.
    pub fn validate_binding(&self, channel_id: &str, fingerprint: &str) -> Result<ChannelBinding> {
        let channel = self
            .channel_metadata(channel_id)?
            .ok_or_else(|| invalid("radio channel binding no longer exists"))?;
        if !channel.verified || channel.fingerprint != fingerprint {
            return Err(invalid(
                "radio channel binding is unverified or changed; automatic transmission refused",
            ));
        }
        Ok(channel)
    }

    pub fn enrollment(
        &self,
        channel_id: &str,
        actor: Option<&StaffIdentity>,
    ) -> Result<EnrollmentExport> {
        let mut db = self.connect()?;
        let transaction = db.transaction()?;
        let selected =
            metadata_on(&transaction, channel_id)?.ok_or_else(|| invalid("channel not found"))?;
        if selected.scope != ChannelScope::Community
            && !match actor {
                Some(actor) => access_on(&transaction, actor, &selected)?,
                None => false,
            }
        {
            return Err(invalid(
                "channel membership required for private enrollment",
            ));
        }
        let community = metadata_on(&transaction, "community")?
            .ok_or_else(|| invalid("community channel not found"))?;
        if !selected.verified || !community.verified {
            return Err(invalid(
                "channel deployment must be independently verified before enrollment export",
            ));
        }
        let primary: Vec<u8> = transaction.query_row(
            "SELECT settings FROM radio_channels WHERE scope='community' AND slot=0",
            [],
            |row| row.get(0),
        )?;
        let lora: Option<Vec<u8>> =
            transaction.query_row("SELECT lora FROM radio_plan WHERE singleton=1", [], |row| {
                row.get(0)
            })?;
        if lora.as_ref().is_none_or(|lora| lora.is_empty()) {
            return Err(invalid(
                "verified LoRa settings are required for enrollment export",
            ));
        }
        let secondary = if selected.scope == ChannelScope::Community {
            vec![]
        } else {
            vec![transaction.query_row(
                "SELECT settings FROM radio_channels WHERE id=?",
                [&selected.id],
                |row| row.get::<_, Vec<u8>>(0),
            )?]
        };
        let url = channels::share_url(&primary, &secondary, lora.as_deref())?;
        let qr_svg = channels::qr_svg(&url)?;
        transaction.commit()?;
        Ok(EnrollmentExport {
            channel: selected,
            url,
            qr_svg,
        })
    }

    pub fn grant_channel_access(
        &self,
        operator_id: &str,
        staff_id: &str,
        channel_id: &str,
    ) -> Result<()> {
        RecordId::new(staff_id)?;
        let mut db = self.connect()?;
        let transaction = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_operator(&transaction, operator_id)?;
        if persisted_role(&transaction, staff_id)?.is_none() {
            return Err(invalid("enrolled staff identity required"));
        }
        let channel =
            metadata_on(&transaction, channel_id)?.ok_or_else(|| invalid("channel not found"))?;
        if channel.scope != ChannelScope::Department {
            return Err(invalid(
                "explicit membership applies only to department channels",
            ));
        }
        transaction.execute(
            "INSERT OR IGNORE INTO channel_access VALUES(?,?)",
            params![channel_id, staff_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn revoke_channel_access(
        &self,
        operator_id: &str,
        staff_id: &str,
        channel_id: &str,
    ) -> Result<()> {
        RecordId::new(staff_id)?;
        let mut db = self.connect()?;
        let transaction = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_operator(&transaction, operator_id)?;
        let channel =
            metadata_on(&transaction, channel_id)?.ok_or_else(|| invalid("channel not found"))?;
        if channel.scope != ChannelScope::Department {
            return Err(invalid(
                "explicit membership applies only to department channels",
            ));
        }
        transaction.execute(
            "DELETE FROM channel_access WHERE channel_id=? AND staff_id=?",
            params![channel_id, staff_id],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Caller must obtain these fingerprints from the radio's independently read-back snapshot.
    /// No partial deployment becomes sendable: the complete plan is checked before all flags commit.
    pub fn mark_deployment_verified(&self, observed: &[(u32, String)]) -> Result<()> {
        let mut db = self.connect()?;
        let transaction = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let expected: BTreeSet<_> = all_metadata(&transaction)?
            .into_iter()
            .map(|channel| (channel.slot, channel.fingerprint))
            .collect();
        let actual: BTreeSet<_> = observed.iter().cloned().collect();
        if expected.is_empty() || actual != expected || actual.len() != observed.len() {
            transaction.execute("UPDATE radio_channels SET verified=0", [])?;
            transaction.commit()?;
            return Err(invalid(
                "radio read-back does not match the complete channel plan",
            ));
        }
        transaction.execute("UPDATE radio_channels SET verified=1", [])?;
        transaction.commit()?;
        Ok(())
    }

    pub fn clear_deployment_verification(&self) -> Result<()> {
        self.connect()?
            .execute("UPDATE radio_channels SET verified=0", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{MessageText, NewRecord, TransmissionState};

    fn plan() -> Result<ChannelPlan> {
        Ok(ChannelPlan {
            channels: vec![
                PlannedChannel {
                    id: "community".into(),
                    scope: ChannelScope::Community,
                    slot: 0,
                    settings: channels::generated("Event")?.settings,
                },
                PlannedChannel {
                    id: "staff".into(),
                    scope: ChannelScope::Staff,
                    slot: 1,
                    settings: channels::generated("Staff")?.settings,
                },
                PlannedChannel {
                    id: "medical".into(),
                    scope: ChannelScope::Department,
                    slot: 2,
                    settings: channels::generated("Medical")?.settings,
                },
            ],
            lora: Some(vec![0x08, 0x01]),
        })
    }
    fn identity(enrollment: &crate::domain::Enrollment) -> StaffIdentity {
        StaffIdentity {
            id: enrollment.id.clone(),
            name: enrollment.name.clone(),
            role: enrollment.role,
        }
    }
    fn verify(store: &EventStore) -> Result<()> {
        let observed: Vec<_> = store
            .radio_channels()?
            .iter()
            .map(|channel| (channel.metadata.slot, channel.metadata.fingerprint.clone()))
            .collect();
        store.mark_deployment_verified(&observed)
    }

    #[test]
    fn plan_install_is_atomic_private_and_never_overwrites_existing_slots() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let store = EventStore::new(directory.path(), 100, false)?;
        let pending = store.create(NewRecord::message(
            MessageText::new("Private legacy queue")?,
            "staff",
        )?)?;
        let plan = plan()?;
        assert_eq!(store.install_channel_plan(&plan)?, 1);
        assert_eq!(
            store.record(&pending.id)?.unwrap().state,
            TransmissionState::Uncertain
        );
        assert!(store.pending()?.is_empty());
        assert!(store.install_channel_plan(&plan).is_err());
        let public = serde_json::to_string(&store.public_channel_catalog()?)?;
        assert!(!public.contains("Staff"));
        assert!(!public.contains("Medical"));
        assert!(!public.contains("psk"));
        assert_eq!(store.public_channel_catalog()?.len(), 1);
        assert_eq!(store.radio_channels()?.len(), 3);
        assert!(store.enrollment("community", None).is_err());
        Ok(())
    }

    #[test]
    fn invalid_plans_leave_no_registered_keys_or_quarantine_side_effects() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let store = EventStore::new(directory.path(), 100, false)?;
        let pending = store.create(NewRecord::message(
            MessageText::new("Still pending")?,
            "staff",
        )?)?;
        let mut invalid_plan = plan()?;
        // Reuse a public PSK under a different private name, so the key check (not name uniqueness) catches it.
        let public_key = channels::settings_view(&invalid_plan.channels[0].settings)?.psk;
        let private_key = channels::settings_view(&invalid_plan.channels[1].settings)?.psk;
        let offset = invalid_plan.channels[1]
            .settings
            .windows(private_key.len())
            .position(|bytes| bytes == private_key.as_slice())
            .unwrap();
        invalid_plan.channels[1].settings[offset..offset + private_key.len()]
            .copy_from_slice(&public_key);
        assert!(store.install_channel_plan(&invalid_plan).is_err());
        invalid_plan = plan()?;
        invalid_plan.channels[1].slot = 0;
        assert!(store.install_channel_plan(&invalid_plan).is_err());
        invalid_plan = plan()?;
        invalid_plan.channels[2].scope = ChannelScope::Community;
        assert!(store.install_channel_plan(&invalid_plan).is_err());
        invalid_plan = plan()?;
        invalid_plan.channels.remove(0);
        assert!(store.install_channel_plan(&invalid_plan).is_err());
        assert!(store.radio_channels()?.is_empty());
        assert_eq!(
            store.record(&pending.id)?.unwrap().state,
            TransmissionState::Queued
        );
        Ok(())
    }

    #[test]
    fn private_department_enrollment_requires_current_membership_and_exports_only_selected_keys()
    -> Result<()> {
        let directory = tempfile::tempdir()?;
        let store = EventStore::new(directory.path(), 100, false)?;
        let operator = store.enroll("Owner", Role::Operator, true)?;
        let staff = store.enroll("Staff member", Role::Staff, false)?;
        let operator_actor = identity(&operator);
        let staff_actor = identity(&staff);
        store.install_channel_plan(&plan()?)?;
        verify(&store)?;
        let public = channels::import_url(&store.enrollment("community", None)?.url)?;
        assert_eq!(public.settings.len(), 1);
        assert_eq!(channels::settings_view(&public.settings[0])?.name, "Event");
        assert!(store.enrollment("staff", None).is_err());
        let private = channels::import_url(&store.enrollment("staff", Some(&staff_actor))?.url)?;
        assert_eq!(private.settings.len(), 2);
        assert_eq!(channels::settings_view(&private.settings[1])?.name, "Staff");
        assert!(!store.can_access(&staff_actor, "medical")?);
        assert!(store.enrollment("medical", Some(&staff_actor)).is_err());
        assert!(
            store
                .grant_channel_access(&staff.id, &staff.id, "medical")
                .is_err()
        );
        let fabricated = StaffIdentity {
            role: Role::Operator,
            ..staff_actor.clone()
        };
        assert!(!store.can_access(&fabricated, "medical")?);
        store.grant_channel_access(&operator.id, &staff.id, "medical")?;
        assert!(store.can_access(&staff_actor, "medical")?);
        let selected = channels::import_url(&store.enrollment("medical", Some(&staff_actor))?.url)?;
        assert_eq!(selected.settings.len(), 2);
        assert_eq!(
            channels::settings_view(&selected.settings[1])?.name,
            "Medical"
        );
        assert!(store.can_access(&operator_actor, "medical")?);
        store.revoke_channel_access(&operator.id, &staff.id, "medical")?;
        assert!(store.enrollment("medical", Some(&staff_actor)).is_err());
        store.revoke(&staff.id)?;
        assert!(store.enrollment("staff", Some(&staff_actor)).is_err());
        Ok(())
    }

    #[test]
    fn verification_is_atomic_exact_and_mismatch_stops_automatic_transmission() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let store = EventStore::new(directory.path(), 100, false)?;
        store.install_channel_plan(&plan()?)?;
        let channels = store.radio_channels()?;
        let fingerprints: Vec<_> = channels
            .iter()
            .map(|channel| (channel.metadata.slot, channel.metadata.fingerprint.clone()))
            .collect();
        assert!(store.validate_binding("staff", &fingerprints[1].1).is_err());
        assert!(store.mark_deployment_verified(&fingerprints[..2]).is_err());
        assert!(
            store
                .radio_channels()?
                .iter()
                .all(|channel| !channel.metadata.verified)
        );
        store.mark_deployment_verified(&fingerprints)?;
        assert!(
            store
                .validate_binding("staff", &fingerprints[1].1)?
                .verified
        );
        assert!(store.validate_binding("staff", &fingerprints[0].1).is_err());
        assert_eq!(store.route_radio_channel(2)?.unwrap().id, "medical");
        assert!(store.route_radio_channel(7)?.is_none());
        assert!(store.mark_deployment_verified(&fingerprints[..2]).is_err());
        assert!(store.validate_binding("staff", &fingerprints[1].1).is_err());
        store.mark_deployment_verified(&fingerprints)?;
        store.clear_deployment_verification()?;
        assert!(store.validate_binding("staff", &fingerprints[1].1).is_err());
        Ok(())
    }

    #[test]
    fn registry_and_membership_survive_consistent_backup_and_restore() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let original = EventStore::new(&directory.path().join("original"), 100, false)?;
        let owner = original.enroll("Owner", Role::Operator, true)?;
        let staff = original.enroll("Staff", Role::Staff, false)?;
        original.install_channel_plan(&plan()?)?;
        original.grant_channel_access(&owner.id, &staff.id, "medical")?;
        let observed: Vec<_> = original
            .radio_channels()?
            .iter()
            .map(|channel| (channel.metadata.slot, channel.metadata.fingerprint.clone()))
            .collect();
        original.mark_deployment_verified(&observed)?;
        let source_url = original.enrollment("medical", Some(&identity(&staff)))?.url;
        let backup = directory.path().join("backup.sqlite");
        original.backup(&backup)?;
        let spare = directory.path().join("spare");
        crate::recovery::restore_snapshot(&backup, &spare)?;
        let restored = EventStore::new(&spare, 100, false)?;
        assert!(restored.can_access(&identity(&staff), "medical")?);
        assert!(
            restored
                .radio_channels()?
                .iter()
                .all(|channel| !channel.metadata.verified)
        );
        assert!(
            restored
                .enrollment("medical", Some(&identity(&staff)))
                .is_err()
        );
        verify(&restored)?;
        assert_eq!(
            restored.enrollment("medical", Some(&identity(&staff)))?.url,
            source_url
        );
        assert_eq!(restored.public_channel_catalog()?.len(), 1);
        Ok(())
    }

    #[test]
    fn foreign_device_slots_can_remain_between_registered_channels() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let store = EventStore::new(directory.path(), 100, false)?;
        let mut plan = plan()?;
        plan.channels[1].slot = 3;
        plan.channels[2].slot = 6;
        store.install_channel_plan(&plan)?;
        assert_eq!(store.channel_metadata("staff")?.unwrap().slot, 3);
        assert!(store.route_radio_channel(1)?.is_none());
        assert_eq!(
            store.route_radio_channel(6)?.unwrap().scope,
            ChannelScope::Department
        );
        Ok(())
    }

    #[test]
    fn missing_lora_never_exports_a_provisioning_qr_even_if_slot_flags_are_verified() -> Result<()>
    {
        let directory = tempfile::tempdir()?;
        let store = EventStore::new(directory.path(), 100, false)?;
        let mut plan = plan()?;
        plan.lora = None;
        store.install_channel_plan(&plan)?;
        verify(&store)?;
        assert!(store.enrollment("community", None).is_err());
        assert_eq!(store.public_channel_catalog()?.len(), 1);
        Ok(())
    }

    #[test]
    fn existing_default_unencrypted_and_128_bit_community_channels_can_be_joined() -> Result<()> {
        for key in [vec![], vec![1], vec![0xAB; 16]] {
            let directory = tempfile::tempdir()?;
            let store = EventStore::new(directory.path(), 100, false)?;
            let mut plan = plan()?;
            let settings = if key.is_empty() {
                vec![]
            } else {
                let mut settings = vec![0x12, key.len() as u8];
                settings.extend_from_slice(&key);
                settings
            };
            plan.channels[0].settings = settings;
            store.install_channel_plan(&plan)?;
            verify(&store)?;
            let enrolled = channels::import_url(&store.enrollment("community", None)?.url)?;
            assert_eq!(enrolled.settings.len(), 1);
            let view = channels::settings_view(&enrolled.settings[0])?;
            assert_eq!(view.psk, key);
            assert!(view.name.is_empty());
        }
        Ok(())
    }
}
