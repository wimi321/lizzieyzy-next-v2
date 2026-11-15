#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::{AnalysisRequest, EngineStatus, QueryEvent, TransportSpec, ZhiziConnector};
use serde_json::Value;
use std::{
    future::Future,
    panic::Location,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, OnceLock},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
    sync::broadcast,
    task::{JoinHandle, JoinSet},
};
use tokio_rustls::{
    TlsAcceptor,
    rustls::{self, pki_types::PrivatePkcs8KeyDer},
};
use tokio_tungstenite::{
    Connector, accept_hdr_async,
    tungstenite::handshake::server::{Request, Response},
};

#[track_caller]
fn within<T>(future: impl Future<Output = T>) -> impl Future<Output = T> {
    let caller = Location::caller();
    async move {
        timeout(Duration::from_secs(5), future)
            .await
            .unwrap_or_else(|_| panic!("5s deadline at {}:{}", caller.file(), caller.line()))
    }
}

fn config() -> SessionConfig {
    SessionConfig {
        startup_timeout: Duration::from_secs(120),
        response_timeout: Duration::from_secs(120),
        restart_backoff: Duration::from_millis(1),
        max_restart_backoff: Duration::from_millis(1),
        ..SessionConfig::default()
    }
}

fn fake_binary() -> &'static Path {
    // Cargo republishes its executable even on a fresh build. All tests in this
    // process must finish this single fixture build before any engine is started.
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            let result = std::process::Command::new(
                std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()),
            )
            .args(["build", "--offline", "--locked", "--manifest-path"])
            .arg(root.join("tools/fake-gtp/Cargo.toml"))
            .arg("--target-dir")
            .arg(root.join("target"))
            .arg("--message-format=json")
            .output()
            .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            String::from_utf8(result.stdout)
                .unwrap()
                .lines()
                .find_map(|line| {
                    let value: Value = serde_json::from_str(line).ok()?;
                    (value["reason"] == "compiler-artifact"
                        && value["target"]["name"] == "fake-gtp")
                        .then(|| value["executable"].as_str().map(PathBuf::from))
                        .flatten()
                })
                .expect("fake executable artifact")
        })
        .as_path()
}

fn run(test: impl Future<Output = ()>) {
    // Build before creating a runtime: never block a timer or a live socket on cargo.
    fake_binary();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(test);
}

#[derive(Debug)]
enum Seen {
    Connected(String),
    Pong,
    Command(String),
}

struct Server {
    connection: ZhiziConnection,
    seen: mpsc::UnboundedReceiver<Seen>,
    control: broadcast::Sender<String>,
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

impl Server {
    async fn new(crash_first: bool, ready: Option<oneshot::Receiver<()>>) -> Self {
        Self::with_heartbeat(crash_first, ready, Duration::from_secs(120)).await
    }

    async fn with_heartbeat(
        crash_first: bool,
        ready: Option<oneshot::Receiver<()>>,
        heartbeat: Duration,
    ) -> Self {
        let binary = fake_binary().to_owned();
        let certified = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let server = rustls::ServerConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![certified.cert.der().clone()],
                PrivatePkcs8KeyDer::from(certified.signing_key.serialize_der()).into(),
            )
            .unwrap();
        let mut roots = rustls::RootCertStore::empty();
        roots.add(certified.cert.der().clone()).unwrap();
        let client = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let acceptor = TlsAcceptor::from(Arc::new(server));
        let listener = within(TcpListener::bind("127.0.0.1:0")).await.unwrap();
        let mut connection = ZhiziConnection::new(
            format!(
                "https://localhost:{}",
                listener.local_addr().unwrap().port()
            ),
            "secret-token-0".into(),
        );
        connection.tls = Some(Connector::Rustls(Arc::new(client)));
        let (seen_tx, seen) = mpsc::unbounded_channel();
        let (stop, mut stopping) = oneshot::channel();
        let (control, _) = broadcast::channel(8);
        let sessions_control = control.clone();
        let task = tokio::spawn(async move {
            let mut sessions = JoinSet::new();
            let mut first = true;
            let mut ready = ready;
            while let Some(accepted) = within(async {
                tokio::select! {
                    _ = &mut stopping => None,
                    accepted = listener.accept() => Some(accepted),
                }
            })
            .await
            {
                let (tcp, _) = accepted.unwrap();
                let acceptor = acceptor.clone();
                let seen = seen_tx.clone();
                let binary = binary.clone();
                let control = sessions_control.subscribe();
                let gate = ready.take();
                let crash = crash_first && first;
                first = false;
                sessions.spawn(async move {
                    let Ok(stream) = within(acceptor.accept(tcp)).await else {
                        return;
                    };
                    serve(stream, binary, seen, control, gate, crash, heartbeat).await;
                });
            }
            let _ = sessions_control.send("close".into());
            while let Some(result) = within(sessions.join_next()).await {
                result.unwrap();
            }
        });
        Self {
            connection,
            seen,
            control,
            stop,
            task,
        }
    }

