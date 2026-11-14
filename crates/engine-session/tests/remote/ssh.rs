use super::common::*;
use engine_session::{
    QueryEvent, RemoteTransportError, SshAuth, TransportSpec, probe_remote, spawn,
};
use russh::{
    Channel, ChannelId, ChannelMsg,
    keys::{Algorithm, PrivateKey, PublicKey},
    server::{self, ChannelOpenHandle, Msg, Session, VerifiedPublicKey},
};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
    sync::{broadcast, mpsc, oneshot},
    task::{JoinHandle, JoinSet},
};

#[derive(Debug)]
enum Event {
    Password(String),
    PublicKey,
    Ended,
    Request(Value),
}
struct Start {
    channel: Channel<Msg>,
    crash: bool,
    secret: String,
}
struct Handler {
    channel: Option<Channel<Msg>>,
    events: mpsc::UnboundedSender<Event>,
    starts: mpsc::UnboundedSender<Start>,
    key: PublicKey,
    crash: bool,
    secret: String,
}
impl server::Handler for Handler {
    type Error = russh::Error;
    async fn auth_password(
        &mut self,
        user: &str,
        password: &str,
    ) -> Result<server::Auth, Self::Error> {
        self.secret = password.to_owned();
        let _ = self.events.send(Event::Password(password.to_owned()));
        Ok(if user == "test" && password == "ssh-secret-password" {
            server::Auth::Accept
        } else {
            server::Auth::reject()
        })
    }
    async fn auth_publickey(
        &mut self,
        user: &str,
        key: VerifiedPublicKey<'_>,
    ) -> Result<server::Auth, Self::Error> {
        let _ = self.events.send(Event::PublicKey);
        Ok(if user == "test" && key.public_key == &self.key {
            server::Auth::Accept
        } else {
            server::Auth::reject()
        })
    }
    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channel = Some(channel);
        reply.accept().await;
        Ok(())
    }
    async fn exec_request(
        &mut self,
        id: ChannelId,
        command: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        assert_eq!(command, b"fake-katago analysis");
        session.channel_success(id)?;
        self.starts
            .send(Start {
                channel: self.channel.take().unwrap(),
                crash: self.crash,
                secret: self.secret.clone(),
            })
            .unwrap();
        Ok(())
    }
}

struct Server {
    port: u16,
    events: mpsc::UnboundedReceiver<Event>,
    key: PrivateKey,
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
}
impl Server {
    async fn new(crash_first: bool, rotate_key: bool) -> Self {
        let binary = fake_katago::binary_path().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let host_key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
        let second_host_key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
        let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
        let accepted_key = key.public_key().clone();
        let (events_tx, events) = mpsc::unbounded_channel();
        let (starts_tx, mut starts) = mpsc::unbounded_channel::<Start>();
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
                        let config = Arc::new(server::Config {
                            keys: vec![if !first && rotate_key { second_host_key.clone() } else { host_key.clone() }],
                            auth_rejection_time: Duration::ZERO,
                            auth_rejection_time_initial: Some(Duration::ZERO),
                            ..server::Config::default()
                        });
                        let handler = Handler { channel: None, events: events_tx.clone(), starts: starts_tx.clone(), key: accepted_key.clone(), crash: crash_first && first, secret: String::new() };
                        first = false;
                        let events = events_tx.clone();
                        let mut closing = close.subscribe();
                        sessions.spawn(async move {
                            if let Ok(mut session) = server::run_stream(config, stream, handler).await {
                                let handle = session.handle();
                                tokio::select! {
                                    _ = &mut session => {},
                                    _ = closing.recv() => { let _ = handle.disconnect(russh::Disconnect::ByApplication, String::new(), String::new()).await; let _ = session.await; },
                                }
                            }
                            let _ = events.send(Event::Ended);
                        });
                    },
                    Some(start) = starts.recv() => {
                        sessions.spawn(bridge(start.channel, binary.clone(), start.crash, start.secret, events_tx.clone(), close.subscribe()));
                    },
                    _ = sessions.join_next(), if !sessions.is_empty() => {},
                }
            }
            let _ = close.send(());
            while let Some(result) = sessions.join_next().await {
                result.unwrap();
            }
        });
        Self {
            port,
            events,
            key,
            stop,
            task,
        }
    }
    fn spec(&self, directory: &std::path::Path, auth: SshAuth) -> TransportSpec {
        TransportSpec::Ssh {
            host: "127.0.0.1".into(),
            port: self.port,
            user: "test".into(),
            auth,
            command: "fake-katago analysis".into(),
            known_hosts: directory.join("ssh/known_hosts"),
        }
    }
    async fn event(&mut self) -> Event {
        within(self.events.recv()).await.unwrap()
    }
    async fn request(&mut self) -> Value {
        loop {
            if let Event::Request(value) = self.event().await
                && value.get("moves").is_some()
            {
                return value;
            }
        }
    }
    async fn ended(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        loop {
            let event = self.event().await;
            if matches!(event, Event::Ended) {
                return events;
            }
            events.push(event);
        }
    }
    async fn shutdown(self) {
        let _ = self.stop.send(());
        within(self.task).await.unwrap();
    }
}

