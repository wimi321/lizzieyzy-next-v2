use super::*;
use app_core::tencent::{TencentClient, TencentEndpoints};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

#[tokio::test]
async fn online_commands_preserve_typed_validation_errors() {
    let harness = Harness::new().await;
    for (command, arguments, expected) in [
        (
            "tencent_search",
            json!({"query": "  "}),
            json!({"type": "Tencent", "data": {"type": "InvalidQuery"}}),
        ),
        (
            "tencent_next_page",
            json!({"query": "", "cursor": "fixture-cursor"}),
            json!({"type": "Tencent", "data": {"type": "InvalidQuery"}}),
        ),
        (
            "tencent_open_game",
            json!({"chessid": "../../invalid"}),
            json!({"type": "Tencent", "data": {"type": "Request", "data": {"type": "InvalidGameId"}}}),
        ),
        (
            "open_url",
            json!({"url": "file:///fixture.sgf"}),
            json!({"type": "OpenUrl", "data": {"type": "UnsupportedScheme"}}),
        ),
        (
            "open_url",
            json!({"url": "https://yikeweiqi.com/fixture"}),
            json!({"type": "OpenUrl", "data": {"type": "UnsupportedYike"}}),
        ),
    ] {
        assert_eq!(
            request(&harness.main, command, arguments, local_url())
                .await
                .unwrap_err(),
            expected
        );
    }
    harness.finish().await;
}

#[tokio::test]
async fn online_commands_are_denied_to_other_windows_and_remote_origins() {
    let harness = Harness::new().await;
    let other = tauri::WebviewWindowBuilder::new(&harness.app, "other", Default::default())
        .build()
        .unwrap();
    for command in [
        "tencent_search",
        "tencent_next_page",
        "tencent_open_game",
        "tencent_clear_history",
        "open_url",
    ] {
        for (window, url) in [
            (&other, local_url()),
            (&harness.main, "https://example.invalid/"),
        ] {
            let error = request(window, command, json!({}), url).await.unwrap_err();
            assert!(
                error
                    .as_str()
                    .is_some_and(|message| message.contains("not allowed")),
                "{command}: {error}"
            );
        }
    }
    harness.finish().await;
}

#[tokio::test]
async fn online_ipc_queries_open_and_clear_persisted_history() {
    let (endpoint, mut requests, server) = serve(vec![
        r#"{"result":0,"chesslist":[]}"#,
        r#"{"result":0,"chesslist":[]}"#,
        r#"{"result":0,"chess":"(;GM[1]FF[4]SZ[9];B[aa])"}"#,
        "not a game record",
        "(;GM[1]FF[4]SZ[13];B[bb])",
    ])
    .await;
    let client = TencentClient::with_endpoints(
        TencentEndpoints {
            list_url: format!("{endpoint}/list"),
            sgf_url: format!("{endpoint}/sgf"),
        },
        Duration::from_secs(3),
    )
    .unwrap();
    let mut harness = Harness::with_config(AppConfig {
        tencent_client: Some(client),
        ..AppConfig::default()
    })
    .await;
    harness.subscribe(71).await;
    harness.document(71, 0).await;
    let page = harness
        .invoke("tencent_search", json!({"query": " fixture-query "}))
        .await;
    assert_eq!(page, json!({"games": [], "next_cursor": null}));
    let sent = within(requests.recv()).await.unwrap();
    assert!(sent.starts_with("GET /list?"));
    assert!(sent.contains("username=fixture-query"));
    assert!(sent.contains("lastCode=0"));
    assert_eq!(
        harness.invoke("settings", json!({})).await["tencent_history"],
        json!(["fixture-query"])
    );
    let page = harness
        .invoke(
            "tencent_next_page",
            json!({"query": "fixture-query", "cursor": "fixture-cursor"}),
        )
        .await;
    assert_eq!(page["games"], json!([]));
    let sent = within(requests.recv()).await.unwrap();
    assert!(sent.contains("username=fixture-query"));
    assert!(sent.contains("lastCode=fixture-cursor"));
    let first = harness
        .invoke("tencent_open_game", json!({"chessid": "fixture-game"}))
        .await
        .as_u64()
        .unwrap();
    assert!(first > 0);
    assert!(
        within(requests.recv())
            .await
            .unwrap()
            .contains("chessid=fixture-game")
    );
    let before = harness.document(71, first).await;
    assert_eq!(before.board.as_ref().unwrap().width, 9);
    let failed = request(
        &harness.main,
        "open_url",
        json!({"url": format!("{endpoint}/invalid")}),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(
        failed,
        json!({"type": "OpenUrl", "data": {"type": "InvalidSgf"}})
    );
    within(requests.recv()).await.unwrap();
    harness.subscribe(72).await;
    assert_eq!(harness.document(72, first).await, before);
    let second = harness
        .invoke(
            "open_url",
            json!({"url": format!(" \"{endpoint}/valid\" ")}),
        )
        .await
        .as_u64()
        .unwrap();
    assert!(second > first);
    assert!(
        within(requests.recv())
            .await
            .unwrap()
            .starts_with("GET /valid ")
    );
    assert_eq!(harness.document(72, second).await.board.unwrap().width, 13);
    harness.invoke("tencent_clear_history", json!({})).await;
    assert_eq!(
        harness.invoke("settings", json!({})).await["tencent_history"],
        json!([])
    );
    let saved: Settings = serde_json::from_slice(
        &std::fs::read(harness.directory.path().join("settings.json")).unwrap(),
    )
    .unwrap();
    assert!(saved.tencent_history.is_empty());
    within(server).await.unwrap();
    harness.finish().await;
}

async fn serve(
    responses: Vec<&'static str>,
) -> (
    String,
    mpsc::UnboundedReceiver<String>,
    tokio::task::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (send, requests) = mpsc::unbounded_channel();
    let server = tokio::spawn(async move {
        for body in responses {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            while !bytes.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                socket.read_exact(&mut byte).await.unwrap();
                bytes.push(byte[0]);
                assert!(bytes.len() < 8192);
            }
            send.send(String::from_utf8(bytes).unwrap()).unwrap();
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        }
    });
    (endpoint, requests, server)
}
