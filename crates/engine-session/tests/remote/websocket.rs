use super::common::*;
use engine_session::{
    EngineStatus, QueryEvent, RemoteTransportError, TransportSpec, probe_remote, spawn,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::{broadcast, mpsc, oneshot},
    task::{JoinHandle, JoinSet},
};
use tokio_tungstenite::{
    accept_async,
    tungstenite::{
        Message,
        protocol::{CloseFrame, frame::coding::CloseCode},
    },
};
use tracing::instrument::WithSubscriber;

struct Server {
    url: String,
    requests: mpsc::UnboundedReceiver<Value>,
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
}
impl Server {
    async fn new(crash_first: bool) -> Self {
        Self::at("127.0.0.1:0", crash_first).await
    }
    async fn at(address: &str, crash_first: bool) -> Self {
        let binary = fake_katago::binary_path().unwrap();
        let listener = TcpListener::bind(address).await.unwrap();
        let url = format!(
            "ws://{}/analysis?token=remote-secret-token",
            listener.local_addr().unwrap()
        );
        let (requests_tx, requests) = mpsc::unbounded_channel();
        let (stop, mut stopping) = oneshot::channel();
        let task = tokio::spawn(async move {
            let (close, _) = broadcast::channel(1);
            let mut sessions = JoinSet::new();
            let mut first = true;
            loop {
                tokio::select! {
                    _ = &mut stopping => break,
                    connection = listener.accept() => {
                        let (stream, _) = connection.unwrap();
                        sessions.spawn(bridge(stream, binary.clone(), crash_first && first, requests_tx.clone(), close.subscribe()).with_subscriber(tracing::subscriber::NoSubscriber::default()));
                        first = false;
                    }
                    _ = sessions.join_next(), if !sessions.is_empty() => {},
                }
            }
            let _ = close.send(());
            while let Some(result) = sessions.join_next().await {
                result.unwrap();
            }
        });
        Self {
            url,
            requests,
            stop,
            task,
        }
    }
    fn spec(&self) -> TransportSpec {
        TransportSpec::WebSocket {
            url: self.url.clone(),
        }
    }
    async fn analysis_request(&mut self) -> Value {
        within(async {
            loop {
                let value = self.requests.recv().await.unwrap();
                if value.get("moves").is_some() {
                    return value;
                }
            }
        })
        .await
    }
    async fn shutdown(self) {
        let _ = self.stop.send(());
        within(self.task).await.unwrap();
    }
}

async fn bridge(
    stream: TcpStream,
    binary: PathBuf,
    crash: bool,
    requests: mpsc::UnboundedSender<Value>,
    mut close: broadcast::Receiver<()>,
) {
    let Ok(mut socket) = accept_async(stream).await else {
        return;
    };
    let mut child = fake(&binary).await;
    let mut input = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap()).lines();
    loop {
        tokio::select! {
            _ = close.recv() => break,
            frame = socket.next() => match frame {
                Some(Ok(Message::Text(text))) => {
                    let value: Value = serde_json::from_str(&text).unwrap();
                    let _ = requests.send(value.clone());
                    if crash && value.get("moves").is_some() {
                        let _ = socket.close(Some(CloseFrame { code: CloseCode::Away, reason: "remote-secret-token".into() })).await;
                        break;
                    }
                    if input.write_all(format!("{text}\n").as_bytes()).await.is_err() { break; }
                }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                _ => {},
            },
            line = stdout.next_line() => match line {
                Ok(Some(line)) => {
                    // Multiple lines prove frame splitting. A secret in a
                    // numeric field also exercises serde's diagnostic boundary.
                    let malformed = r#"{"id":"q","turnNumber":0,"isDuringSearch":false,"moveInfos":[],"rootInfo":{"visits":"remote-secret-token"}}"#;
                    let text = format!("{{\"warning\":\"remote-secret-token\"}}\r\n{malformed}\n{line}\n");
                    if socket.send(Message::Text(text.into())).await.is_err() { break; }
                },
                _ => break,
            }
        }
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
}

