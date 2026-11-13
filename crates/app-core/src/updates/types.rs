use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UpdateCheck {
    #[ts(as = "Option<bool>", optional)]
    pub available: bool,
    #[ts(optional = nullable)]
    pub version: Option<String>,
    #[ts(optional = nullable)]
    pub notes: Option<String>,
    #[ts(optional = nullable)]
    pub published_at: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum UpdatePhase {
    #[default]
    Idle,
    Downloading,
    Downloaded,
    Installing,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UpdateDownloadStatus {
    #[ts(as = "Option<UpdatePhase>", optional)]
    pub phase: UpdatePhase,
    #[ts(as = "Option<f64>", optional)]
    pub downloaded_bytes: u64,
    #[ts(as = "Option<f64>", optional = nullable)]
    pub total_bytes: Option<u64>,
    #[ts(optional = nullable)]
    pub version: Option<String>,
    #[ts(optional = nullable)]
    pub error: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("this build has no update signing key configured")]
    SigningNotConfigured,
    #[error("no update is available; check for updates first")]
    NotAvailable,
    #[error("download and verify the update before installing")]
    NotDownloaded,
    #[error("an update operation is already running")]
    Busy,
    #[error("update service is closed or unavailable")]
    Closed,
    #[error("update SHA-256 verification failed")]
    Verification,
    #[error("update backend: {0}")]
    Backend(String),
}

/// Trusted backend metadata. The UI cannot supply URLs, hashes or install paths.
#[derive(Clone, Debug)]
pub struct UpdateRelease {
    pub view: UpdateCheck,
    pub sha256: [u8; 32],
}
