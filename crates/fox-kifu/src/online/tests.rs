#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use crate::{
    FoxEndpoints,
    test_http::{Reply, Server, within},
};
use std::{net::SocketAddr, time::Duration};

fn client(server: &Server, idle: Duration) -> FoxClient {
    FoxClient::with_endpoints(
        FoxEndpoints {
            user_url: format!("{}/user", server.base),
            list_url: format!("{}/list", server.base),
            sgf_urls: vec![format!("{}/sgf", server.base)],
        },
        idle,
    )
    .unwrap()
}

fn client_for_host(server: &Server, host: &str) -> (FoxClient, String) {
    let address: SocketAddr = server
        .base
        .strip_prefix("http://")
        .unwrap()
        .parse()
        .unwrap();
    let mut client = client(server, Duration::from_secs(1));
    // Resolve even share-page URLs locally so a routing regression cannot use the network.
    client.online_http = reqwest::Client::builder()
        .no_proxy()
        .resolve(&host.to_ascii_lowercase(), address)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    (client, format!("http://{host}:{}", address.port()))
}

#[tokio::test]
async fn invalid_schemes_yike_and_credentials_are_rejected_without_http() {
    let server = Server::start(vec![Reply::json("unexpected")]).await;
    let client = client(&server, Duration::from_secs(1));
    for url in [
        "file:///tmp/game.sgf",
        "ftp://localhost/game",
        "javascript:alert(1)",
    ] {
        assert!(matches!(
            within(client.fetch_url(url)).await,
            Err(OpenUrlError::UnsupportedScheme)
        ));
    }
    for url in [
        "https://yikeweiqi.com/game",
        "https://www.yikeweiqi.com/game?chessid=sample",
        "https://yikeweiqi.com./game",
    ] {
        assert!(matches!(
            within(client.fetch_url(url)).await,
            Err(OpenUrlError::UnsupportedYike)
        ));
    }
    assert!(matches!(
        within(client.fetch_url("https://user:pass@localhost/game")).await,
        Err(OpenUrlError::InvalidUrl)
    ));
    assert!(matches!(
        within(client.fetch_url("not a url")).await,
        Err(OpenUrlError::InvalidUrl)
    ));
    assert!(server.finish().await.is_empty());
}

#[tokio::test]
async fn quoted_generic_url_returns_original_encoded_bytes_once() {
    let bytes = b"(;CA[GB18030]PB[\xb2\xe2\xca\xd4];B[aa])".to_vec();
    let server = Server::start(vec![Reply::Body(200, bytes.clone())]).await;
    let result = within(
        client(&server, Duration::from_secs(1))
            .fetch_url(&format!("  '\"{}/game.sgf\"'  ", server.base)),
    )
    .await
    .unwrap();
    assert_eq!(result, bytes);
    assert_eq!(server.finish().await.len(), 1);
}

#[tokio::test]
async fn share_chessid_uses_fox_download_and_never_fetches_the_share_page() {
    for host in [
        "foxwq.com",
        "share.foxwq.com",
        "h5.foxwq.com",
        "qq.com",
        "huanle.qq.com",
        "ShArE.FoXwQ.CoM",
        "HUANLE.QQ.COM",
    ] {
        let server = Server::start(vec![Reply::json(
            r#"{"result":0,"chess":"(;SZ[19];B[aa])"}"#,
        )])
        .await;
        let (client, base) = client_for_host(&server, host);
        let sgf =
            within(client.fetch_url(&format!("{base}/index.html?other=1&chessid=sample-game")))
                .await
                .unwrap();
        assert!(sgf.starts_with(b"(;"), "{host}");
        let seen = server.finish().await;
        assert_eq!(seen.len(), 1, "{host}");
        assert!(
            seen[0].starts_with("GET /sgf?chessid=sample-game "),
            "{host}"
        );
    }
}

#[tokio::test]
async fn other_hosts_with_chessid_fetch_the_original_url() {
    let server = Server::start(vec![Reply::Body(200, b"(;SZ[9];B[aa])".to_vec())]).await;
    let (client, base) = client_for_host(&server, "kifu.example.org");
    let sgf = within(client.fetch_url(&format!("{base}/game.sgf?chessid=sample-game")))
        .await
        .unwrap();
    assert_eq!(sgf, b"(;SZ[9];B[aa])");
    let seen = server.finish().await;
    assert_eq!(seen.len(), 1);
    assert!(seen[0].starts_with("GET /game.sgf?chessid=sample-game "));
}

#[tokio::test]
async fn lookalike_hosts_with_chessid_are_not_share_links() {
    for host in [
        "foxwq.com.example.org",
        "evilfoxwq.com",
        "qq.com.example.org",
        "evilqq.com",
    ] {
        let server = Server::start(vec![Reply::Body(200, b"(;SZ[9];B[aa])".to_vec())]).await;
        let (client, base) = client_for_host(&server, host);
        let sgf = within(client.fetch_url(&format!("{base}/game.sgf?chessid=sample-game")))
            .await
            .unwrap();
        assert_eq!(sgf, b"(;SZ[9];B[aa])", "{host}");
        let seen = server.finish().await;
        assert_eq!(seen.len(), 1, "{host}");
        assert!(
            seen[0].starts_with("GET /game.sgf?chessid=sample-game "),
            "{host}"
        );
    }
}

#[tokio::test]
async fn redirects_allow_five_hops_and_refuse_the_sixth() {
    for (hops, succeeds) in [(5, true), (6, false)] {
        let mut replies = (0..hops)
            .map(|i| Reply::Redirect(format!("/next-{i}")))
            .collect::<Vec<_>>();
        replies.push(Reply::Body(200, b"(;SZ[19])".to_vec()));
        let server = Server::start(replies).await;
        let result = within(
            client(&server, Duration::from_secs(1)).fetch_url(&format!("{}/start", server.base)),
        )
        .await;
        if succeeds {
            assert_eq!(result.unwrap(), b"(;SZ[19])");
        } else {
            assert!(matches!(result, Err(OpenUrlError::RedirectLimit)));
        }
        assert_eq!(server.finish().await.len(), 6);
    }
}

#[tokio::test]
async fn non_http_redirects_are_rejected_before_following() {
    for location in ["file:///tmp/game.sgf", "ftp://localhost/game"] {
        let server = Server::start(vec![Reply::Redirect(location.into())]).await;
        assert!(matches!(
            within(client(&server, Duration::from_secs(1)).fetch_url(&server.base)).await,
            Err(OpenUrlError::UnsupportedScheme)
        ));
        assert_eq!(server.finish().await.len(), 1);
    }
}

#[tokio::test]
async fn generic_download_size_http_and_idle_failures_are_not_retried() {
    for (reply, expected) in [
        (Reply::TooLarge, "size"),
        (Reply::Body(503, Vec::new()), "http"),
        (Reply::Truncated, "network"),
        (Reply::Idle, "timeout"),
    ] {
        let server = Server::start(vec![reply]).await;
        let result =
            within(client(&server, Duration::from_millis(100)).fetch_url(&server.base)).await;
        match expected {
            "size" => assert!(matches!(
                result,
                Err(OpenUrlError::Request(FoxError::ResponseTooLarge))
            )),
            "http" => assert!(matches!(
                result,
                Err(OpenUrlError::Request(FoxError::Http { status: 503 }))
            )),
            "timeout" => assert!(matches!(
                result,
                Err(OpenUrlError::Request(FoxError::Timeout))
            )),
            _ => assert!(
                matches!(result,Err(OpenUrlError::Request(error)) if error.is_retryable_transport())
            ),
        }
        assert_eq!(server.finish().await.len(), 1);
    }
}