async fn bridge(
    mut channel: Channel<Msg>,
    binary: PathBuf,
    crash: bool,
    secret: String,
    events: mpsc::UnboundedSender<Event>,
    mut close: broadcast::Receiver<()>,
) {
    let mut child = fake(&binary).await;
    let mut input = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut pending = String::new();
    // Exercise stderr separately from stdout, including secret echo redaction.
    let stderr = format!("remote stderr {secret}\n");
    let _ = channel.extended_data(1, stderr.as_bytes()).await;
    loop {
        tokio::select! {
            _ = close.recv() => break,
            message = channel.wait() => match message {
                Some(ChannelMsg::Data { data }) => {
                    pending.push_str(std::str::from_utf8(&data).unwrap());
                    while let Some(end) = pending.find('\n') {
                        let line: String = pending.drain(..=end).collect();
                        let value: Value = serde_json::from_str(&line).unwrap();
                        let _ = events.send(Event::Request(value.clone()));
                        if crash && value.get("moves").is_some() {
                            let _ = channel.close().await;
                            let _ = child.kill().await;
                            let _ = child.wait().await;
                            return;
                        }
                        if input.write_all(line.as_bytes()).await.is_err() { break; }
                    }
                },
                Some(ChannelMsg::Eof | ChannelMsg::Close) | None => break,
                _ => {},
            },
            line = stdout.next_line() => match line {
                Ok(Some(mut line)) => {
                    line.push('\n');
                    // Split each line across SSH data packets to check buffering.
                    let middle = line.len()/2;
                    if channel.data(&line.as_bytes()[..middle]).await.is_err() { break; }
                    if channel.data(&line.as_bytes()[middle..]).await.is_err() { break; }
                },
                _ => break,
            }
        }
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
    let _ = channel.close().await;
}
fn password() -> SshAuth {
    SshAuth::Password("ssh-secret-password".into())
}

#[tokio::test]
async fn ssh_password_tofu_live_whole_game_cancel_black_and_stderr() {
    let mut server = Server::new(false, false).await;
    let directory = tempfile::tempdir().unwrap();
    let spec = server.spec(directory.path(), password());
    assert_eq!(
        within(probe_remote(&spec, config())).await.unwrap(),
        "1.16.5-fake"
    );
    let events = server.ended().await;
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Password(value) if value == "ssh-secret-password"))
    );
    let known_hosts = std::fs::read_to_string(directory.path().join("ssh/known_hosts")).unwrap();
    assert!(known_hosts.starts_with(&format!("[127.0.0.1]:{} ssh-ed25519 ", server.port)));
    assert!(!known_hosts.contains("ssh-secret-password"));
    let engine = spawn(spec, config());
    ready(&engine).await;
    let mut live = engine
        .submit(request(
            json!({"maxVisits":1000000,"reportDuringSearchEvery":0.001}),
        ))
        .await
        .unwrap();
    assert!(matches!(next(&mut live).await, QueryEvent::Partial(_)));
    assert_eq!(
        server.request().await["overrideSettings"]["reportAnalysisWinratesAs"],
        "BLACK"
    );
    engine.terminate(live.id().into()).await.unwrap();
    assert_eq!(next(&mut live).await, QueryEvent::Terminated);
    let mut review = engine
        .submit(request(
            json!({"moves":[["B","D4"],["W","E5"]],"analyzeTurns":[0,1,2]}),
        ))
        .await
        .unwrap();
    assert_eq!(
        complete(&mut review)
            .await
            .iter()
            .map(|response| response.turn_number)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert_eq!(
        server.request().await["overrideSettings"]["reportAnalysisWinratesAs"],
        "BLACK"
    );
    within(engine.shutdown()).await.unwrap();
    server.shutdown().await;
}

