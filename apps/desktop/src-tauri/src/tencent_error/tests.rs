#![allow(clippy::unwrap_used)]

use app_core::{
    AppError,
    fox::FoxError,
    tencent::{OpenUrlError, TencentError},
};
use serde_json::json;

use crate::error::CommandError;

#[test]
fn tencent_failures_preserve_api_details_and_share_transport_categories() {
    for (error, expected) in [
        (TencentError::InvalidQuery, json!({"type": "InvalidQuery"})),
        (
            TencentError::Api {
                result: 3,
                ret: Some(7),
                message: "fixture rejection".into(),
            },
            json!({"type": "Api", "data": {"result": 3, "ret": 7, "message": "fixture rejection"}}),
        ),
        (
            TencentError::Api {
                result: 0,
                ret: Some(9),
                message: String::new(),
            },
            json!({"type": "Api", "data": {"result": 0, "ret": 9, "message": ""}}),
        ),
        (
            TencentError::Common(FoxError::Timeout),
            json!({"type": "Request", "data": {"type": "Timeout"}}),
        ),
        (
            TencentError::Common(FoxError::Http { status: 503 }),
            json!({"type": "Request", "data": {"type": "Http", "data": {"status": 503}}}),
        ),
        (
            TencentError::Common(FoxError::InvalidResponse("chesslist")),
            json!({"type": "Request", "data": {"type": "InvalidResponse"}}),
        ),
        (
            TencentError::Common(FoxError::ResponseTooLarge),
            json!({"type": "Request", "data": {"type": "ResponseTooLarge"}}),
        ),
    ] {
        assert_eq!(
            serde_json::to_value(CommandError::from(AppError::Tencent(error))).unwrap(),
            json!({"type": "Tencent", "data": expected})
        );
    }
}

#[test]
fn online_url_failures_keep_routing_parsing_and_http_errors_distinct() {
    for (error, expected) in [
        (OpenUrlError::InvalidUrl, json!({"type": "InvalidUrl"})),
        (
            OpenUrlError::UnsupportedScheme,
            json!({"type": "UnsupportedScheme"}),
        ),
        (
            OpenUrlError::UnsupportedYike,
            json!({"type": "UnsupportedYike"}),
        ),
        (
            OpenUrlError::RedirectLimit,
            json!({"type": "RedirectLimit"}),
        ),
        (
            OpenUrlError::InvalidRedirect,
            json!({"type": "InvalidRedirect"}),
        ),
        (OpenUrlError::InvalidSgf, json!({"type": "InvalidSgf"})),
        (
            OpenUrlError::Request(FoxError::Http { status: 403 }),
            json!({"type": "Request", "data": {"type": "Http", "data": {"status": 403}}}),
        ),
        (
            OpenUrlError::Request(FoxError::DownloadFailed {
                failures: vec![FoxError::Timeout],
            }),
            json!({"type": "Request", "data": {"type": "Timeout"}}),
        ),
    ] {
        assert_eq!(
            serde_json::to_value(CommandError::from(AppError::OpenUrl(error))).unwrap(),
            json!({"type": "OpenUrl", "data": expected})
        );
    }
}
