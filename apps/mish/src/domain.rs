//! Validated command values and explicit delivery states, independent of I/O.
use anyhow::Result;
use serde::{Deserialize, Deserializer, Serialize};
use std::str::FromStr;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Text<const MAX: usize>(String);

impl<const MAX: usize> Text<MAX> {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.trim().is_empty() || value.len() > MAX {
            return Err(crate::invalid(format!(
                "text must be nonempty and at most {MAX} UTF-8 bytes"
            )));
        }
        Ok(Self(value.trim().to_owned()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl<'de, const MAX: usize> Deserialize<'de> for Text<MAX> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}
pub type MessageText = Text<160>;
pub type RadioText = Text<200>;
pub type StaffName = Text<80>;
pub type AnnouncementText = Text<4000>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct RecordId(String);
impl RecordId {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 128
            || !value.as_bytes()[0].is_ascii_alphanumeric()
            || !value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        {
            return Err(crate::invalid(
                "identifier must contain 1–128 ASCII letters, digits, hyphens or underscores",
            ));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl<'de> Deserialize<'de> for RecordId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

macro_rules! string_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
        pub enum $name { $(#[serde(rename = $value)] $variant),+ }
        impl $name {
            pub fn as_str(self) -> &'static str { match self { $(Self::$variant => $value),+ } }
        }
        impl FromStr for $name {
            type Err = anyhow::Error;
            fn from_str(value: &str) -> Result<Self> {
                match value { $($value => Ok(Self::$variant)),+, _ => Err(crate::invalid(concat!("invalid ", stringify!($name)))) }
            }
        }
    };
}
string_enum!(Role { Staff => "staff", Operator => "operator" });
string_enum!(MessageKind { Message => "message", Alert => "alert", Urgent => "urgent" });
string_enum!(TransmissionState {
    Queued => "queued", Sending => "sending", Submitted => "submitted", Confirmed => "confirmed",
    Uncertain => "uncertain", Failed => "failed", Expired => "expired", Received => "received"
});
string_enum!(AcknowledgementSource { Browser => "browser", Radio => "radio" });

#[derive(Clone, Debug)]
pub enum TransmissionUpdate {
    Claim,
    Submitted { packet_id: String },
    Uncertain,
    Failed,
    Expired,
}
impl TransmissionState {
    pub fn transition(self, update: &TransmissionUpdate) -> Option<Self> {
        use TransmissionState::*;
        use TransmissionUpdate as U;
        match (self, update) {
            (Queued, U::Claim) => Some(Sending),
            (Sending, U::Submitted { .. }) => Some(Submitted),
            (Sending | Submitted, U::Uncertain) => Some(Uncertain),
            (Queued | Sending, U::Failed) => Some(Failed),
            (Queued | Sending, U::Expired) => Some(Expired),
            _ => None,
        }
    }
    pub fn transport_confirmed(self) -> Option<Self> {
        matches!(self, Self::Submitted | Self::Uncertain).then_some(Self::Confirmed)
    }
}

#[derive(Clone, Debug)]
pub struct NewRecord {
    pub(crate) text: MessageText,
    pub(crate) author: String,
    pub(crate) kind: MessageKind,
    pub(crate) responder: Option<StaffName>,
    pub(crate) expires: Option<f64>,
}
impl NewRecord {
    pub fn message(text: MessageText, author: impl Into<String>) -> Result<Self> {
        let author = StaffName::new(author)?.as_str().to_owned();
        Ok(Self {
            text,
            author,
            kind: MessageKind::Message,
            responder: None,
            expires: None,
        })
    }
    pub fn alert(
        text: MessageText,
        author: impl Into<String>,
        responder: StaffName,
        expires: f64,
        urgent: bool,
        now: f64,
    ) -> Result<Self> {
        if !expires.is_finite() || expires <= now || expires > now + 86400.0 {
            return Err(crate::invalid("alerts require expiration within 24 hours"));
        }
        Ok(Self {
            text,
            author: StaffName::new(author)?.as_str().to_owned(),
            kind: if urgent {
                MessageKind::Urgent
            } else {
                MessageKind::Alert
            },
            responder: Some(responder),
            expires: Some(expires),
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StaffIdentity {
    pub id: String,
    pub name: String,
    pub role: Role,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Enrollment {
    pub id: String,
    pub name: String,
    pub role: Role,
    pub token: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Acknowledgement {
    pub record_id: String,
    pub actor: String,
    pub source: AcknowledgementSource,
    pub node: String,
    pub channel: i64,
    pub created: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Delivery {
    pub persisted: bool,
    pub queue: String,
    pub radio_submitted_at: Option<f64>,
    pub transport_confirmed_at: Option<f64>,
    pub uncertain_at: Option<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    pub id: String,
    pub text: String,
    pub kind: MessageKind,
    pub author: String,
    pub created: f64,
    pub expires: Option<f64>,
    pub responder: Option<String>,
    pub state: TransmissionState,
    pub packet_id: Option<String>,
    pub submitted_at: Option<f64>,
    pub confirmed_at: Option<f64>,
    pub uncertain_at: Option<f64>,
    pub author_name: String,
    pub acknowledgements: Vec<Acknowledgement>,
    pub expired: bool,
    pub delivery: Delivery,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingRecord {
    pub id: String,
    pub text: String,
    pub kind: MessageKind,
    pub expires: Option<f64>,
}
/// Attendee projection deliberately has no staff author, source ID, position or ACK fields.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Announcement {
    pub id: String,
    pub text: String,
    pub published: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utf8_and_identifiers_are_bounded_at_input() {
        assert!(MessageText::new("💧".repeat(41)).is_err());
        assert!(MessageText::new("💧".repeat(40)).is_ok());
        for input in ["", "../outside", "/tmp/file", "a/b", "a\\b", ".", "💧"] {
            assert!(RecordId::new(input).is_err());
        }
        assert!(serde_json::from_str::<MessageText>("12").is_err());
        assert!(serde_json::from_str::<MessageText>("\"  \"").is_err());
    }
    #[test]
    fn received_and_uncertain_records_cannot_be_claimed_or_requeued() {
        for state in [
            TransmissionState::Received,
            TransmissionState::Uncertain,
            TransmissionState::Expired,
            TransmissionState::Confirmed,
        ] {
            assert_eq!(state.transition(&TransmissionUpdate::Claim), None);
        }
        assert_eq!(TransmissionState::Queued.transport_confirmed(), None);
        assert_eq!(
            TransmissionState::Submitted.transport_confirmed(),
            Some(TransmissionState::Confirmed)
        );
        assert_eq!(
            TransmissionState::Uncertain.transport_confirmed(),
            Some(TransmissionState::Confirmed)
        );
    }

    #[test]
    fn generated_delivery_sequences_never_replay_ambiguous_sends() {
        // Exhaust every five-operation history over claim, submit, ambiguity,
        // failure, expiry and confirmation. Ambiguity must permanently close
        // the automatic transmission path, even when later ACKs arrive.
        for mut sequence in 0usize..6usize.pow(5) {
            let mut state = TransmissionState::Queued;
            let mut ambiguous = false;
            for _ in 0..5 {
                let operation = sequence % 6;
                sequence /= 6;
                let update = match operation {
                    0 => TransmissionUpdate::Claim,
                    1 => TransmissionUpdate::Submitted {
                        packet_id: "packet".into(),
                    },
                    2 => TransmissionUpdate::Uncertain,
                    3 => TransmissionUpdate::Failed,
                    _ => TransmissionUpdate::Expired,
                };
                if operation == 5 {
                    state = state.transport_confirmed().unwrap_or(state);
                } else {
                    state = state.transition(&update).unwrap_or(state);
                }
                ambiguous |= state == TransmissionState::Uncertain;
                if ambiguous {
                    assert!(!matches!(
                        state,
                        TransmissionState::Queued
                            | TransmissionState::Sending
                            | TransmissionState::Submitted
                    ));
                    assert!(state.transition(&TransmissionUpdate::Claim).is_none());
                }
            }
        }
    }
}
