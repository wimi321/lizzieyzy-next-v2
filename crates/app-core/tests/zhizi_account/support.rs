use crate::common::within;
use app_core::commentary_keys::{KeyStoreFuture, KeyStoreUnavailable, SecretStore};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{Barrier, mpsc, oneshot},
    task::{JoinHandle, JoinSet},
};

pub struct Stub {
    pub path: String,
    pub status: u16,
    pub body: String,
    pub headers: String,
    pub gate: Option<Arc<Barrier>>,
}
impl Stub {
    pub fn json(path: &str, value: Value) -> Self {
        Self {
            path: path.into(),
            status: 200,
            body: value.to_string(),
            headers: String::new(),
            gate: None,
        }
    }
    pub fn error(path: &str, status: u16, key: &str) -> Self {
        Self {
            status,
            ..Self::json(path, serde_json::json!({"key":key}))
        }
    }
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push_str(&format!("{name}: {value}\r\n"));
        self
    }
}
pub struct Server {
    pub base: String,
    pub requests: mpsc::UnboundedReceiver<String>,
    task: JoinHandle<()>,
}
impl Server {
    pub async fn new(mut stubs: Vec<Stub>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (sent, requests) = mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            let mut active = JoinSet::new();
            while !stubs.is_empty() {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_request(&mut stream).await;
                let path = request
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap();
                let index = stubs
                    .iter()
                    .position(|stub| stub.path == path)
                    .unwrap_or_else(|| panic!("unexpected endpoint: {path}"));
                let stub = stubs.remove(index);
                sent.send(request).unwrap();
                active.spawn(async move {
                    if let Some(gate) = stub.gate { gate.wait().await; }
                    let response = format!("HTTP/1.1 {} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n{}", stub.status, stub.body.len(), stub.headers, stub.body);
                    stream.write_all(response.as_bytes()).await.unwrap();
                });
            }
            while let Some(result) = active.join_next().await {
                result.unwrap();
            }
        });
        Self {
            base,
            requests,
            task,
        }
    }
    pub async fn next(&mut self) -> String {
        within(self.requests.recv()).await.unwrap()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn read_request(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0; 4096];
        let count = stream.read(&mut chunk).await.unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&chunk[..count]);
        if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
            let header = String::from_utf8_lossy(&bytes[..end]);
            let length = header
                .lines()
                .find_map(|line| {
                    line.to_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if bytes.len() >= end + 4 + length {
                return String::from_utf8(bytes).unwrap();
            }
        }
    }
}
pub fn body(request: &str) -> Value {
    serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap()
}

