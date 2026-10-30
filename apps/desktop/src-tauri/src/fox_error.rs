//! Stable IPC error variants; callers can localize failures without parsing text.
use app_core::fox::FoxError;
use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, thiserror::Error, Serialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum FoxFailure {
    #[error("Fox account not found")]
    UserNotFound,
    #[error("Fox nickname must not be empty")]
    InvalidNickname,
    #[error("Fox UID must contain ASCII digits only")]
    InvalidUid,
    #[error("Fox game ID or page cursor is invalid")]
    InvalidGameId,
    #[error("invalid Fox client configuration")]
    InvalidConfiguration,
    #[error("Fox request timed out")]
    Timeout,
    #[error("Fox network request failed")]
    Network,
    #[error("Fox HTTP status {status}")]
    Http { status: u16 },
    #[error("Fox API returned error code {code}")]
    Api {
        #[ts(type = "number")]
        code: i64,
    },
    #[error("invalid Fox server response")]
    InvalidResponse,
    #[error("Fox response exceeds the size limit")]
    ResponseTooLarge,
    #[error("all Fox SGF endpoints failed")]
    DownloadFailed,
}

impl From<FoxError> for FoxFailure {
    fn from(error: FoxError) -> Self {
        match error {
            FoxError::UserNotFound => Self::UserNotFound,
            FoxError::InvalidNickname => Self::InvalidNickname,
            FoxError::InvalidUid => Self::InvalidUid,
            FoxError::InvalidGameId => Self::InvalidGameId,
            FoxError::InvalidConfiguration => Self::InvalidConfiguration,
            FoxError::Timeout => Self::Timeout,
            FoxError::Transport(_) => Self::Network,
            FoxError::Http { status } => Self::Http { status },
            FoxError::Api { code } => Self::Api { code },
            FoxError::Json(_) | FoxError::InvalidResponse(_) => Self::InvalidResponse,
            FoxError::ResponseTooLarge => Self::ResponseTooLarge,
            FoxError::DownloadFailed { .. } => Self::DownloadFailed,
        }
    }
}

#[cfg(test)]
mod tests;
