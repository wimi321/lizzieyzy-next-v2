use super::*;
use crate::web_board::connection::{ConnectionChannels, Outbound};
use crate::web_board::security::random_token;
use crate::web_board::{WebAnalysisUpdate, WebBoardAsset, WebFullState, WebServerMessage};
use futures_util::{SinkExt, StreamExt};
use std::collections::BTreeMap;
use std::future::Future;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::{Error, Message};

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("local transport operation timed out")
}

#[derive(Debug)]
struct Assets(BTreeMap<String, WebBoardAsset>);
impl WebBoardAssets for Assets {
    fn get(&self, path: &str) -> Option<WebBoardAsset> {
        self.0.get(path).cloned()
    }
}

struct Server {
    port: u16,
    token: String,
    stop: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
    broker: tokio::task::JoinHandle<()>,
    outbound: watch::Sender<Outbound>,
    messages: mpsc::Receiver<crate::web_board::WebClientMessage>,
}

impl Server {
    async fn start() -> Self {
        let config = Config {
            bind: Ipv4Addr::LOCALHOST,
            start_port: 0,
            ..Config::default()
        };
        let (listener, port) = within(bind(&config)).await.unwrap();
        let token = random_token().unwrap();
        let assets = Assets(BTreeMap::from([
            (
                "web.html".into(),
                WebBoardAsset {
                    bytes: b"<html>board</html>".to_vec(),
                    mime: "text/html".into(),
                },
            ),
            (
                "assets/board.js".into(),
                WebBoardAsset {
                    bytes: b"export const board = true;".to_vec(),
                    mime: "text/javascript".into(),
                },
            ),
            (
                "settings.json".into(),
                WebBoardAsset {
                    bytes: b"secret".to_vec(),
                    mime: "application/json".into(),
                },
            ),
        ]));
        let (events, mut received) = mpsc::channel(32);
        let (stop, stopped) = watch::channel(false);
        let (outbound, _) = watch::channel(Outbound::default());
        let (messages_tx, messages) = mpsc::channel(32);
        let task = tokio::spawn(run(
            listener,
            token.clone(),
            Arc::new(assets),
            events,
            stopped.clone(),
            config.analysis_interval,
        ));
        let updates = outbound.clone();
        let broker = tokio::spawn(async move {
            let mut stopped = stopped;
            let mut next_id = 0;
            loop {
                tokio::select! {
                    biased;
                    _ = connection::stopped(&mut stopped) => break,
                    event = received.recv() => match event {
                        Some(TransportEvent::Connected { reply, .. }) => {
                            next_id += 1;
                            let _ = reply.send(Some(ConnectionChannels { connection_id: next_id, outbound: updates.subscribe() }));
                        },
                        Some(TransportEvent::Message { message, .. }) => {
                            let _ = messages_tx.try_send(message);
                        },
                        Some(TransportEvent::Disconnected { .. }) => {},
                        None => break,
                    }
                }
            }
        });
        Self {
            port,
            token,
            stop,
            task,
            broker,
            outbound,
            messages,
        }
    }

    async fn request(&self, method: &str, path: &str, extra: &str) -> String {
        let mut stream = within(TcpStream::connect((Ipv4Addr::LOCALHOST, self.port)))
            .await
            .unwrap();
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n{extra}\r\n",
            self.port
        );
        within(stream.write_all(request.as_bytes())).await.unwrap();
        let mut bytes = Vec::new();
        if let Err(error) = within(stream.read_to_end(&mut bytes)).await {
            // A header rejected before its remaining bytes are drained can be
            // reset by the OS after the server sends 431 (or before delivery).
            assert_eq!(error.kind(), std::io::ErrorKind::ConnectionReset);
        }
        String::from_utf8(bytes).unwrap()
    }

    async fn websocket(&self, origin: Option<&str>) -> Result<WebSocketStream<TcpStream>, Error> {
        let stream = within(TcpStream::connect((Ipv4Addr::LOCALHOST, self.port)))
            .await
            .unwrap();
        let mut request = format!("ws://127.0.0.1:{}/{}/ws", self.port, self.token)
            .into_client_request()
            .unwrap();
        if let Some(origin) = origin {
            request
                .headers_mut()
                .insert(ORIGIN, origin.parse().unwrap());
        }
        within(tokio_tungstenite::client_async(request, stream))
            .await
            .map(|(socket, _)| socket)
    }

    async fn close(self) {
        self.stop.send_replace(true);
        within(self.task).await.unwrap();
        within(self.broker).await.unwrap();
    }
}

