#![allow(dead_code)]

use app_core::{
    AppEvent, AppSubscription, DocumentView,
    fox::{FoxClient, FoxEndpoints},
    tencent::{TencentClient, TencentEndpoints},
};
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};

use crate::common::within;

pub struct Reply {
    pub status: u16,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
}

impl Reply {
    pub fn body(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            headers: Vec::new(),
            body: body.into(),
        }
    }

    pub fn redirect(location: impl Into<String>) -> Self {
        Self {
            status: 302,
            headers: vec![("Location", location.into())],
            body: Vec::new(),
        }
    }

    pub fn too_large() -> Self {
        Self {
            status: 200,
            headers: vec![("Content-Length", (4 * 1024 * 1024 + 1).to_string())],
            body: Vec::new(),
        }
    }
}

pub struct Server {
    pub base: String,
    task: JoinHandle<Vec<String>>,
}

impl Server {
    pub async fn start(replies: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let mut requests = Vec::new();
            for reply in replies {
                let (mut socket, _) = listener.accept().await.unwrap();
                requests.push(read_request(&mut socket).await);
                respond(&mut socket, &reply).await;
            }
            requests
        });
        Self { base, task }
    }

    pub fn tencent_client(&self) -> TencentClient {
        tencent_client(&self.base)
    }

    pub fn fox_client(&self) -> FoxClient {
        FoxClient::with_endpoints(
            FoxEndpoints {
                user_url: format!("{}/user", self.base),
                list_url: format!("{}/fox-list", self.base),
                sgf_urls: vec![format!("{}/fox-sgf", self.base)],
            },
            Duration::from_secs(4),
        )
        .unwrap()
    }

    pub async fn finish(self) -> Vec<String> {
        within(self.task).await.unwrap()
    }
}

pub fn tencent_client(base: &str) -> TencentClient {
    TencentClient::with_endpoints(
        TencentEndpoints {
            list_url: format!("{base}/tencent-list"),
            sgf_url: format!("{base}/tencent-sgf"),
        },
        Duration::from_secs(4),
    )
    .unwrap()
}

pub async fn read_request(socket: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    loop {
        let mut buffer = [0_u8; 4096];
        let count = socket.read(&mut buffer).await.unwrap();
        assert_ne!(count, 0, "incomplete request");
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&bytes[..end]);
            let length = headers
                .lines()
                .find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if bytes.len() >= end + 4 + length {
                return String::from_utf8(bytes).unwrap();
            }
        }
    }
}

pub async fn respond(socket: &mut TcpStream, reply: &Reply) {
    let mut headers = format!("HTTP/1.1 {} Replay\r\nConnection: close\r\n", reply.status);
    if !reply
        .headers
        .iter()
        .any(|(key, _)| key.eq_ignore_ascii_case("Content-Length"))
    {
        headers.push_str(&format!("Content-Length: {}\r\n", reply.body.len()));
    }
    for (key, value) in &reply.headers {
        headers.push_str(&format!("{key}: {value}\r\n"));
    }
    headers.push_str("\r\n");
    socket.write_all(headers.as_bytes()).await.unwrap();
    // Oversize and redirect responses may be rejected before their body is read.
    let _ = socket.write_all(&reply.body).await;
    let _ = socket.shutdown().await;
}

pub async fn document_event(events: &mut AppSubscription, expected: u64) -> DocumentView {
    within(async {
        loop {
            if let AppEvent::DocumentChanged { revision, view } = events.recv().await.unwrap() {
                assert_eq!(revision, expected);
                return view;
            }
        }
    })
    .await
}
