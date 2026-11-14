//! Loopback-only WebSocket service backed by a real fake-katago child process.
use super::within;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::{path::Path, process::Stdio};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::{mpsc, watch},
    task::JoinHandle,
};
use tokio_tungstenite::tungstenite::Message;

enum Observation {
    Request(Value),
    Closed,
}

pub(super) struct Server {
    pub url: String,
    output: mpsc::UnboundedReceiver<Observation>,
    stopping: watch::Sender<bool>,
    task: JoinHandle<()>,
}

impl Server {
    pub async fn start() -> Self {
        // Build before entering any response deadline.
        let binary = fake_katago::binary_path().expect("fake-katago binary");
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "ws://{}/analysis?token=fixture-only",
            listener.local_addr().unwrap()
        );
        let (send, output) = mpsc::unbounded_channel();
        let (stopping, mut stop) = watch::channel(false);
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = stop.changed() => return,
                    accepted = listener.accept() => {
                        let (socket, _) = accepted.unwrap();
                        serve(socket, &binary, &send, &mut stop).await;
                        if *stop.borrow() { return; }
                    }
                }
            }
        });
        Self {
            url,
            output,
            stopping,
            task,
        }
    }

    pub async fn request(&mut self, predicate: impl Fn(&Value) -> bool) -> Value {
        within(async {
            loop {
                if let Observation::Request(request) =
                    self.output.recv().await.expect("service stopped")
                {
                    if request.get("moves").is_some() {
                        assert_eq!(
                            request["overrideSettings"]["reportAnalysisWinratesAs"],
                            "BLACK"
                        );
                    }
                    if predicate(&request) {
                        return request;
                    }
                }
            }
        })
        .await
    }

    pub async fn closed(&mut self) {
        within(async {
            while !matches!(
                self.output.recv().await.expect("service stopped"),
                Observation::Closed
            ) {}
        })
        .await
    }

    pub async fn shutdown(self) {
        self.stopping.send(true).unwrap();
        within(self.task).await.unwrap();
    }
}

async fn serve(
    socket: TcpStream,
    binary: &Path,
    observed: &mpsc::UnboundedSender<Observation>,
    stop: &mut watch::Receiver<bool>,
) {
    let mut websocket = tokio_tungstenite::accept_async(socket).await.unwrap();
    let mut command = tokio::process::Command::new(binary);
    command
        .args([
            "analysis",
            "-model",
            "unused",
            "-config",
            "unused",
            "-override-config",
            "numAnalysisThreads=2",
            "--default-perspective-to-play",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = engine_session::spawn_process(&mut command).await.unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap()).lines();
    loop {
        tokio::select! {
            _ = stop.changed() => break,
            frame = websocket.next() => match frame {
                Some(Ok(Message::Text(text))) => {
                    input.write_all(text.as_bytes()).await.unwrap();
                    input.write_all(b"\n").await.unwrap();
                    let request = serde_json::from_str(&text).unwrap();
                    let _ = observed.send(Observation::Request(request));
                }
                Some(Ok(Message::Ping(bytes))) => { if websocket.send(Message::Pong(bytes)).await.is_err() { break; } }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                _ => {}
            },
            line = output.next_line() => match line.unwrap() {
                Some(line) => { if websocket.send(Message::Text(line.into())).await.is_err() { break; } }
                None => break,
            },
        }
    }
    let _ = child.start_kill();
    child.wait().await.unwrap();
    let _ = websocket.close(None).await;
    let _ = observed.send(Observation::Closed);
}