#[tokio::test]
async fn http_only_exposes_token_scoped_canonical_embedded_assets() {
    let server = Server::start().await;
    let root = format!("/{}/", server.token);
    let result = server.request("GET", &root, "").await;
    assert!(result.starts_with("HTTP/1.1 200"), "{result}");
    assert!(result.contains("content-type: text/html"));
    assert!(result.ends_with("<html>board</html>"));
    let result = server
        .request("GET", &format!("{root}assets/board.js"), "")
        .await;
    assert!(result.contains("content-type: text/javascript"));
    assert!(result.ends_with("export const board = true;"));
    for path in [
        "/wrong-token/".to_owned(),
        format!("{root}settings.json"),
        format!("{root}assets/missing.js"),
        format!("{root}assets/../settings.json"),
        format!("{root}assets/%2e%2e/settings.json"),
        format!("{root}assets/..\\settings.json"),
        format!("{root}assets/a%2fb"),
        format!("{root}assets/a%5cb"),
    ] {
        let result = server.request("GET", &path, "").await;
        assert!(result.starts_with("HTTP/1.1 404"), "{path}: {result}");
        assert!(!result.contains("secret"));
    }
    assert!(
        server
            .request("POST", &root, "Content-Length: 0\r\n")
            .await
            .starts_with("HTTP/1.1 405")
    );
    assert!(
        server
            .request("HEAD", &root, "")
            .await
            .starts_with("HTTP/1.1 405")
    );
    let oversized = format!("X-Oversized: {}\r\n", "a".repeat(9000));
    let response = server.request("GET", &root, &oversized).await;
    assert!(
        response.starts_with("HTTP/1.1 431") || response.is_empty(),
        "{response}"
    );
    server.close().await;
}

#[tokio::test]
async fn websocket_requires_exact_same_origin_and_rejects_oversized_commands() {
    let mut server = Server::start().await;
    for origin in [
        None,
        Some("null"),
        Some("http://example.invalid"),
        Some("http://127.0.0.1"),
    ] {
        let error = server.websocket(origin).await.unwrap_err();
        assert!(
            matches!(error, Error::Http(response) if response.status() == StatusCode::FORBIDDEN)
        );
    }
    let origin = format!("http://127.0.0.1:{}", server.port);
    let mut socket = server.websocket(Some(&origin)).await.unwrap();
    within(socket.send(Message::Text("{\"type\":\"EnterTrial\"}".into())))
        .await
        .unwrap();
    assert_eq!(
        within(server.messages.recv()).await,
        Some(crate::web_board::WebClientMessage::EnterTrial)
    );
    within(socket.send(Message::Text("x".repeat(4097).into())))
        .await
        .unwrap();
    let response = within(socket.next()).await;
    assert!(matches!(
        response,
        Some(Ok(Message::Close(_))) | Some(Err(_)) | None
    ));
    assert!(server.messages.try_recv().is_err());
    server.close().await;
}

#[tokio::test]
async fn websocket_flushes_initial_snapshot_and_service_stop_closes_connections() {
    let server = Server::start().await;
    server.outbound.send_modify(|outbound| {
        outbound.push(WebServerMessage::FullState(WebFullState {
            revision: 7,
            node_id: 4,
            board: None,
            analysis: None,
        }))
    });
    let origin = format!("http://127.0.0.1:{}", server.port);
    let mut socket = server.websocket(Some(&origin)).await.unwrap();
    let Message::Text(text) = within(socket.next()).await.unwrap().unwrap() else {
        panic!("expected snapshot")
    };
    assert!(matches!(
        serde_json::from_str::<WebServerMessage>(&text).unwrap(),
        WebServerMessage::FullState(WebFullState { revision: 7, .. })
    ));
    server.close().await;
    assert!(matches!(
        within(socket.next()).await,
        Some(Ok(Message::Close(_))) | Some(Err(_)) | None
    ));
}

