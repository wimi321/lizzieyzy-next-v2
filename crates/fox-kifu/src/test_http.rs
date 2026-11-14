#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{future::Future, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
    task::JoinHandle,
};

#[derive(Clone)]
pub(crate) enum Reply {
    Body(u16, Vec<u8>),
    Redirect(String),
    TooLarge,
    Truncated,
    Idle,
}

impl Reply {
    pub(crate) fn json(text: impl Into<String>) -> Self {
        Self::Body(200, text.into().into_bytes())
    }
}

pub(crate) struct Server {
    pub(crate) base: String,
    stop: oneshot::Sender<()>,
    task: JoinHandle<Vec<String>>,
}

impl Server {
    pub(crate) async fn start(replies: Vec<Reply>) -> Self {
        assert!(!replies.is_empty());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (stop, mut stopped) = oneshot::channel();
        let task = tokio::spawn(async move {
            let mut seen = Vec::new();
            loop {
                let (mut socket, _) = tokio::select! {
                    _ = &mut stopped => break,
                    accepted = listener.accept() => accepted.unwrap(),
                };
                let text = request(&mut socket).await;
                let index = seen.len().min(replies.len() - 1);
                seen.push(text);
                tokio::select! {
                    _ = &mut stopped => break,
                    () = respond(&mut socket, &replies[index]) => {},
                }
            }
            seen
        });
        Self { base, stop, task }
    }

    pub(crate) async fn finish(self) -> Vec<String> {
        let _ = self.stop.send(());
        within(self.task).await.unwrap()
    }
}

pub(crate) async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("test deadline")
}

async fn request(socket: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    loop {
        let mut buffer = [0; 4096];
        let count = socket.read(&mut buffer).await.unwrap();
        assert_ne!(count, 0);
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..end]);
            let length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .map(|n| n.parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if bytes.len() >= end + 4 + length {
                return String::from_utf8(bytes).unwrap();
            }
        }
    }
}

async fn respond(socket: &mut TcpStream, reply: &Reply) {
    let (status, length, extra, body) = match reply {
        Reply::Body(status, body) => (*status, body.len(), String::new(), body.as_slice()),
        Reply::Redirect(url) => (302, 0, format!("Location: {url}\r\n"), &[][..]),
        Reply::TooLarge => (200, crate::MAX_RESPONSE_BYTES + 1, String::new(), &[][..]),
        Reply::Truncated | Reply::Idle => (200, 100, String::new(), &b"{\"result\":"[..]),
    };
    let headers = format!(
        "HTTP/1.1 {status} Replay\r\nContent-Length: {length}\r\n{extra}Connection: close\r\n\r\n"
    );
    if socket.write_all(headers.as_bytes()).await.is_err() {
        return;
    }
    if socket.write_all(body).await.is_err() {
        return;
    }
    if matches!(reply, Reply::Idle) {
        let mut ignored = Vec::new();
        let _ = socket.read_to_end(&mut ignored).await;
    }
}