#[tokio::test]
async fn ssh_private_key_and_encrypted_private_key_authenticate() {
    let mut server = Server::new(false, false).await;
    let directory = tempfile::tempdir().unwrap();
    for passphrase in [None, Some("private-key-secret-passphrase")] {
        let path = directory.path().join(if passphrase.is_some() {
            "encrypted"
        } else {
            "plain"
        });
        let key = if let Some(password) = passphrase {
            server.key.encrypt(&mut rand::rng(), password).unwrap()
        } else {
            server.key.clone()
        };
        std::fs::write(
            &path,
            key.to_openssh(russh::keys::ssh_key::LineEnding::LF)
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
        let spec = server.spec(
            directory.path(),
            SshAuth::PrivateKey {
                path,
                passphrase: passphrase.map(str::to_owned),
            },
        );
        assert_eq!(
            within(probe_remote(&spec, config())).await.unwrap(),
            "1.16.5-fake"
        );
        assert!(
            server
                .ended()
                .await
                .iter()
                .any(|event| matches!(event, Event::PublicKey))
        );
    }
    server.shutdown().await;
}

#[tokio::test]
async fn ssh_changed_key_rejected_before_password_or_key_authentication() {
    let mut server = Server::new(false, true).await;
    let directory = tempfile::tempdir().unwrap();
    let spec = server.spec(directory.path(), password());
    within(probe_remote(&spec, config())).await.unwrap();
    server.ended().await;
    let original = std::fs::read(directory.path().join("ssh/known_hosts")).unwrap();
    let error = within(probe_remote(&spec, config())).await.unwrap_err();
    let RemoteTransportError::HostKeyChanged {
        old_fingerprint,
        new_fingerprint,
        path,
    } = error
    else {
        panic!("wrong error: {error:?}");
    };
    assert_ne!(old_fingerprint, new_fingerprint);
    assert!(old_fingerprint.starts_with("SHA256:"));
    assert_eq!(path, directory.path().join("ssh/known_hosts"));
    assert!(
        !server
            .ended()
            .await
            .iter()
            .any(|event| matches!(event, Event::Password(_) | Event::PublicKey))
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
    let private = server.spec(
        directory.path(),
        SshAuth::PrivateKey {
            path: directory.path().join("key-must-not-be-opened"),
            passphrase: Some("private-key-secret-passphrase".into()),
        },
    );
    assert!(matches!(
        within(probe_remote(&private, config())).await,
        Err(RemoteTransportError::HostKeyChanged { .. })
    ));
    assert!(
        !server
            .ended()
            .await
            .iter()
            .any(|event| matches!(event, Event::Password(_) | Event::PublicKey))
    );
    assert_eq!(std::fs::read(path).unwrap(), original);
    server.shutdown().await;
}

#[tokio::test]
async fn ssh_authentication_failure_is_typed_and_secret_free() {
    let mut server = Server::new(false, false).await;
    let directory = tempfile::tempdir().unwrap();
    let spec = server.spec(
        directory.path(),
        SshAuth::Password("wrong-secret-password".into()),
    );
    assert!(!format!("{spec:?}").contains("wrong-secret-password"));
    let error = within(probe_remote(&spec, config())).await.unwrap_err();
    assert_eq!(error, RemoteTransportError::Authentication);
    assert!(!format!("{error:?} {error}").contains("wrong-secret-password"));
    assert!(
        server
            .ended()
            .await
            .iter()
            .any(|event| matches!(event, Event::Password(_)))
    );
    server.shutdown().await;
}

#[tokio::test]
async fn ssh_disconnect_reconnects_and_replays_query() {
    let mut server = Server::new(true, false).await;
    let directory = tempfile::tempdir().unwrap();
    let engine = spawn(server.spec(directory.path(), password()), config());
    ready(&engine).await;
    let mut query = engine.submit(request(json!({}))).await.unwrap();
    let first = server.request().await;
    assert_eq!(next(&mut query).await, QueryEvent::Restarted);
    assert_ne!(first["id"], server.request().await["id"]);
    assert_eq!(complete(&mut query).await.len(), 1);
    within(engine.shutdown()).await.unwrap();
    server.shutdown().await;
}

#[tokio::test]
async fn remote_trace_logs_redact_url_tokens_and_ssh_passwords() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("transport.log");
    let (logs_tx, mut logs_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .without_time()
        .with_ansi(false)
        .with_writer(move || LogWriter(logs_tx.clone()))
        .finish();
    tracing::subscriber::set_global_default(subscriber).unwrap();
    let server = Server::new(false, false).await;
    let engine = spawn(server.spec(directory.path(), password()), config());
    ready(&engine).await;
    within(engine.shutdown()).await.unwrap();
    server.shutdown().await;
    super::websocket::exercise_for_logs().await;
    let mut bytes = Vec::new();
    while let Ok(chunk) = logs_rx.try_recv() {
        bytes.extend(chunk);
    }
    std::fs::write(&path, bytes).unwrap();
    let logs = std::fs::read_to_string(path).unwrap();
    assert!(
        logs.contains("remote stderr [redacted]"),
        "actual stderr is recorded after redaction"
    );
    assert!(!logs.contains("ssh-secret-password"));
    assert!(!logs.contains("wrong-secret-password"));
    assert!(!logs.contains("private-key-secret-passphrase"));
    assert!(
        logs.contains("invalid remote engine output"),
        "malformed remote output reaches diagnostics without quoting its content"
    );
    assert!(!logs.contains("remote-secret-token"));
}

struct LogWriter(mpsc::UnboundedSender<Vec<u8>>);
impl std::io::Write for LogWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let _ = self.0.send(bytes.to_vec());
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