    fn spec(&self) -> TransportSpec {
        let connection = self.connection.clone();
        TransportSpec::Zhizi {
            connect: ZhiziConnector::new(move || {
                let connection = connection.clone();
                async { Ok(connection) }
            }),
        }
    }

    async fn connected(&mut self) -> String {
        within(async {
            loop {
                if let Seen::Connected(token) = self.seen.recv().await.unwrap() {
                    return token;
                }
            }
        })
        .await
    }

    async fn command(&mut self, wanted: &str) -> String {
        within(async {
            loop {
                if let Seen::Command(line) = self.seen.recv().await.unwrap()
                    && line.split_ascii_whitespace().any(|word| word == wanted)
                {
                    return line;
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

// Tungstenite's callback trait fixes its error type to an unboxed HTTP response.
#[allow(clippy::result_large_err)]
async fn serve(
    stream: tokio_rustls::server::TlsStream<TcpStream>,
    binary: PathBuf,
    seen: mpsc::UnboundedSender<Seen>,
    mut control: broadcast::Receiver<String>,
    mut gate: Option<oneshot::Receiver<()>>,
    crash: bool,
    heartbeat: Duration,
) {
    let mut token = String::new();
    let mut socket = within(accept_hdr_async(
        stream,
        |request: &Request, response: Response| {
            let url = url::Url::parse(&format!("https://localhost{}", request.uri())).unwrap();
            assert_eq!(url.path(), "/socket.io.v4/");
            let query: std::collections::BTreeMap<_, _> = url.query_pairs().into_owned().collect();
            assert_eq!(query["EIO"], "4");
            assert_eq!(query["transport"], "websocket");
            token = query["zz-socketio-token"].clone();
            Ok(response)
        },
    ))
    .await
    .unwrap();
    within(socket.send(Message::Text(format!("0{}", json!({"sid":"test-session", "upgrades":[], "pingInterval":heartbeat.as_millis() / 2, "pingTimeout":heartbeat.as_millis() / 2})).into()))).await.unwrap();
    let Some(Ok(Message::Text(connect))) = within(socket.next()).await else {
        return;
    };
    assert_eq!(connect, "40");
    within(socket.send(Message::Text("40{\"sid\":\"test-namespace\"}".into())))
        .await
        .unwrap();
    seen.send(Seen::Connected(token)).unwrap();
    if let Some(gate) = &mut gate {
        let released = within(async {
            tokio::select! {
                _ = gate => true,
                _ = control.recv() => false,
                frame = socket.next() => {
                    assert!(matches!(frame, None | Some(Err(_)) | Some(Ok(Message::Close(_)))));
                    false
                }
            }
        })
        .await;
        if !released {
            return;
        }
    }
    within(socket.send(Message::Text("42[\"ready\"]".into())))
        .await
        .unwrap();
    within(socket.send(Message::Text("2probe".into())))
        .await
        .unwrap();
    let mut command = tokio::process::Command::new(binary);
    command
        .args(["--kata"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = within(crate::spawn_process(&mut command)).await.unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut binary = false;
    loop {
        let keep_running = within(async {
        tokio::select! {
            message = control.recv() => {
                if let Ok(message) = message {
                    if message == "close" { let _ = within(socket.close(None)).await; return false; }
                    if message == "binary-stderr" {
                        for bytes in [vec![0xe6,0x99], vec![0xba,0xe5,0xad,0x90,b'\r',b'\n']] {
                            if socket.send(Message::Text("451-[\"stderr\",{\"_placeholder\":true,\"num\":0}]".into())).await.is_err() { return false; }
                            if socket.send(Message::Binary(bytes.into())).await.is_err() { return false; }
                        }
                        return true;
                    }
                    if socket.send(Message::Text(message.into())).await.is_err() { return false; }
                }
            }
            frame = socket.next() => match frame {
                Some(Ok(Message::Text(text))) if text == "3probe" => { let _ = seen.send(Seen::Pong); }
                Some(Ok(Message::Text(text))) if text.starts_with("42") => {
                    let event: Vec<Value> = serde_json::from_str(&text[2..]).unwrap();
                    assert_eq!(event[0], "stdin");
                    let line = event[1].as_str().unwrap(); assert!(line.ends_with('\n'));
                    let _ = seen.send(Seen::Command(line.into()));
                    if crash && line.contains("kata-analyze") { let _ = within(socket.close(None)).await; return false; }
                    if within(input.write_all(line.as_bytes())).await.is_err() { return false; }
                    if line == "quit\n" { return false; }
                }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return false,
                Some(Ok(Message::Text(text))) if text == "41" => return false,
                _ => {},
            },
            line = stdout.next_line() => match line {
                Ok(Some(line)) => {
                    binary = !binary;
                    if binary {
                        if socket.send(Message::Text("451-[\"stdout\",{\"_placeholder\":true,\"num\":0}]".into())).await.is_err() { return false; }
                        if socket.send(Message::Binary(format!("{line}\n").into_bytes().into())).await.is_err() { return false; }
                    } else if socket.send(Message::Text(format!("42{}",json!(["stdout",format!("{line}\n")])).into())).await.is_err() { return false; }
                }
                _ => return false,
            }
        }
        true
        }).await;
        if !keep_running {
            break;
        }
    }
    drop(input);
    let _ = within(child.kill()).await;
    let _ = within(child.wait()).await;
}

async fn ready(engine: &crate::EngineHandle) {
    let mut status = engine.status();
    within(async {
        loop {
            match status.borrow_and_update().clone() {
                EngineStatus::Ready { .. } => return,
                EngineStatus::Failed { reason, .. } => panic!("failed: {reason}"),
                _ => {}
            }
            status.changed().await.unwrap();
        }
    })
    .await;
}

#[test]
fn ready_gates_connection_and_fake_gtp_text_binary_ping_quit_round_trip() {
    run(async {
        let (release, gate) = oneshot::channel();
        let mut server = Server::new(false, Some(gate)).await;
        let connection = server.connection.clone();
        let connecting = tokio::spawn(async move { within(start(connection, &config())).await });
        server.connected().await;
        assert!(
            !connecting.is_finished(),
            "namespace connected is not engine ready"
        );
        release.send(()).unwrap();
        let Transport {
            outgoing,
            mut incoming,
            exited,
            stop,
            driver,
        } = within(connecting).await.unwrap().unwrap();
        let task = tokio::spawn(driver);
        within(outgoing.send("1 name".into())).await.unwrap();
        let stdout = within(async {
            let mut result = Vec::new();
            while result.len() < 2 {
                if let Some(Output::Stdout(line)) = incoming.recv().await {
                    result.push(line);
                }
            }
            result
        })
        .await;
        assert!(stdout[0].contains("Fake GTP"));
        assert_eq!(stdout[1], "");
        server.command("name").await;
        server
            .control
            .send(format!(
                "42{}",
                json!(["stderr", "diagnostic secret-token-0\n"])
            ))
            .unwrap();
        assert!(
            matches!(within(incoming.recv()).await, Some(Output::Stderr(line)) if line == "diagnostic [redacted]")
        );
        server.control.send("binary-stderr".into()).unwrap();
        assert!(
            matches!(within(incoming.recv()).await, Some(Output::Stderr(line)) if line == "智子")
        );
        stop.unwrap().send(()).unwrap();
        server.command("quit").await;
        assert!(within(exited).await.unwrap().status.is_ok());
        within(task).await.unwrap();
        server.shutdown().await;
    });
}

#[test]
fn reconnect_fetches_fresh_session_token_and_replays_through_bridge() {
    run(async {
        let mut server = Server::new(true, None).await;
        let (fetch, mut fetched) = mpsc::channel::<oneshot::Sender<ZhiziConnection>>(4);
        let template = server.connection.clone();
        let source = tokio::spawn(async move {
            let mut attempt = 0;
            while let Some(reply) = within(fetched.recv()).await {
                attempt += 1;
                let mut connection = template.clone();
                connection.token = format!("fresh-token-{attempt}");
                let _ = reply.send(connection);
            }
            attempt
        });
        let connect = ZhiziConnector::new(move || {
            let fetch = fetch.clone();
            async move {
                let (reply, response) = oneshot::channel();
                within(fetch.send(reply))
                    .await
                    .map_err(|_| Error::Connect)?;
                within(response).await.map_err(|_| Error::Connect)
            }
        });
        let engine = crate::spawn(TransportSpec::Zhizi { connect }, config());
        ready(&engine).await;
        assert_eq!(server.connected().await, "fresh-token-1");
        let request: AnalysisRequest = serde_json::from_value(json!({"moves": [["B","D4"]], "rules":"chinese", "boardXSize":9,"boardYSize":9,"maxVisits":10,"reportDuringSearchEvery":0.1})).unwrap();
        let mut query = within(engine.submit(request)).await.unwrap();
        let mut restarted = false;
        let result = within(async {
            loop {
                match query.recv().await {
                    Some(QueryEvent::Restarted) => restarted = true,
                    Some(QueryEvent::Final(result)) => break result,
                    Some(QueryEvent::Partial(_)) => {}
                    event => panic!("unexpected event: {event:?}"),
                }
            }
        })
        .await;
        assert!(restarted);
        assert!(result.root_info.visits >= 10);
        assert_eq!(server.connected().await, "fresh-token-2");
        within(engine.shutdown()).await.unwrap();
        assert_eq!(within(source).await.unwrap(), 2);
        server.shutdown().await;
    });
}

#[test]
fn fatal_codes_after_ready_never_retry_and_ignore_free_text() {
    run(async {
        for payload in [
            json!({"status":401}),
            json!({"statusCode":403}),
            json!({"key":"insufficient_balance"}),
            json!({"key":"vip_expired"}),
            json!({"key":"membership_expired"}),
            json!({"key":"not_membership"}),
        ] {
            let mut server = Server::new(false, None).await;
            let engine = crate::spawn(server.spec(), config());
            ready(&engine).await;
            server.connected().await;
            let mut status = engine.status();
            server
                .control
                .send(format!("42{}", json!(["error", payload])))
                .unwrap();
            within(async {
                loop {
                    match status.borrow_and_update().clone() {
                        EngineStatus::Failed { reason, .. } => {
                            assert!(!reason.contains("secret-token"));
                            break;
                        }
                        EngineStatus::Restarting { .. } => panic!("fatal error was retried"),
                        _ => {}
                    }
                    status.changed().await.unwrap();
                }
            })
            .await;
            within(engine.shutdown()).await.unwrap();
            server.shutdown().await;
        }
        assert_eq!(Error::zhizi_fatal(None, "a balance or vip message"), None);
    });
}

#[test]
fn insecure_endpoints_rejected_without_connecting_and_debug_redacts_secrets() {
    run(async {
        for url in [
            "http://localhost:1",
            "ws://localhost:1",
            "file:///tmp/socket",
            "https://user:password@localhost",
        ] {
            let connection = ZhiziConnection::new(url.into(), "private-token".into());
            assert!(matches!(
                within(start(connection.clone(), &config())).await,
                Err(Error::InvalidConfiguration)
            ));
            assert!(!format!("{connection:?}").contains(url));
            assert!(!format!("{connection:?}").contains("private-token"));
        }
    });
}

#[test]
fn ready_timeout_and_heartbeat_timeout_use_real_time() {
    run(async {
        let (_release, gate) = oneshot::channel();
        let mut server = Server::new(false, Some(gate)).await;
        let connection = server.connection.clone();
        let connecting = tokio::spawn(async move {
            within(start_inner(
                connection,
                &config(),
                Duration::from_millis(250),
            ))
            .await
        });
        server.connected().await;
        assert!(matches!(
            within(connecting).await.unwrap(),
            Err(Error::ReadyTimeout)
        ));
        server.shutdown().await;

        // Windows child stdout uses a blocking read, which inhibits Tokio's
        // paused-clock auto-advance. Keep both this real pipe and the 5s safety
        // deadlines on real time; shorten the server-advertised heartbeat only.
        let mut server = Server::with_heartbeat(false, None, Duration::from_secs(1)).await;
        let transport = within(start(server.connection.clone(), &config()))
            .await
            .unwrap();
        let Transport {
            outgoing: _outgoing,
            incoming: _incoming,
            exited,
            stop: _stop,
            driver,
        } = transport;
        let task = tokio::spawn(driver);
        within(async {
            loop {
                if let Seen::Pong = server.seen.recv().await.unwrap() {
                    break;
                }
            }
        })
        .await;
        assert!(
            within(exited)
                .await
                .unwrap()
                .reason
                .unwrap()
                .contains("timed out")
        );
        within(task).await.unwrap();
        server.shutdown().await;
    });
}

#[test]
fn fatal_token_fetch_fails_without_any_retry_or_socket_connection() {
    run(async {
        let (calls, mut received) = mpsc::unbounded_channel();
        let connect = ZhiziConnector::new(move || {
            calls.send(()).unwrap();
            async { Err(Error::ZhiziAccessDenied) }
        });
        let engine = crate::spawn(TransportSpec::Zhizi { connect }, config());
        let mut status = engine.status();
        within(async {
            loop {
                match status.borrow_and_update().clone() {
                    EngineStatus::Failed { reason, .. } => {
                        assert!(reason.contains("access denied"));
                        break;
                    }
                    EngineStatus::Restarting { .. } => panic!("fatal token request was retried"),
                    _ => {}
                }
                status.changed().await.unwrap();
            }
        })
        .await;
        assert_eq!(received.try_recv(), Ok(()));
        assert!(received.try_recv().is_err());
        within(engine.shutdown()).await.unwrap();
    });
}

#[test]
fn trace_log_file_omits_session_tokens_and_server_echoed_accounts() {
    run(async {
        let mut server = Server::new(false, None).await;
        let directory = tempfile::tempdir().unwrap();
        let log_path = directory.path().join("socketio.log");
        let log = std::fs::File::create(&log_path).unwrap();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_max_level(tracing::Level::TRACE)
            .with_writer(move || log.try_clone().unwrap())
            .finish();
        let (commands, receiver) = mpsc::channel(64);
        let (status, status_rx) = tokio::sync::watch::channel(EngineStatus::Starting);
        let task = tokio::spawn(
            crate::actor::run(server.spec(), config(), receiver, status)
                .with_subscriber(subscriber),
        );
        let engine = crate::EngineHandle {
            commands,
            status: status_rx,
        };
        ready(&engine).await;
        server.connected().await;
        server
            .control
            .send(format!(
                "42{}",
                json!(["stderr", "person@example.test secret-token-0\n"])
            ))
            .unwrap();
        server
            .control
            .send(format!(
                "42{}",
                json!(["error",{"status":401,"message":"person@example.test secret-token-0"}])
            ))
            .unwrap();
        let mut status = engine.status();
        within(async {
            loop {
                if let EngineStatus::Failed { stderr_tail, .. } = status.borrow_and_update().clone()
                {
                    assert!(stderr_tail.is_empty());
                    break;
                }
                status.changed().await.unwrap();
            }
        })
        .await;
        within(engine.shutdown()).await.unwrap();
        within(task).await.unwrap();
        let text = std::fs::read_to_string(log_path).unwrap();
        assert!(text.contains("engine session failed"));
        assert!(!text.contains("secret-token-0"));
        assert!(!text.contains("person@example.test"));
        server.shutdown().await;
    });
}
