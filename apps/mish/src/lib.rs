//! Mish: one authoritative offline event coordinator.

pub mod appliance;
pub mod channel_store;
pub mod channels;
pub mod coordinator;
pub mod domain;
pub mod provision;
pub mod radio;
pub mod recovery;
pub mod server;
pub mod storage;
pub mod validation;

#[derive(Debug)]
pub struct InputError(pub String);

impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for InputError {}

pub fn invalid(message: impl Into<String>) -> anyhow::Error {
    InputError(message.into()).into()
}

pub mod community;
pub mod community_http;
pub mod radio_endpoint;
