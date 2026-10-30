use serde::Serialize;

/// Serializable IPC failures. Domain errors are rendered only at this boundary.
#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum CommandError {
    #[error("{0}")]
    App(String),
    #[error("event stream is closed")]
    StreamClosed,
    #[error("subscription identifiers are exhausted")]
    SubscriptionLimit,
    #[error("cannot send event: {0}")]
    Channel(String),
}

impl From<app_core::AppError> for CommandError {
    fn from(error: app_core::AppError) -> Self {
        Self::App(error.to_string())
    }
}
