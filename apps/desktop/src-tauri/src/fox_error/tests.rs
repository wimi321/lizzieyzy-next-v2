#![allow(clippy::unwrap_used)]

use app_core::{AppError, fox::FoxError};
use serde_json::json;

use crate::error::CommandError;

#[test]
fn account_not_found_is_distinct_from_api_transport_and_download_failures() {
    for (error, expected) in [
        (FoxError::UserNotFound, json!({"type": "UserNotFound"})),
        (
            FoxError::Api { code: 5 },
            json!({"type": "Api", "data": {"code": 5}}),
        ),
        (
            FoxError::Http { status: 503 },
            json!({"type": "Http", "data": {"status": 503}}),
        ),
        (FoxError::Timeout, json!({"type": "Timeout"})),
        (
            FoxError::InvalidResponse("chesslist"),
            json!({"type": "InvalidResponse"}),
        ),
        (
            FoxError::ResponseTooLarge,
            json!({"type": "ResponseTooLarge"}),
        ),
        (
            FoxError::DownloadFailed {
                failures: vec![FoxError::Timeout],
            },
            json!({"type": "DownloadFailed"}),
        ),
    ] {
        let error = CommandError::from(AppError::Fox(error));
        assert_eq!(
            serde_json::to_value(error).unwrap(),
            json!({"type": "Fox", "data": expected})
        );
    }
}