#[tokio::test]
async fn websocket_coalesces_bursts_and_full_state_supersedes_throttled_analysis() {
    fn analysis(revision: u64) -> WebServerMessage {
        WebServerMessage::AnalysisUpdate(WebAnalysisUpdate {
            revision,
            node_id: 0,
            board: None,
            analysis: None,
        })
    }

    async fn next_state(socket: &mut WebSocketStream<TcpStream>) -> WebServerMessage {
        let Message::Text(text) = within(socket.next()).await.unwrap().unwrap() else {
            panic!("expected a state frame")
        };
        serde_json::from_str(&text).unwrap()
    }

    let server = Server::start().await;
    let origin = format!("http://127.0.0.1:{}", server.port);
    let mut socket = server.websocket(Some(&origin)).await.unwrap();
    // Start before publishing the first frame: unlike two client receive times,
    // this lower bound remains valid if a loaded CI worker reads buffered data.
    let first_published = tokio::time::Instant::now();
    server.outbound.send_modify(|out| out.push(analysis(1)));
    assert_eq!(next_state(&mut socket).await, analysis(1));

    let full = WebServerMessage::FullState(WebFullState {
        revision: 5,
        node_id: 0,
        board: None,
        analysis: None,
    });
    // One synchronous watch update is a deterministic producer burst. The
    // connection cannot consume its intermediate values while this is held.
    server.outbound.send_modify(|out| {
        for revision in 2..=4 {
            out.push(analysis(revision));
        }
        out.push(full.clone());
    });
    within(socket.send(Message::Ping(b"full-promoted".to_vec().into())))
        .await
        .unwrap();
    // The new board is eligible before the connection handles the barrier,
    // even while the earlier analysis frame still owns the throttle window.
    assert_eq!(next_state(&mut socket).await, full);
    let pong = within(socket.next()).await.unwrap().unwrap();
    assert!(matches!(pong, Message::Pong(bytes) if bytes.as_ref() == b"full-promoted"));

    server.outbound.send_modify(|out| {
        for revision in 6..=9 {
            out.push(analysis(revision));
        }
    });
    assert_eq!(next_state(&mut socket).await, analysis(9));
    assert!(first_published.elapsed() >= Duration::from_millis(100));

    // A ping/pong barrier detects an extra already-buffered state frame. The
    // next analysis interval then also detects a stale timer-delayed frame.
    within(socket.send(Message::Ping(b"latest-drained".to_vec().into())))
        .await
        .unwrap();
    let pong = within(socket.next()).await.unwrap().unwrap();
    assert!(matches!(pong, Message::Pong(bytes) if bytes.as_ref() == b"latest-drained"));
    server.outbound.send_modify(|out| out.push(analysis(10)));
    assert_eq!(next_state(&mut socket).await, analysis(10));
    assert!(first_published.elapsed() >= Duration::from_millis(200));
    server.close().await;
}

#[tokio::test]
async fn occupied_starting_port_advances_and_shutdown_releases_listener() {
    let occupied = within(async {
        loop {
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
            if listener.local_addr().unwrap().port() <= u16::MAX - 9 {
                break listener;
            }
        }
    })
    .await;
    let start_port = occupied.local_addr().unwrap().port();
    let config = Config {
        bind: Ipv4Addr::LOCALHOST,
        start_port,
        ..Config::default()
    };
    let (listener, port) = within(bind(&config)).await.unwrap();
    assert!(port > start_port && u32::from(port) < u32::from(start_port) + 10);
    drop(listener);
    drop(occupied);
    let server = Server::start().await;
    let port = server.port;
    server.close().await;
    let listener = within(TcpListener::bind((Ipv4Addr::LOCALHOST, port)))
        .await
        .unwrap();
    drop(listener);
}
