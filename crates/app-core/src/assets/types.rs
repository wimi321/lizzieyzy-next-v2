use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum DownloadPhase {
    #[default]
    Idle,
    Downloading,
    Installing,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AssetDownloadStatus {
    #[ts(as = "Option<DownloadPhase>", optional)]
    pub phase: DownloadPhase,
    #[ts(optional = nullable)]
    pub asset_id: Option<String>,
    #[ts(as = "Option<f64>", optional)]
    pub downloaded_bytes: u64,
    #[ts(as = "Option<f64>", optional)]
    pub total_bytes: u64,
    #[ts(optional = nullable)]
    pub path: Option<String>,
    #[ts(optional = nullable)]
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OfficialWeight {
    #[ts(as = "Option<String>", optional)]
    pub id: String,
    #[ts(as = "Option<String>", optional)]
    pub name: String,
    #[ts(as = "Option<f64>", optional)]
    pub size_bytes: u64,
    #[ts(optional = nullable)]
    pub published_at: Option<String>,
    #[ts(as = "Option<bool>", optional)]
    pub downloaded: bool,
    #[ts(optional = nullable)]
    pub path: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum AccelerationReason {
    #[default]
    UnsupportedPlatform,
    NoNvidiaGpu,
    NvidiaDetected,
    DetectionFailed,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AccelerationPlan {
    #[ts(as = "Option<AccelerationReason>", optional)]
    pub reason: AccelerationReason,
    #[ts(as = "Option<Vec<String>>", optional)]
    pub recommended_assets: Vec<String>,
    #[ts(as = "Option<Vec<String>>", optional)]
    pub optional_assets: Vec<String>,
    #[ts(optional = nullable)]
    pub runtime_profile: Option<String>,
    #[ts(as = "Option<bool>", optional)]
    pub tensor_rt_supported: bool,
    #[ts(as = "Option<bool>", optional)]
    pub tensor_rt_recommended: bool,
    // A verified archive is not an installed runtime or a successful GPU probe.
    #[ts(as = "Option<bool>", optional)]
    pub archives_only: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    #[error("invalid trusted asset catalog: {0}")]
    Catalog(String),
    #[error("unknown asset: {0}")]
    UnknownAsset(String),
    #[error("another asset is downloading")]
    Busy,
    #[error("asset downloader is closed")]
    Closed,
    #[error("asset file operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("asset download failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("asset size or SHA-256 verification failed")]
    Verification,
    #[error("asset is not downloaded and verified")]
    NotDownloaded,
    #[error("asset worker failed: {0}")]
    Worker(String),
    #[error("asset is not applicable to this system")]
    NotApplicable,
    #[error("unsafe or incomplete runtime archive: {0}")]
    Archive(String),
    #[error("asset installation cancelled")]
    Cancelled,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RuntimeInstallation {
    #[ts(as = "Option<String>", optional)]
    pub profile_id: String,
    #[ts(as = "Option<bool>", optional)]
    pub ready: bool,
    /// The DLL search directory; all runtime DLLs and katago.exe are here.
    #[ts(optional = nullable)]
    pub directory: Option<std::path::PathBuf>,
    #[ts(optional = nullable)]
    pub program: Option<std::path::PathBuf>,
    #[ts(as = "Option<Vec<String>>", optional)]
    pub missing: Vec<String>,
    #[ts(optional = nullable)]
    pub error: Option<String>,
}
