#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use serde_json::json;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
    task::JoinHandle,
    time::Instant,
};

#[derive(Clone)]
enum Reply {
    Json(u16, String),
    Slow(String),
    Idle,
    Reset,
    Truncated,
    TooLarge,
}

struct SeenRequest {
    text: String,
    at: Instant,
}

struct Server {
    endpoints: FoxEndpoints,
    stop: oneshot::Sender<()>,
    task: JoinHandle<Vec<SeenRequest>>,
}

impl Server {
    async fn start(replies: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let endpoints = FoxEndpoints {
            user_url: format!("{base}/user"),
            list_url: format!("{base}/list"),
            sgf_urls: vec![format!("{base}/cgi"), format!("{base}/h5")],
        };
        let (stop, mut stopped) = oneshot::channel();
        let task = tokio::spawn(async move {
            let mut seen = Vec::new();
            loop {
                let (mut socket, _) = tokio::select! {
                    _ = &mut stopped => break,
                    accepted = listener.accept() => accepted.unwrap(),
                };
                let text = read_request(&mut socket).await;
                let index = seen.len().min(replies.len() - 1);
                seen.push(SeenRequest {
                    text,
                    at: Instant::now(),
                });
                tokio::select! {
                    _ = &mut stopped => break,
                    () = respond(&mut socket, &replies[index]) => {},
                }
            }
            seen
        });
        Self {
            endpoints,
            stop,
            task,
        }
    }

    fn client(&self, read_timeout: Duration) -> FoxClient {
        FoxClient::with_endpoints(self.endpoints.clone(), read_timeout).unwrap()
    }

    async fn finish(self) -> Vec<SeenRequest> {
        let _ = self.stop.send(());
        within(self.task).await.unwrap()
    }
}

async fn within<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("test deadline")
}

async fn read_request(socket: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    loop {
        let mut buffer = [0; 4096];
        let count = socket.read(&mut buffer).await.unwrap();
        assert_ne!(count, 0);
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..end]);
            let length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .map(|value| value.parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if bytes.len() >= end + 4 + length {
                return String::from_utf8(bytes).unwrap();
            }
        }
    }
}

async fn headers(socket: &mut TcpStream, status: u16, length: usize) {
    let text = format!(
        "HTTP/1.1 {status} Replay\r\nContent-Type: application/json\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n"
    );
    socket.write_all(text.as_bytes()).await.unwrap();
}

async fn respond(socket: &mut TcpStream, reply: &Reply) {
    match reply {
        Reply::Json(status, body) => {
            headers(socket, *status, body.len()).await;
            socket.write_all(body.as_bytes()).await.unwrap();
        }
        Reply::Slow(body) => {
            headers(socket, 200, body.len()).await;
            // Intentional network shaping, not a wait used to coordinate tasks:
            // twenty data chunks every 60 ms exceed a 250 ms idle limit in total.
            let mut pacing = tokio::time::interval(Duration::from_millis(60));
            pacing.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            for bytes in body.as_bytes().chunks(body.len().div_ceil(20)) {
                pacing.tick().await;
                if socket.write_all(bytes).await.is_err() {
                    return;
                }
            }
        }
        Reply::Idle => {
            headers(socket, 200, 100).await;
            socket.write_all(b"{\"result\":").await.unwrap();
            // The client closes the connection when its idle deadline expires.
            let mut remaining = Vec::new();
            let _ = socket.read_to_end(&mut remaining).await;
        }
        Reply::Reset => {
            socket2::SockRef::from(&*socket)
                .set_linger(Some(Duration::ZERO))
                .unwrap();
            // Dropping the accepted socket after this branch emits a TCP RST.
        }
        Reply::Truncated => {
            headers(socket, 200, 100).await;
            socket.write_all(b"{\"result\":0}").await.unwrap();
        }
        Reply::TooLarge => headers(socket, 200, MAX_RESPONSE_BYTES + 1).await,
    }
}

fn user_response() -> Reply {
    Reply::Json(
        200,
        json!({"result":0,"uid":"123","username":"sample"}).to_string(),
    )
}

fn page_response() -> String {
    let games = (0..100)
        .map(|index| {
            json!({
                "chessid": format!("game-{index}"), "starttime":"2026-10-30",
                "blackuid":123, "whiteuid":456, "blacknick":"sample", "whitenick":"opponent",
                "blackdan":24, "whitedan":24, "winner":1, "point":-1,"rule":1,"movenum":177,
            })
        })
        .collect::<Vec<_>>();
    json!({"result":0,"chesslist":games}).to_string()
}

#[tokio::test]
async fn slow_hundred_game_page_can_exceed_idle_timeout_without_retrying() {
    let server = Server::start(vec![Reply::Slow(page_response())]).await;
    let idle = Duration::from_millis(250);
    let start = Instant::now();
    let page = within(server.client(idle).list("123", None)).await.unwrap();
    assert!(start.elapsed() > idle);
    assert_eq!(page.games.len(), 100);
    assert_eq!(page.next_cursor.as_deref(), Some("game-99"));
    assert_eq!(server.finish().await.len(), 1);
}

