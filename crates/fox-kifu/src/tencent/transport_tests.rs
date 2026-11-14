#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::tests::{GAMES, NEXT, SGF};
use super::*;
use crate::test_http::{Reply, Server, within};
use serde_json::json;

fn client(server: &Server, idle: Duration) -> TencentClient {
    TencentClient::with_endpoints(
        TencentEndpoints {
            list_url: format!("{}/list", server.base),
            sgf_url: format!("{}/sgf", server.base),
        },
        idle,
    )
    .unwrap()
}

#[tokio::test]
async fn query_and_pages_replay_recordings_with_exact_wire_parameters() {
    let server = Server::start(vec![
        Reply::json(GAMES),
        Reply::json(NEXT),
        Reply::json(r#"{"result":0,"chesslist":[]}"#),
    ])
    .await;
    let client = client(&server, Duration::from_secs(2));
    let query = TencentQuery(" 样本棋手910002 ".into());
    let first = within(client.search(&query)).await.unwrap();
    let next = within(client.list(&query, first.next_cursor.as_deref()))
        .await
        .unwrap();
    let end = within(client.list(&query, next.next_cursor.as_deref()))
        .await
        .unwrap();
    assert!(end.next_cursor.is_none());
    let seen = server.finish().await;
    assert_eq!(seen.len(), 3);
    for (request, cursor) in seen.iter().zip(["0", "810004", "810054"]) {
        let path = request
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap();
        let url = reqwest::Url::parse(&format!("http://localhost{path}")).unwrap();
        let params: std::collections::BTreeMap<_, _> = url.query_pairs().collect();
        assert_eq!(params["type"], "7");
        assert_eq!(params["lastCode"], cursor);
        assert_eq!(params["username"], "样本棋手910002");
        assert_eq!(params["srcuid"], "");
        assert_eq!(params["txwqsession"], "lizzieyzy-next");
        assert_eq!(params["fetchnum"], "100");
    }
}

#[tokio::test]
async fn numeric_query_is_sent_as_username_and_srcuid_and_download_reuses_normalization() {
    let server = Server::start(vec![Reply::json(GAMES), Reply::json(SGF), Reply::json(json!({"result":0,"chess":"\u{feff}\\(\\;GM[1]SZ[19]PB[Sample black]PW[Sample white]C[keep\\]value]\\;B[aa])"}).to_string())]).await;
    let client = client(&server, Duration::from_secs(2));
    within(client.search(&TencentQuery("001234".into())))
        .await
        .unwrap();
    let sgf = within(client.download("recorded-detail")).await.unwrap();
    assert!(sgf.contains("PB[样本棋手910002]"));
    assert_eq!(sgf.matches(";B[").count() + sgf.matches(";W[").count(), 79);
    let cleaned = within(client.download("cleanup")).await.unwrap();
    assert!(cleaned.starts_with("(;"));
    assert!(cleaned.contains("C[keep\\]value]"));
    assert!(cleaned.ends_with(";B[aa])"));
    let seen = server.finish().await;
    assert_eq!(seen.len(), 3);
    assert!(seen[0].contains("username=001234&srcuid=001234"));
    assert!(seen[1].starts_with("GET /sgf?chessid=recorded-detail "));
}

#[tokio::test]
async fn shared_transport_retries_interrupted_bodies_and_classifies_timeout() {
    let server = Server::start(vec![Reply::Truncated, Reply::json(GAMES)]).await;
    let page =
        within(client(&server, Duration::from_secs(1)).search(&TencentQuery("sample".into())))
            .await
            .unwrap();
    assert_eq!(page.games.len(), 5);
    assert_eq!(server.finish().await.len(), 2);
    let server = Server::start(vec![Reply::Idle]).await;
    let error =
        within(client(&server, Duration::from_millis(50)).search(&TencentQuery("sample".into())))
            .await
            .unwrap_err();
    assert!(matches!(error, TencentError::Common(FoxError::Timeout)));
    assert_eq!(server.finish().await.len(), 3);
}

#[tokio::test]
async fn api_http_json_and_size_errors_do_not_retry() {
    for (reply, expected) in [
        (
            Reply::json(r#"{"result":7,"resultstr":"sample failure"}"#),
            "api",
        ),
        (
            Reply::json(r#"{"result":0,"ret":8,"resultstr":"sample failure"}"#),
            "api",
        ),
        (Reply::Body(503, Vec::new()), "http"),
        (Reply::json("broken"), "json"),
        (Reply::TooLarge, "size"),
    ] {
        let server = Server::start(vec![reply]).await;
        let error =
            within(client(&server, Duration::from_secs(1)).search(&TencentQuery("sample".into())))
                .await
                .unwrap_err();
        match expected {
            "api" => assert!(
                matches!(error, TencentError::Api { message, .. } if message == "sample failure")
            ),
            "http" => assert!(matches!(
                error,
                TencentError::Common(FoxError::Http { status: 503 })
            )),
            "json" => assert!(matches!(error, TencentError::Common(FoxError::Json(_)))),
            _ => assert!(matches!(
                error,
                TencentError::Common(FoxError::ResponseTooLarge)
            )),
        }
        assert_eq!(server.finish().await.len(), 1);
    }
}

#[tokio::test]
async fn connection_failure_keeps_cross_platform_transport_classification() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let client = TencentClient::with_endpoints(
        TencentEndpoints {
            list_url: base.clone(),
            sgf_url: base,
        },
        Duration::from_millis(100),
    )
    .unwrap();
    let error = within(client.search(&TencentQuery("sample".into())))
        .await
        .unwrap_err();
    assert!(matches!(error, TencentError::Common(error) if error.is_retryable_transport()));
}
