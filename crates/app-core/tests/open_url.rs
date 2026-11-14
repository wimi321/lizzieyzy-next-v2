#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;
#[path = "common/online.rs"]
mod online;

use app_core::{
    AppConfig, AppCore, AppError, AppSubscription, fox::FoxError, tencent::OpenUrlError,
};
use common::within;
use online::{Reply, Server, document_event};
use serde_json::json;
use std::path::Path;
use tokio::{net::TcpListener, sync::oneshot};

async fn spawn(directory: &Path, server: &Server) -> (AppCore, AppSubscription) {
    within(app_core::spawn_with_config(
        directory,
        AppConfig {
            fox_client: Some(server.fox_client()),
            ..AppConfig::default()
        },
    ))
    .await
    .unwrap()
}

#[tokio::test]
async fn unsupported_schemes_and_yike_links_fail_without_replacing_the_game() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = within(app_core::spawn(directory.path())).await.unwrap();
    let revision = within(app.open_bytes(b"(;SZ[9]C[keep];B[aa])".to_vec()))
        .await
        .unwrap();
    document_event(&mut events, revision).await;
    for url in [
        "file:///tmp/sample.sgf",
        "ftp://example.invalid/game.sgf",
        "javascript:alert(1)",
        "data:text/plain,(;SZ[9])",
    ] {
        assert!(matches!(
            within(app.open_url(url.into())).await,
            Err(AppError::OpenUrl(OpenUrlError::UnsupportedScheme))
        ));
        assert_eq!(within(app.step(0)).await.unwrap(), revision);
    }
    for url in [
        "https://yikeweiqi.com/game/1",
        "http://www.yikeweiqi.com/game/1?chessid=sample",
    ] {
        assert!(matches!(
            within(app.open_url(url.into())).await,
            Err(AppError::OpenUrl(OpenUrlError::UnsupportedYike))
        ));
        assert_eq!(within(app.step(0)).await.unwrap(), revision);
    }
    for url in ["", "  \"  \"  ", "not a URL"] {
        assert!(matches!(
            within(app.open_url(url.into())).await,
            Err(AppError::OpenUrl(OpenUrlError::InvalidUrl))
        ));
    }
    let revision = within(app.step(1)).await.unwrap();
    assert_eq!(
        document_event(&mut events, revision)
            .await
            .board
            .unwrap()
            .move_number,
        1
    );
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn quoted_url_opens_plain_sgf_and_clears_local_file_identity() {
    let server = Server::start(vec![Reply::body("(;FF[4]SZ[9]C[downloaded];B[aa];W[bb])")]).await;
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("local.sgf");
    std::fs::write(&source, b"(;SZ[19])").unwrap();
    let (app, mut events) = spawn(directory.path(), &server).await;
    within(app.open_path(source)).await.unwrap();
    let revision = within(app.open_url(format!(" \t\"{}/game.sgf\"\n", server.base)))
        .await
        .unwrap();
    let opened = document_event(&mut events, revision).await;
    assert_eq!(opened.file_name, None);
    assert_eq!(opened.file_path, None);
    assert_eq!(opened.comment.as_deref(), Some("downloaded"));
    assert_eq!(opened.tree.main_line.len(), 3);
    assert_eq!(opened.board.unwrap().width, 9);
    let requests = server.finish().await;
    assert_eq!(requests.len(), 1);
    assert!(requests[0].starts_with("GET /game.sgf "));
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn ordinary_url_keeps_original_bytes_for_sgf_encoding_detection() {
    let server = Server::start(vec![Reply::body(
        b"(;CA[GBK]SZ[9]C[\xd6\xd0\xce\xc4];B[aa])".to_vec(),
    )])
    .await;
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = spawn(directory.path(), &server).await;
    let revision = within(app.open_url(format!("'{}/encoded.sgf'", server.base)))
        .await
        .unwrap();
    let opened = document_event(&mut events, revision).await;
    assert_eq!(opened.comment.as_deref(), Some("中文"));
    assert_eq!(opened.tree.main_line.len(), 2);
    assert_eq!(server.finish().await.len(), 1);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn fox_share_query_uses_the_download_client_instead_of_fetching_the_page() {
    let server = Server::start(vec![Reply::body(
        json!({"result":0,"chess":"\u{feff}\\(\\;SZ[9]C[shared]\\;B[aa])"}).to_string(),
    )])
    .await;
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = spawn(directory.path(), &server).await;
    // The share URL is never requested; the injected Fox endpoint is local.
    let revision = within(
        app.open_url("https://share.foxwq.com/index.html?from=sample&chessid=game-share".into()),
    )
    .await
    .unwrap();
    let opened = document_event(&mut events, revision).await;
    assert_eq!(opened.comment.as_deref(), Some("shared"));
    assert_eq!(opened.tree.main_line.len(), 2);
    let requests = server.finish().await;
    assert_eq!(requests.len(), 1);
    assert!(requests[0].starts_with("GET /fox-sgf?chessid=game-share "));
    assert!(within(app.settings()).await.unwrap().fox_history.is_empty());
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn invalid_sgf_http_failure_and_oversize_response_preserve_current_document() {
    let mut unavailable = Reply::body("unavailable");
    unavailable.status = 503;
    let server = Server::start(vec![
        Reply::body("<html>not a game</html>"),
        Reply::body("(;C[unterminated"),
        unavailable,
        Reply::too_large(),
    ])
    .await;
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = spawn(directory.path(), &server).await;
    let revision = within(app.open_bytes(b"(;SZ[9]C[keep];B[aa])".to_vec()))
        .await
        .unwrap();
    document_event(&mut events, revision).await;
    let before = directory.path().join("before.sgf");
    within(app.save(&before)).await.unwrap();
    for path in ["html", "broken"] {
        assert!(matches!(
            within(app.open_url(format!("{}/{path}", server.base))).await,
            Err(AppError::OpenUrl(OpenUrlError::InvalidSgf))
        ));
        assert_eq!(within(app.step(0)).await.unwrap(), revision);
    }
    assert!(matches!(
        within(app.open_url(format!("{}/unavailable", server.base))).await,
        Err(AppError::OpenUrl(OpenUrlError::Request(FoxError::Http {
            status: 503
        })))
    ));
    assert!(matches!(
        within(app.open_url(format!("{}/large", server.base))).await,
        Err(AppError::OpenUrl(OpenUrlError::Request(
            FoxError::ResponseTooLarge
        )))
    ));
    assert_eq!(within(app.step(0)).await.unwrap(), revision);
    let after = directory.path().join("after.sgf");
    within(app.save(&after)).await.unwrap();
    assert_eq!(
        std::fs::read(before).unwrap(),
        std::fs::read(after).unwrap()
    );
    assert_eq!(server.finish().await.len(), 4);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn redirects_to_non_http_schemes_are_rejected_without_opening() {
    let server = Server::start(vec![
        Reply::redirect("file:///tmp/unrequested.sgf"),
        Reply::redirect("ftp://example.invalid/game.sgf"),
    ])
    .await;
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = spawn(directory.path(), &server).await;
    let revision = within(app.open_bytes(b"(;SZ[9]C[keep])".to_vec()))
        .await
        .unwrap();
    document_event(&mut events, revision).await;
    for path in ["file", "ftp"] {
        assert!(matches!(
            within(app.open_url(format!("{}/{path}", server.base))).await,
            Err(AppError::OpenUrl(OpenUrlError::UnsupportedScheme))
        ));
        assert_eq!(within(app.step(0)).await.unwrap(), revision);
    }
    assert_eq!(server.finish().await.len(), 2);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn exactly_five_redirects_open_but_a_sixth_redirect_fails() {
    let mut replies: Vec<_> = (1..=5)
        .map(|index| Reply::redirect(format!("/step-{index}")))
        .collect();
    replies.push(Reply::body("(;SZ[9]C[after five])"));
    replies.extend((1..=6).map(|index| Reply::redirect(format!("/too-many-{index}"))));
    let server = Server::start(replies).await;
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = spawn(directory.path(), &server).await;
    let revision = within(app.open_url(format!("{}/start", server.base)))
        .await
        .unwrap();
    assert_eq!(
        document_event(&mut events, revision)
            .await
            .comment
            .as_deref(),
        Some("after five")
    );
    assert!(matches!(
        within(app.open_url(format!("{}/too-many", server.base))).await,
        Err(AppError::OpenUrl(OpenUrlError::RedirectLimit))
    ));
    assert_eq!(within(app.step(0)).await.unwrap(), revision);
    let requests = server.finish().await;
    assert_eq!(requests.len(), 12);
    assert!(requests[5].starts_with("GET /step-5 "));
    assert!(requests[11].starts_with("GET /too-many-5 "));
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn pending_url_download_does_not_block_document_commands() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/pending.sgf", listener.local_addr().unwrap());
    let (arrived, arrival) = oneshot::channel();
    let (release, released) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        online::read_request(&mut socket).await;
        arrived.send(()).unwrap();
        released.await.unwrap();
        online::respond(&mut socket, &Reply::body("(;SZ[9]C[downloaded])")).await;
    });
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = within(app_core::spawn(directory.path())).await.unwrap();
    let opening = app.clone();
    let open = tokio::spawn(async move { opening.open_url(url).await });
    within(arrival).await.unwrap();
    let revision = within(app.open_bytes(b"(;SZ[9]C[responsive])".to_vec()))
        .await
        .unwrap();
    assert_eq!(
        document_event(&mut events, revision)
            .await
            .comment
            .as_deref(),
        Some("responsive")
    );
    release.send(()).unwrap();
    let revision = within(open).await.unwrap().unwrap();
    assert_eq!(
        document_event(&mut events, revision)
            .await
            .comment
            .as_deref(),
        Some("downloaded")
    );
    within(server).await.unwrap();
    within(app.shutdown()).await.unwrap();
}
