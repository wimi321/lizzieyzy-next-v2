//! Commentary DTOs and channel-only application API. Secrets never enter settings.
use crate::{AppCore, AppError, CommentaryKeyStatus, actor};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use ts_rs::TS;

mod settings;
mod worker;
pub use settings::*;
pub(crate) use worker::{Generation, generate};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS, thiserror::Error)]
#[serde(tag = "type", content = "data")]
pub enum CommentaryError {
    #[error("commentary service is not configured")]
    NotConfigured,
    #[error("no analyzed candidates are available")]
    NoAnalysis,
    #[error("a commentary request is already running")]
    Busy,
    #[error("invalid commentary settings: {0}")]
    InvalidSettings(String),
    #[error("invalid commentary range")]
    InvalidRange,
    #[error("no previous commentary is available")]
    NoPrevious,
    #[error("commentary position changed")]
    PositionChanged,
    #[error("commentary prompt or answer exceeds its limit")]
    TooLong,
    #[error("commentary evidence could not be prepared")]
    InvalidEvidence,
    #[error("commentary could not be written to the document")]
    WriteFailed,
    #[error(transparent)]
    Client(#[from] crate::commentary_client::CommentaryClientError),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum CommentaryMode {
    #[default]
    NextMove,
    Range {
        from: u32,
        to: u32,
    },
    WholeGame,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct CommentaryRequest {
    #[ts(as = "Option<CommentaryMode>", optional)]
    pub mode: CommentaryMode,
    #[ts(optional = nullable)]
    pub follow_up: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum CommentaryPhase {
    Preparing,
    Generating,
    Verifying,
    Completed,
    Failed,
    #[default]
    Stopped,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct CommentaryPosition {
    #[ts(as = "Option<u32>", optional)]
    pub node_id: u32,
    #[ts(as = "Option<u32>", optional)]
    pub move_number: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct CommentaryUpdated {
    #[ts(as = "Option<u32>", optional)]
    pub request_id: u32,
    #[ts(as = "Option<CommentaryPhase>", optional)]
    pub phase: CommentaryPhase,
    /// Authoritative full answer; safe when intermediate events are coalesced.
    #[ts(as = "Option<String>", optional)]
    pub text: String,
    #[ts(as = "Option<String>", optional)]
    pub delta: String,
    /// A correction starts a new answer, replacing the draft.
    #[ts(as = "Option<bool>", optional)]
    pub replacement: bool,
    #[ts(as = "Option<Vec<CommentaryPosition>>", optional)]
    pub positions: Vec<CommentaryPosition>,
    #[ts(as = "Option<String>", optional)]
    pub evidence: String,
    #[ts(as = "Option<u32>", optional)]
    pub analyzed: u32,
    #[ts(as = "Option<u32>", optional)]
    pub selected: u32,
    #[ts(as = "Option<u32>", optional)]
    pub omitted: u32,
    #[ts(as = "Option<Vec<String>>", optional)]
    pub issues: Vec<String>,
    #[ts(optional = nullable)]
    pub error: Option<CommentaryError>,
}

pub(crate) enum Request {
    Start(
        CommentaryRequest,
        oneshot::Sender<Result<CommentaryUpdated, AppError>>,
    ),
    Stop(oneshot::Sender<CommentaryUpdated>),
    Status(oneshot::Sender<CommentaryUpdated>),
    Settings(
        CommentarySettings,
        oneshot::Sender<Result<CommentarySettings, AppError>>,
    ),
    SetKey(
        String,
        bool,
        oneshot::Sender<Result<CommentaryKeyStatus, AppError>>,
    ),
    ClearKey(oneshot::Sender<Result<CommentaryKeyStatus, AppError>>),
    KeyStatus(oneshot::Sender<CommentaryKeyStatus>),
    Connection(oneshot::Sender<Result<Connection, AppError>>),
}

// Deliberately no Debug/Serialize: this value contains a credential.
pub(crate) struct Connection {
    pub client: crate::commentary_client::CommentaryClient,
    pub key: String,
    pub model: String,
}

impl AppCore {
    async fn commentary_send(&self, request: Request) -> Result<(), AppError> {
        self.commands
            .send(actor::Command::Commentary(request))
            .await
            .map_err(|_| AppError::Closed)
    }
    pub async fn start_commentary(
        &self,
        request: CommentaryRequest,
    ) -> Result<CommentaryUpdated, AppError> {
        let (tx, rx) = oneshot::channel();
        self.commentary_send(Request::Start(request, tx)).await?;
        rx.await.map_err(|_| AppError::Closed)?
    }
    pub async fn stop_commentary(&self) -> Result<CommentaryUpdated, AppError> {
        let (tx, rx) = oneshot::channel();
        self.commentary_send(Request::Stop(tx)).await?;
        rx.await.map_err(|_| AppError::Closed)
    }
    pub async fn commentary_status(&self) -> Result<CommentaryUpdated, AppError> {
        let (tx, rx) = oneshot::channel();
        self.commentary_send(Request::Status(tx)).await?;
        rx.await.map_err(|_| AppError::Closed)
    }
    pub async fn set_commentary_settings(
        &self,
        settings: CommentarySettings,
    ) -> Result<CommentarySettings, AppError> {
        let (tx, rx) = oneshot::channel();
        self.commentary_send(Request::Settings(settings, tx))
            .await?;
        rx.await.map_err(|_| AppError::Closed)?
    }
    pub async fn set_commentary_key(
        &self,
        key: String,
        remember: bool,
    ) -> Result<CommentaryKeyStatus, AppError> {
        let (tx, rx) = oneshot::channel();
        self.commentary_send(Request::SetKey(key, remember, tx))
            .await?;
        rx.await.map_err(|_| AppError::Closed)?
    }
    pub async fn clear_commentary_key(&self) -> Result<CommentaryKeyStatus, AppError> {
        let (tx, rx) = oneshot::channel();
        self.commentary_send(Request::ClearKey(tx)).await?;
        rx.await.map_err(|_| AppError::Closed)?
    }
    pub async fn commentary_key_status(&self) -> Result<CommentaryKeyStatus, AppError> {
        let (tx, rx) = oneshot::channel();
        self.commentary_send(Request::KeyStatus(tx)).await?;
        rx.await.map_err(|_| AppError::Closed)
    }
    pub async fn commentary_models(&self) -> Result<Vec<String>, AppError> {
        let (tx, rx) = oneshot::channel();
        self.commentary_send(Request::Connection(tx)).await?;
        let connection = rx.await.map_err(|_| AppError::Closed)??;
        connection
            .client
            .models(&connection.key)
            .await
            .map_err(|e| CommentaryError::from(e).into())
    }
}

pub(crate) fn domain_error(error: ::commentary::Error) -> CommentaryError {
    match error {
        ::commentary::Error::NoAnalysis => CommentaryError::NoAnalysis,
        ::commentary::Error::PromptTooLong | ::commentary::Error::CommentaryTooLong => {
            CommentaryError::TooLong
        }
        _ => CommentaryError::InvalidEvidence,
    }
}
pub(crate) use settings::language;
