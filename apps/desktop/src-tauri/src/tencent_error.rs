//! Online kifu failures retain typed data and share the Fox transport mapping.
use app_core::tencent::{OpenUrlError, TencentError};
use serde::Serialize;
use ts_rs::TS;

use crate::fox_error::FoxFailure;

#[derive(Debug, thiserror::Error, Serialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum TencentFailure {
    #[error("Tencent query is invalid")]
    InvalidQuery,
    #[error("Tencent API returned error code {result}")]
    Api {
        #[ts(type = "number")]
        result: i64,
        #[ts(type = "number | null")]
        ret: Option<i64>,
        message: String,
    },
    #[error("{0}")]
    Request(FoxFailure),
}

impl From<TencentError> for TencentFailure {
    fn from(error: TencentError) -> Self {
        match error {
            TencentError::InvalidQuery => Self::InvalidQuery,
            TencentError::Api {
                result,
                ret,
                message,
            } => Self::Api {
                result,
                ret,
                message,
            },
            TencentError::Common(error) => Self::Request(error.into()),
        }
    }
}

#[derive(Debug, thiserror::Error, Serialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum OpenUrlFailure {
    #[error("online URL is invalid")]
    InvalidUrl,
    #[error("online URL must use HTTP or HTTPS")]
    UnsupportedScheme,
    #[error("Yike links are not supported yet")]
    UnsupportedYike,
    #[error("online URL exceeded the redirect limit")]
    RedirectLimit,
    #[error("online URL redirect is invalid")]
    InvalidRedirect,
    #[error("{0}")]
    Request(FoxFailure),
    #[error("online response is not a valid game record")]
    InvalidSgf,
}

impl From<OpenUrlError> for OpenUrlFailure {
    fn from(error: OpenUrlError) -> Self {
        match error {
            OpenUrlError::InvalidUrl => Self::InvalidUrl,
            OpenUrlError::UnsupportedScheme => Self::UnsupportedScheme,
            OpenUrlError::UnsupportedYike => Self::UnsupportedYike,
            OpenUrlError::RedirectLimit => Self::RedirectLimit,
            OpenUrlError::InvalidRedirect => Self::InvalidRedirect,
            OpenUrlError::Request(error) => Self::Request(error.into()),
            OpenUrlError::InvalidSgf => Self::InvalidSgf,
        }
    }
}

#[cfg(test)]
mod tests;
