#![allow(clippy::unwrap_used)]

use std::time::Duration;

use app_core::{
    AppError,
    fox::{FoxClient, FoxEndpoints, FoxError},
};
use serde_json::json;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::oneshot,
};

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
            json!({"type": "Timeout"}),
        ),
    ] {
        let error = CommandError::from(AppError::Fox(error));
        assert_eq!(
            serde_json::to_value(error).unwrap(),
            json!({"type": "Fox", "data": expected})
        );
    }
}

fn assert_failure(error: FoxError, kind: &str, message: &str) {
    let error = CommandError::from(AppError::Fox(error));
    assert_eq!(error.to_string(), message);
    assert_eq!(
        serde_json::to_value(error).unwrap(),
        json!({"type": "Fox", "data": {"type": kind}})
    );
}

#[test]
fn nested_download_timeouts_remain_visible_among_protocol_failures() {
    for failures in [
        vec![FoxError::Timeout, FoxError::Http { status: 503 }],
        vec![FoxError::Http { status: 503 }, FoxError::Timeout],
        vec![
            FoxError::Api { code: 5 },
            FoxError::DownloadFailed {
                failures: vec![
                    FoxError::InvalidResponse("chess"),
                    FoxError::DownloadFailed {
                        failures: vec![FoxError::Timeout],
                    },
                ],
            },
        ],
    ] {
        assert_failure(
            FoxError::DownloadFailed { failures },
            "Timeout",
            "Fox network request timed out",
        );
    }
}

#[test]
fn empty_and_non_network_download_failures_keep_their_category() {
    for failures in [
        vec![],
        vec![FoxError::Api { code: 5 }, FoxError::Http { status: 503 }],
        vec![FoxError::DownloadFailed {
            failures: vec![
                FoxError::InvalidResponse("chess"),
                FoxError::ResponseTooLarge,
            ],
        }],
        vec![FoxError::DownloadFailed { failures: vec![] }],
    ] {
        assert_failure(
            FoxError::DownloadFailed { failures },
            "DownloadFailed",
            "all Fox SGF endpoints failed",
        );
    }
}

#[tokio::test]
async fn interrupted_body_is_a_network_failure_at_the_ipc_boundary() {
    let error = interrupted_body_error().await;
    let (kind, message) = network_failure(&error);
    assert_failure(error, kind, message);
}

#[tokio::test]
async fn nested_network_failures_remain_visible_in_either_order() {
    for transport_first in [false, true] {
        let error = interrupted_body_error().await;
        let (kind, message) = network_failure(&error);
        let transport = FoxError::DownloadFailed {
            failures: vec![FoxError::InvalidResponse("chess"), error],
        };
        let mut failures = vec![transport, FoxError::Timeout, FoxError::Api { code: 5 }];
        if transport_first {
            failures.reverse();
        }
        assert_failure(FoxError::DownloadFailed { failures }, kind, message);
    }
}

fn network_failure(error: &FoxError) -> (&'static str, &'static str) {
    // A deadline can also win while receiving the interrupted body. The IPC
    // must keep either network category, never turn it into a protocol error.
    match error {
        FoxError::Timeout => ("Timeout", "Fox network request timed out"),
        FoxError::Transport(_) => ("Network", "Fox network request failed"),
        other => panic!("expected a network failure, got {other:?}"),
    }
}

async fn interrupted_body_error() -> FoxError {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (stop, mut stopped) = oneshot::channel();
    let server = tokio::spawn(async move {
        loop {
            let (mut socket, _) = tokio::select! {
                _ = &mut stopped => break,
                accepted = listener.accept() => accepted.unwrap(),
            };
            // lookup sends a GET. Consume its complete headers before replying.
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                request.push(socket.read_u8().await.unwrap());
            }
            // The JSON itself is valid, but the HTTP body ends prematurely.
            // This exercises transport classification without OS error text.
            let body = r#"{"result":0,"uid":"123","username":"sample"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len() + 1,
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            socket.shutdown().await.unwrap();
        }
    });
    let client = FoxClient::with_endpoints(
        FoxEndpoints {
            user_url: endpoint.clone(),
            list_url: endpoint.clone(),
            sgf_urls: vec![endpoint],
        },
        Duration::from_secs(1),
    )
    .unwrap();
    let result = tokio::time::timeout(Duration::from_secs(5), client.lookup("sample")).await;
    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
    let error = result.unwrap().unwrap_err();
    assert!(
        error.is_retryable_transport(),
        "interrupted body: {error:?}"
    );
    error
}