#[tokio::test]
async fn idle_timeouts_retry_then_recover_on_the_third_attempt() {
    let server = Server::start(vec![Reply::Idle, Reply::Idle, user_response()]).await;
    let account = within(server.client(Duration::from_millis(75)).lookup("sample"))
        .await
        .unwrap();
    assert_eq!(account.uid, "123");
    let attempts = server.finish().await;
    assert_eq!(attempts.len(), 3);
    assert!(attempts[1].at - attempts[0].at >= Duration::from_millis(350));
    assert!(attempts[2].at - attempts[1].at >= Duration::from_millis(700));
}

#[tokio::test]
async fn transport_attempts_stop_at_three_and_preserve_the_timeout_error() {
    let server = Server::start(vec![Reply::Idle]).await;
    let error = within(server.client(Duration::from_millis(50)).lookup("sample"))
        .await
        .unwrap_err();
    assert!(matches!(error, FoxError::Timeout));
    assert_eq!(server.finish().await.len(), 3);
}

#[tokio::test]
async fn connection_refusal_is_retried_with_backoff() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let client = FoxClient::with_endpoints(
        FoxEndpoints {
            user_url: format!("http://{address}/refused"),
            ..FoxEndpoints::default()
        },
        Duration::from_millis(100),
    )
    .unwrap();
    let start = Instant::now();
    let error = within(client.lookup("sample")).await.unwrap_err();
    // Windows can keep the TCP connect pending past this short read deadline.
    // reqwest checks that deadline before polling the connection/response, so
    // Timeout and a socket refusal have the same retryable network semantics.
    assert!(error.is_retryable_transport(), "closed port: {error:?}");
    assert!(
        start.elapsed() >= Duration::from_millis(1050),
        "network retries must include both backoffs: {error:?}"
    );
}

#[tokio::test]
async fn tcp_reset_retries_preserve_post_body_and_headers() {
    let server = Server::start(vec![Reply::Reset, Reply::Reset, user_response()]).await;
    let client = server.client(Duration::from_secs(1));
    let response = within(
        client.json(
            client
                .http
                .post(&server.endpoints.user_url)
                .header("User-Agent", "okhttp/3.12.12")
                .form(&[("chessid", "game-1")]),
        ),
    )
    .await
    .unwrap();
    assert_eq!(response["uid"], "123");
    let attempts = server.finish().await;
    assert_eq!(attempts.len(), 3);
    for attempt in attempts {
        assert!(attempt.text.starts_with("POST /user "));
        assert!(attempt.text.contains("okhttp/3.12.12"));
        assert!(attempt.text.ends_with("chessid=game-1"));
    }
}

#[tokio::test]
async fn interrupted_response_body_is_retried_instead_of_parsing_partial_json() {
    let server = Server::start(vec![Reply::Truncated, Reply::Truncated, user_response()]).await;
    let account = within(server.client(Duration::from_secs(1)).lookup("sample"))
        .await
        .unwrap();
    assert_eq!(account.uid, "123");
    assert_eq!(server.finish().await.len(), 3);
}

#[tokio::test]
async fn http_status_json_api_and_size_errors_are_not_retried() {
    for reply in [
        Reply::Json(503, "busy".into()),
        Reply::Json(200, "{".into()),
        Reply::Json(200, json!({"errcode":5}).to_string()),
        Reply::Json(200, json!({"result":0}).to_string()),
        Reply::TooLarge,
    ] {
        let server = Server::start(vec![reply.clone()]).await;
        let error = within(server.client(Duration::from_secs(1)).lookup("sample"))
            .await
            .unwrap_err();
        assert!(
            !error.is_retryable_transport(),
            "non-network failure: {error:?}"
        );
        match reply {
            Reply::Json(503, _) => assert!(matches!(error, FoxError::Http { status: 503 })),
            Reply::Json(_, ref body) if body == "{" => assert!(matches!(error, FoxError::Json(_))),
            Reply::Json(_, ref body) if body.contains("errcode") => {
                assert!(matches!(error, FoxError::Api { code: 5 }))
            }
            Reply::TooLarge => assert!(matches!(error, FoxError::ResponseTooLarge)),
            _ => assert!(matches!(error, FoxError::InvalidResponse(_))),
        }
        assert_eq!(server.finish().await.len(), 1);
    }
}

#[tokio::test]
async fn download_allows_transport_cleanup_before_sgf_validation() {
    let raw = "\u{feff}\\(\\;GM[1]FF[4]SZ[19]KM[375]AP[foxwq]PB[Black]PW[White]\\;B[aa])";
    let server = Server::start(vec![Reply::Json(
        200,
        json!({"result":0,"chess":raw}).to_string(),
    )])
    .await;
    let sgf = within(server.client(Duration::from_secs(1)).download("game-1"))
        .await
        .unwrap();
    assert!(sgf.starts_with("(;"));
    assert!(sgf.contains("KM[375]"));
    assert!(sgf.contains("PB[Black]PW[White]"));
    assert!(sgf.ends_with(";B[aa])"));
    assert_eq!(server.finish().await.len(), 1);
}
