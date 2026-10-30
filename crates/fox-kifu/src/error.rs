use thiserror::Error;

#[derive(Debug, Error)]
pub enum FoxError {
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
    #[error("Fox network request timed out")]
    Timeout,
    #[error("Fox network request failed")]
    Transport(#[source] reqwest::Error),
    #[error("Fox HTTP status {status}")]
    Http { status: u16 },
    #[error("Fox API returned error code {code}")]
    Api { code: i64 },
    #[error("invalid Fox JSON response: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid Fox response field: {0}")]
    InvalidResponse(&'static str),
    #[error("Fox response exceeds the size limit")]
    ResponseTooLarge,
    #[error("all Fox SGF endpoints failed")]
    DownloadFailed { failures: Vec<FoxError> },
}

impl From<reqwest::Error> for FoxError {
    fn from(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            Self::Timeout
        } else {
            // Do not include request parameters in the public error text.
            Self::Transport(error.without_url())
        }
    }
}

impl FoxError {
    /// Stable network-failure classification, independent of OS socket details.
    /// A deadline may expire while connecting or reading; interrupted bodies
    /// may be wrapped as request, body or decode errors by the HTTP stack.
    /// HTTP status, complete-but-invalid JSON and API failures are not retried.
    pub fn is_retryable_transport(&self) -> bool {
        match self {
            Self::Timeout => true,
            Self::Transport(error) => {
                // Response::chunk reports interrupted HTTP bodies as decode
                // errors. JSON parsing happens separately and is never retried.
                error.is_connect() || error.is_request() || error.is_body() || error.is_decode()
            }
            _ => false,
        }
    }
}