#[tokio::test]
async fn websocket_handshake_multiline_live_whole_game_cancel_and_black_override() {
    let mut server = Server::new(false).await;
    assert_eq!(
        within(probe_remote(&server.spec(), config()))
            .await
            .unwrap(),
        "1.16.5-fake"
    );
    let engine = spawn(server.spec(), config());
    ready(&engine).await;
    let mut live = engine.submit(request(json!({"maxVisits":1000000,"reportDuringSearchEvery":0.001,"overrideSettings":{"reportAnalysisWinratesAs":"WHITE","maxTime":1000}}))).await.unwrap();
    assert!(matches!(next(&mut live).await, QueryEvent::Partial(_)));
    let sent = server.analysis_request().await;
    assert_eq!(
        sent["overrideSettings"]["reportAnalysisWinratesAs"],
        "BLACK"
    );
    assert_eq!(sent["overrideSettings"]["maxTime"], 1000);
    engine.terminate(live.id().into()).await.unwrap();
    assert_eq!(next(&mut live).await, QueryEvent::Terminated);
    assert!(within(live.recv()).await.is_none());
    let mut review = engine
        .submit(request(
            json!({"moves":[["B","D4"],["W","E5"]],"analyzeTurns":[0,1,2]}),
        ))
        .await
        .unwrap();
    let finals = complete(&mut review).await;
    assert_eq!(
        finals
            .iter()
            .map(|response| response.turn_number)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert!(
        finals
            .iter()
            .all(|response| response.root_info.visits == 10)
    );
    let review_sent = server.analysis_request().await;
    assert_eq!(
        review_sent["overrideSettings"]["reportAnalysisWinratesAs"],
        "BLACK"
    );
    within(engine.shutdown()).await.unwrap();
    server.shutdown().await;
}

#[tokio::test]
async fn websocket_disconnect_replays_pending_query_and_does_not_expose_close_secret() {
    let mut server = Server::new(true).await;
    let engine = spawn(server.spec(), config());
    ready(&engine).await;
    let mut query = engine.submit(request(json!({}))).await.unwrap();
    let original = server.analysis_request().await;
    assert_eq!(next(&mut query).await, QueryEvent::Restarted);
    let replayed = server.analysis_request().await;
    assert_ne!(original["id"], replayed["id"]);
    assert_eq!(complete(&mut query).await.len(), 1);
    assert!(matches!(
        *engine.status().borrow(),
        EngineStatus::Ready { .. }
    ));
    within(engine.shutdown()).await.unwrap();
    server.shutdown().await;
}

#[tokio::test]
async fn websocket_self_signed_certificate_is_rejected() {
    use tokio_rustls::{
        TlsAcceptor,
        rustls::{self, pki_types::PrivatePkcs8KeyDer},
    };
    let certified = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let key = PrivatePkcs8KeyDer::from(certified.signing_key.serialize_der());
    let tls_config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(vec![certified.cert.der().clone()], key.into())
    .unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(tls_config));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "wss://localhost:{}/token",
        listener.local_addr().unwrap().port()
    );
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        assert!(acceptor.accept(stream).await.is_err());
    });
    assert_eq!(
        within(probe_remote(&TransportSpec::WebSocket { url }, config())).await,
        Err(RemoteTransportError::Connect)
    );
    within(server).await.unwrap();
}

#[tokio::test]
async fn shutdown_cancels_a_remote_connection_waiting_for_handshake() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let engine = spawn(
        TransportSpec::WebSocket {
            url: format!("ws://{}", listener.local_addr().unwrap()),
        },
        config(),
    );
    let (mut socket, _) = within(listener.accept()).await.unwrap();
    let status = engine.status();
    within(engine.shutdown()).await.unwrap();
    use tokio::io::AsyncReadExt;
    let mut bytes = Vec::new();
    within(socket.read_to_end(&mut bytes)).await.unwrap();
    assert_eq!(*status.borrow(), EngineStatus::Stopped);
}

#[tokio::test]
async fn local_transport_query_override_is_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let log = directory.path().join("queries.jsonl");
    let spec = TransportSpec::LocalProcess {
        program: fake_katago::binary_path().unwrap(),
        args: [
            "analysis",
            "-config",
            "unused",
            "-model",
            "unused",
            "--request-log",
        ]
        .into_iter()
        .map(Into::into)
        .chain([log.as_os_str().to_owned()])
        .collect(),
        cwd: None,
        env: Default::default(),
    };
    let engine = spawn(spec, config());
    ready(&engine).await;
    let mut query = engine
        .submit(request(
            json!({"overrideSettings":{"reportAnalysisWinratesAs":"WHITE"}}),
        ))
        .await
        .unwrap();
    assert_eq!(complete(&mut query).await.len(), 1);
    within(engine.shutdown()).await.unwrap();
    let requests: Vec<Value> = std::fs::read_to_string(log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let query = requests
        .iter()
        .find(|request| request.get("moves").is_some())
        .unwrap();
    assert_eq!(
        query["overrideSettings"]["reportAnalysisWinratesAs"],
        "WHITE"
    );
}

pub(super) async fn exercise_for_logs() {
    let server = Server::new(false).await;
    let engine = spawn(server.spec(), config());
    ready(&engine).await;
    within(engine.shutdown()).await.unwrap();
    server.shutdown().await;
}

#[tokio::test]
async fn websocket_temporarily_unreachable_reconnect_uses_remaining_backoff_budget() {
    let mut server = Server::new(false).await;
    let address = url::Url::parse(&server.url).unwrap();
    let address = format!("127.0.0.1:{}", address.port().unwrap());
    let mut cfg = config();
    cfg.restart_backoff = std::time::Duration::from_secs(1);
    cfg.max_restart_backoff = std::time::Duration::from_secs(2);
    let engine = spawn(server.spec(), cfg);
    ready(&engine).await;
    let mut query = engine
        .submit(request(
            json!({"maxVisits":1000000,"reportDuringSearchEvery":0.001}),
        ))
        .await
        .unwrap();
    assert!(matches!(next(&mut query).await, QueryEvent::Partial(_)));
    server.analysis_request().await;
    let mut status = engine.status();
    server.shutdown().await;
    within(async {
        loop {
            match status.borrow_and_update().clone() {
                EngineStatus::Restarting { attempt: 2, .. } => break,
                EngineStatus::Failed { reason, .. } => {
                    panic!("transient connect exhausted budget: {reason}")
                }
                _ => {}
            }
            status.changed().await.unwrap();
        }
    })
    .await;
    let server = Server::at(&address, false).await;
    within(async {
        loop {
            match query.recv().await {
                Some(QueryEvent::Restarted) => break,
                Some(QueryEvent::Partial(_)) => {}
                event => panic!("expected reconnect: {event:?}"),
            }
        }
    })
    .await;
    assert!(matches!(next(&mut query).await, QueryEvent::Partial(_)));
    engine.terminate(query.id().into()).await.unwrap();
    assert_eq!(next(&mut query).await, QueryEvent::Terminated);
    within(engine.shutdown()).await.unwrap();
    server.shutdown().await;
}
