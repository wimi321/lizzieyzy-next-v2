use crate::within;
use app_core::*;
use serde_json::Value;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    task::JoinHandle,
};

pub enum Step {
    Text(String),
    Done,
    End,
}
pub struct Server {
    pub base: String,
    pub requests: mpsc::Receiver<Value>,
    pub steps: mpsc::Sender<Step>,
    pub closed: mpsc::Receiver<()>,
    task: JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn request(stream: &mut TcpStream) -> Value {
    let mut bytes = Vec::new();
    loop {
        let mut buf = [0; 4096];
        let count = stream.read(&mut buf).await.unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&buf[..count]);
        if let Some(offset) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&bytes[..offset]);
            let length: usize = head
                .lines()
                .find_map(|line| {
                    let (k, v) = line.split_once(':')?;
                    k.eq_ignore_ascii_case("content-length")
                        .then(|| v.trim().parse().unwrap())
                })
                .unwrap_or(0);
            if bytes.len() >= offset + 4 + length {
                return serde_json::from_slice(&bytes[offset + 4..offset + 4 + length]).unwrap();
            }
        }
    }
}
async fn chunk(stream: &mut TcpStream, data: &str) -> bool {
    stream
        .write_all(format!("{:x}\r\n{data}\r\n", data.len()).as_bytes())
        .await
        .is_ok()
}
impl Server {
    pub async fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let (requests_tx, requests) = mpsc::channel(8);
        let (steps, mut steps_rx) = mpsc::channel(8);
        let (closed_tx, closed) = mpsc::channel(8);
        let task = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let body = request(&mut stream).await;
                if requests_tx.send(body).await.is_err() {
                    break;
                }
                if stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").await.is_err(){continue;}
                loop {
                    let mut byte = [0];
                    tokio::select! {
                        n=stream.read(&mut byte)=>{if !matches!(n,Ok(1)){let _=closed_tx.send(()).await;break;}}
                        step=steps_rx.recv()=>match step {
                            Some(Step::Text(text))=>{let data=format!("data: {}\n\n",serde_json::json!({"choices":[{"delta":{"content":text}}]}));if !chunk(&mut stream,&data).await{break;}}
                            Some(Step::Done)=>{let _=chunk(&mut stream,"data: [DONE]\n\n").await;let _=stream.write_all(b"0\r\n\r\n").await;break;}
                            Some(Step::End)=>{let _=stream.write_all(b"0\r\n\r\n").await;break;}
                            None=>return,
                        }
                    }
                }
            }
        });
        Self {
            base,
            requests,
            steps,
            closed,
            task,
        }
    }
    pub async fn answer(&self, text: &str) {
        within(self.steps.send(Step::Text(text.into())))
            .await
            .unwrap();
        within(self.steps.send(Step::Done)).await.unwrap();
    }
}
pub const SAVED: &[u8] = include_bytes!("../../../../fixtures/lz/java-analysis.sgf");
pub async fn app(base: &str) -> (tempfile::TempDir, AppCore, AppSubscription) {
    let dir = tempfile::tempdir().unwrap();
    let (app, events) = within(app_core::spawn(dir.path())).await.unwrap();
    within(app.set_commentary_settings(CommentarySettings {
        base_url: base.into(),
        model: "local-test-model".into(),
        ..Default::default()
    }))
    .await
    .unwrap();
    within(app.set_commentary_key("test-only-secret".into(), false))
        .await
        .unwrap();
    within(app.open_bytes(SAVED.to_vec())).await.unwrap();
    (dir, app, events)
}
pub async fn progress(events: &mut AppSubscription, phase: CommentaryPhase) -> CommentaryUpdated {
    within(async {
        loop {
            if let AppEvent::CommentaryUpdated(view) = events.recv().await.unwrap()
                && view.phase == phase
            {
                return view;
            }
        }
    })
    .await
}
pub async fn document(events: &mut AppSubscription) -> DocumentView {
    within(async {
        loop {
            if let AppEvent::DocumentChanged { view, .. } = events.recv().await.unwrap() {
                return view;
            }
        }
    })
    .await
}
pub fn req(mode: CommentaryMode) -> CommentaryRequest {
    CommentaryRequest {
        mode,
        follow_up: None,
    }
}
