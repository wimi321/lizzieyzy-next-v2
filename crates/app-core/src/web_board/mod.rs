//! LAN spectator protocol and transport; mutable session state belongs to AppCore.
pub(crate) mod addresses;
pub(crate) mod connection;
pub mod protocol;
pub(crate) mod security;
pub(crate) mod server;

pub use protocol::*;
use std::fmt::Debug;

#[derive(Clone, Debug)]
pub struct WebBoardAsset {
    pub bytes: Vec<u8>,
    pub mime: String,
}

/// Read-only embedded assets. The server never reads arbitrary filesystem paths.
pub trait WebBoardAssets: Debug + Send + Sync {
    fn get(&self, path: &str) -> Option<WebBoardAsset>;
}

#[derive(Debug)]
pub(crate) struct EmptyAssets;
impl WebBoardAssets for EmptyAssets {
    fn get(&self, _path: &str) -> Option<WebBoardAsset> {
        None
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, thiserror::Error, serde::Serialize, serde::Deserialize, ts_rs::TS,
)]
#[serde(tag = "type", content = "data")]
pub enum WebBoardError {
    #[error("cannot bind web board: {reason}")]
    Bind { reason: String },
    #[error("cannot obtain secure random bytes")]
    Random,
    #[error("a spectator owns the trial; end it before changing the host document")]
    HostBusy,
    #[error("a spectator trial is active")]
    TrialActive,
    #[error("trial branch index is invalid")]
    InvalidBranch,
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct TestConfig {
    pub bind: std::net::Ipv4Addr,
    pub start_port: u16,
    pub analysis_interval: std::time::Duration,
    pub idle_timeout: std::time::Duration,
    pub reconnect_timeout: std::time::Duration,
}

#[cfg(test)]
impl Default for TestConfig {
    fn default() -> Self {
        Self {
            bind: std::net::Ipv4Addr::LOCALHOST,
            start_port: 0,
            analysis_interval: std::time::Duration::from_millis(100),
            idle_timeout: std::time::Duration::from_secs(300),
            reconnect_timeout: std::time::Duration::from_secs(30),
        }
    }
}

#[cfg(test)]
mod tests;