#[derive(Debug)]
pub struct Store {
    commands: mpsc::UnboundedSender<StoreCommand>,
}
enum StoreCommand {
    Read(
        String,
        oneshot::Sender<Result<Option<String>, KeyStoreUnavailable>>,
    ),
    Write(
        String,
        String,
        oneshot::Sender<Result<(), KeyStoreUnavailable>>,
    ),
    Delete(String, oneshot::Sender<Result<(), KeyStoreUnavailable>>),
    Inspect(oneshot::Sender<BTreeMap<String, String>>),
    Available(bool, oneshot::Sender<()>),
}
impl Store {
    pub fn new() -> Arc<Self> {
        let (commands, mut rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut keys = BTreeMap::new();
            let mut available = true;
            while let Some(command) = rx.recv().await {
                match command {
                    StoreCommand::Read(key, tx) => {
                        let _ = tx.send(if available {
                            Ok(keys.get(&key).cloned())
                        } else {
                            Err(KeyStoreUnavailable)
                        });
                    }
                    StoreCommand::Write(key, value, tx) => {
                        let _ = tx.send(if available {
                            keys.insert(key, value);
                            Ok(())
                        } else {
                            Err(KeyStoreUnavailable)
                        });
                    }
                    StoreCommand::Delete(key, tx) => {
                        let _ = tx.send(if available {
                            keys.remove(&key);
                            Ok(())
                        } else {
                            Err(KeyStoreUnavailable)
                        });
                    }
                    StoreCommand::Inspect(tx) => {
                        let _ = tx.send(keys.clone());
                    }
                    StoreCommand::Available(value, tx) => {
                        available = value;
                        let _ = tx.send(());
                    }
                }
            }
        });
        Arc::new(Self { commands })
    }
    pub async fn inspect(&self) -> BTreeMap<String, String> {
        let (tx, rx) = oneshot::channel();
        self.commands.send(StoreCommand::Inspect(tx)).unwrap();
        within(rx).await.unwrap()
    }
    pub async fn available(&self, available: bool) {
        let (tx, rx) = oneshot::channel();
        self.commands
            .send(StoreCommand::Available(available, tx))
            .unwrap();
        within(rx).await.unwrap();
    }
}
impl SecretStore for Store {
    fn read(&self, key: String) -> KeyStoreFuture<'_, Option<String>> {
        Box::pin(async move {
            let (tx, rx) = oneshot::channel();
            self.commands
                .send(StoreCommand::Read(key, tx))
                .map_err(|_| KeyStoreUnavailable)?;
            rx.await.map_err(|_| KeyStoreUnavailable)?
        })
    }
    fn write(&self, key: String, value: String) -> KeyStoreFuture<'_, ()> {
        Box::pin(async move {
            let (tx, rx) = oneshot::channel();
            self.commands
                .send(StoreCommand::Write(key, value, tx))
                .map_err(|_| KeyStoreUnavailable)?;
            rx.await.map_err(|_| KeyStoreUnavailable)?
        })
    }
    fn delete(&self, key: String) -> KeyStoreFuture<'_, ()> {
        Box::pin(async move {
            let (tx, rx) = oneshot::channel();
            self.commands
                .send(StoreCommand::Delete(key, tx))
                .map_err(|_| KeyStoreUnavailable)?;
            rx.await.map_err(|_| KeyStoreUnavailable)?
        })
    }
}

#[derive(Debug)]
pub struct HangingStore {
    pub phase: &'static str,
    pub operations: mpsc::UnboundedSender<&'static str>,
}
impl SecretStore for HangingStore {
    fn read(&self, _: String) -> KeyStoreFuture<'_, Option<String>> {
        Box::pin(async move {
            self.operations.send("read").unwrap();
            if self.phase == "read" {
                std::future::pending().await
            } else {
                Ok(None)
            }
        })
    }
    fn write(&self, _: String, _: String) -> KeyStoreFuture<'_, ()> {
        Box::pin(async move {
            self.operations.send("write").unwrap();
            if self.phase == "write" {
                std::future::pending().await
            } else {
                Ok(())
            }
        })
    }
    fn delete(&self, _: String) -> KeyStoreFuture<'_, ()> {
        Box::pin(async move {
            self.operations.send("delete").unwrap();
            if self.phase == "delete" {
                std::future::pending().await
            } else {
                Ok(())
            }
        })
    }
}

/// Mimics an OS call already running in spawn_blocking: dropping its caller
/// cannot cancel the eventual write, while another native delete could finish.
#[derive(Debug)]
pub struct DelayedNativeStore {
    pub inner: Arc<Store>,
    pub gate: tokio::sync::watch::Receiver<bool>,
    pub operations: mpsc::UnboundedSender<&'static str>,
}
impl SecretStore for DelayedNativeStore {
    fn read(&self, account: String) -> KeyStoreFuture<'_, Option<String>> {
        self.inner.read(account)
    }
    fn delete(&self, account: String) -> KeyStoreFuture<'_, ()> {
        Box::pin(async move {
            self.operations.send("delete").unwrap();
            self.inner.delete(account).await
        })
    }
    fn write(&self, account: String, token: String) -> KeyStoreFuture<'_, ()> {
        let inner = self.inner.clone();
        let mut gate = self.gate.clone();
        let operations = self.operations.clone();
        Box::pin(async move {
            tokio::spawn(async move {
                operations.send("write-start").unwrap();
                while !*gate.borrow_and_update() {
                    gate.changed().await.unwrap();
                }
                inner.write(account, token).await.unwrap();
                operations.send("write-done").unwrap();
            })
            .await
            .map_err(|_| KeyStoreUnavailable)?;
            Ok(())
        })
    }
}
