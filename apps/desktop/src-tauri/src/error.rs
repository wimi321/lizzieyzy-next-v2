use serde::Serialize;
use ts_rs::TS;

use crate::fox_error::FoxFailure;

/// Serializable IPC failures. Domain errors are rendered only at this boundary.
#[derive(Debug, thiserror::Error, Serialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum CommandError {
    #[error("{0}")]
    App(String),
    #[error("{0}")]
    Fox(FoxFailure),
    #[error("event stream is closed")]
    StreamClosed,
    #[error("subscription identifiers are exhausted")]
    SubscriptionLimit,
    #[error("cannot send event: {0}")]
    Channel(String),
}

impl From<app_core::AppError> for CommandError {
    fn from(error: app_core::AppError) -> Self {
        match error {
            app_core::AppError::Fox(error) => Self::Fox(error.into()),
            other => Self::App(other.to_string()),
        }
    }
}
