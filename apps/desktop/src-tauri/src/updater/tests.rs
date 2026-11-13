#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use app_core::updates::{self, UpdateHandle, UpdatePhase};
use serde_json::{Value, json};
use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::mpsc,
};

mod automatic;
#[path = "test_vectors.rs"]
mod vectors;

const SHA256: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

struct Server {
    url: String,
    requests: mpsc::UnboundedReceiver<String>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Server {
    async fn new(edit: impl FnOnce(&mut Value), payload: Vec<u8>, hold: bool) -> Self {
        Self::with_manifest_response(edit, payload, hold, None, false).await
    }

    async fn with_manifest_response(
        edit: impl FnOnce(&mut Value),
        payload: Vec<u8>,
        hold: bool,
        mut manifest_gate: Option<oneshot::Receiver<()>>,
        disconnect_manifest: bool,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mut manifest = json!({
            "version": "2.0.0",
            "notes": "A local signed update",
            "pub_date": "2026-11-01T00:00:00Z",
            "platforms": {"test-target": {
                "url": format!("{url}/package"),
                "signature": vectors::SIGNATURE,
                "sha256": SHA256,
            }},
        });
        edit(&mut manifest);
        let manifest = serde_json::to_vec(&manifest).unwrap();
        let (send, requests) = mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    if socket.read_exact(&mut byte).await.is_err() {
                        break;
                    }
                    request.push(byte[0]);
                    assert!(request.len() <= 8192);
                }
                let path = String::from_utf8_lossy(&request)
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("")
                    .to_owned();
                let _ = send.send(path.clone());
                if path == "/latest.json" {
                    if let Some(gate) = manifest_gate.take() {
                        let _ = gate.await;
                    }
                    if disconnect_manifest {
                        continue;
                    }
                }
                let body = if path == "/package" {
                    &payload
                } else {
                    &manifest
                };
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                if socket.write_all(header.as_bytes()).await.is_err() {
                    continue;
                }
                if path == "/package" && hold {
                    let _ = socket.write_all(&body[..2]).await;
                    // Read until the client closes the cancelled response. No
                    // timer or bandwidth assumption drives this handshake.
                    let mut scratch = [0];
                    let _ = socket.read(&mut scratch).await;
                } else {
                    let _ = socket.write_all(body).await;
                }
            }
        });
        Self {
            url,
            requests,
            task,
        }
    }

    async fn assert_no_pending_requests(&mut self) {
        // The caller first finishes all update operations and shuts their
        // actor down. This marker drains existing accepts, without measuring
        // a quiet interval or depending on network scheduling speed.
        let mut marker = within(tokio::net::TcpStream::connect(
            self.url.strip_prefix("http://").unwrap(),
        ))
        .await
        .unwrap();
        marker
            .write_all(b"GET /test-fence HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();
        assert_eq!(within(self.requests.recv()).await.unwrap(), "/test-fence");
    }
}

fn adapter(server: &Server, directory: &TempDir) -> (tauri::App<MockRuntime>, OfficialUpdater) {
    let mut context = mock_context(noop_assets());
    context.config_mut().plugins.0.insert(
        "updater".into(),
        json!({
            "pubkey": "",
            "requireSignedVersion": true,
            "dangerousInsecureTransportProtocol": true,
        }),
    );
    let app = register_with_key(mock_builder(), Some(vectors::PUBLIC_KEY))
        .build(context)
        .unwrap();
    let updater = app
        .updater_builder()
        .target("test-target")
        .endpoints(vec![format!("{}/latest.json", server.url).parse().unwrap()])
        .unwrap()
        .executable_path(
            directory
                .path()
                .join("installation/App.app/Contents/MacOS/app"),
        )
        .no_proxy()
        .build()
        .unwrap();
    (
        app,
        OfficialUpdater {
            updater,
            selected: None,
            allow_loopback: true,
        },
    )
}

async fn terminal(handle: &UpdateHandle, wanted: UpdatePhase) -> UpdateDownloadStatus {
    let mut status = handle.subscribe_status();
    within(async {
        loop {
            let current = status.borrow_and_update().clone();
            if current.phase == wanted {
                return current;
            }
            status.changed().await.unwrap();
        }
    })
    .await
}

async fn within<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("update safety deadline")
}

#[tokio::test]
async fn official_local_manifest_new_version_and_valid_signed_download() {
    let directory = TempDir::new().unwrap();
    let server = Server::new(|_| {}, b"test".to_vec(), false).await;
    let (_app, backend) = adapter(&server, &directory);
    let handle = updates::spawn(Box::new(backend));
    let checked = within(handle.check()).await.unwrap();
    assert!(checked.available);
    assert_eq!(checked.version.as_deref(), Some("2.0.0"));
    assert_eq!(checked.notes.as_deref(), Some("A local signed update"));
    assert_eq!(
        checked.published_at.as_deref(),
        Some("2026-11-01T00:00:00Z")
    );
    within(handle.download()).await.unwrap();
    let status = terminal(&handle, UpdatePhase::Downloaded).await;
    assert_eq!(status.downloaded_bytes, 4);
    assert_eq!(status.total_bytes, Some(4));
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn official_local_manifest_no_new_version() {
    let directory = TempDir::new().unwrap();
    let server = Server::new(
        |manifest| manifest["version"] = json!("0.0.0"),
        b"test".to_vec(),
        false,
    )
    .await;
    let (_app, backend) = adapter(&server, &directory);
    let handle = updates::spawn(Box::new(backend));
    assert_eq!(
        within(handle.check()).await.unwrap(),
        UpdateCheck::default()
    );
    assert!(matches!(
        within(handle.download()).await,
        Err(UpdateError::NotAvailable)
    ));
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn official_local_manifest_rejects_malformed_json_shape_and_missing_hash() {
    for missing_hash in [false, true] {
        let directory = TempDir::new().unwrap();
        let server = Server::new(
            |manifest| {
                if missing_hash {
                    manifest["platforms"]["test-target"]["sha256"] = Value::Null;
                } else {
                    manifest["version"] = json!({"not": "a version"});
                }
            },
            b"test".to_vec(),
            false,
        )
        .await;
        let (_app, backend) = adapter(&server, &directory);
        let handle = updates::spawn(Box::new(backend));
        assert!(within(handle.check()).await.is_err());
        assert!(matches!(
            within(handle.install()).await,
            Err(UpdateError::NotDownloaded)
        ));
        within(handle.shutdown()).await.unwrap();
    }
}

#[tokio::test]
async fn official_bad_signature_never_becomes_installable_and_preserves_user_files() {
    let directory = TempDir::new().unwrap();
    let settings = directory.path().join("user-data");
    std::fs::create_dir(&settings).unwrap();
    for name in ["settings.json", "analysis-cache.sqlite3", "game.sgf"] {
        std::fs::write(settings.join(name), format!("untouched {name}")).unwrap();
    }
    let server = Server::new(|_| {}, b"evil".to_vec(), false).await;
    let (_app, mut backend) = adapter(&server, &directory);
    within(backend.check()).await.unwrap();
    // Exercise the official install entry itself, not only AppCore's state gate.
    assert!(within(backend.install(b"evil".to_vec())).await.is_err());
    let handle = updates::spawn(Box::new(backend));
    within(handle.check()).await.unwrap();
    within(handle.download()).await.unwrap();
    assert!(terminal(&handle, UpdatePhase::Failed).await.error.is_some());
    assert!(matches!(
        within(handle.install()).await,
        Err(UpdateError::NotDownloaded)
    ));
    for name in ["settings.json", "analysis-cache.sqlite3", "game.sgf"] {
        assert_eq!(
            std::fs::read_to_string(settings.join(name)).unwrap(),
            format!("untouched {name}")
        );
    }
    assert_eq!(std::fs::read_dir(&settings).unwrap().count(), 3);
    assert!(!directory.path().join("installation").exists());
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn official_local_stream_can_be_cancelled_without_waiting_for_server() {
    let directory = TempDir::new().unwrap();
    let mut server = Server::new(|_| {}, b"test".to_vec(), true).await;
    let (_app, backend) = adapter(&server, &directory);
    let handle = updates::spawn(Box::new(backend));
    within(handle.check()).await.unwrap();
    assert_eq!(
        within(server.requests.recv()).await.unwrap(),
        "/latest.json"
    );
    within(handle.download()).await.unwrap();
    assert_eq!(within(server.requests.recv()).await.unwrap(), "/package");
    let mut status = handle.subscribe_status();
    within(async {
        while status.borrow_and_update().downloaded_bytes != 2 {
            status.changed().await.unwrap();
        }
    })
    .await;
    assert_eq!(
        within(handle.cancel()).await.unwrap().phase,
        UpdatePhase::Cancelled
    );
    assert!(matches!(
        within(handle.install()).await,
        Err(UpdateError::NotDownloaded)
    ));
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn authentic_signature_cannot_be_relabelled_as_another_version() {
    let directory = TempDir::new().unwrap();
    let server = Server::new(
        |manifest| manifest["version"] = json!("3.0.0"),
        b"test".to_vec(),
        false,
    )
    .await;
    let (_app, backend) = adapter(&server, &directory);
    let handle = updates::spawn(Box::new(backend));
    within(handle.check()).await.unwrap();
    within(handle.download()).await.unwrap();
    assert!(terminal(&handle, UpdatePhase::Failed).await.error.is_some());
    assert!(matches!(
        within(handle.install()).await,
        Err(UpdateError::NotDownloaded)
    ));
    within(handle.shutdown()).await.unwrap();
}

#[test]
fn signing_key_configuration_trims_and_requires_a_nonempty_value() {
    assert_eq!(normalized_public_key(None), None);
    assert_eq!(normalized_public_key(Some(" \t\r\n")), None);
    assert_eq!(
        normalized_public_key(Some("  public-key\n")),
        Some("public-key")
    );
}

#[tokio::test]
async fn missing_signing_key_disables_plugin_startup_and_commands_without_network() {
    for key in [None, Some(" \t\n")] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let directory = TempDir::new().unwrap();
        let mut context = mock_context(noop_assets());
        context.config_mut().plugins.0.insert(
            "updater".into(),
            json!({
                "pubkey": "",
                "endpoints": [format!("http://{address}/latest.json")],
                "dangerousInsecureTransportProtocol": true,
                "requireSignedVersion": true,
            }),
        );
        let app = register_with_key(mock_builder(), key)
            .build(context)
            .unwrap();
        assert!(
            !app.handle().remove_plugin("updater"),
            "the disabled plugin must never be registered"
        );
        let (core, _events) = app_core::spawn(directory.path()).await.unwrap();
        let before = core.settings().await.unwrap();
        assert!(before.auto_check_updates);
        assert!(before.last_update_check_unix_ms.is_none());
        let backend = backend_with_key(app.handle(), key);
        assert!(matches!(backend, Err(UpdateError::SigningNotConfigured)));
        crate::release_startup::start_updates(&core, backend)
            .await
            .unwrap();
        assert_eq!(within(core.automatic_update_check()).await, None);
        for rejected in [
            within(core.check_update()).await.map(|_| ()),
            within(core.download_update()).await.map(|_| ()),
            within(core.install_update()).await.map(|_| ()),
        ] {
            assert!(matches!(
                rejected,
                Err(app_core::AppError::Update(
                    UpdateError::SigningNotConfigured
                ))
            ));
        }
        assert_eq!(core.settings().await.unwrap(), before);
        assert!(!directory.path().join("settings.json").exists());
        within(core.shutdown()).await.unwrap();

        // All operations and actors have finished. A marker connection flushes
        // the listener's existing accept queue; inspecting through that marker
        // proves no HTTP request preceded it, without a short wait-for-silence.
        let mut marker = within(tokio::net::TcpStream::connect(address))
            .await
            .unwrap();
        marker
            .write_all(b"GET /test-fence HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();
        let (mut connection, _) = within(listener.accept()).await.unwrap();
        let request = within(async {
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                if connection.read_exact(&mut byte).await.is_err() {
                    break;
                }
                request.push(byte[0]);
                assert!(request.len() < 8192);
            }
            request
        })
        .await;
        assert_eq!(
            request,
            b"GET /test-fence HTTP/1.1\r\nHost: localhost\r\n\r\n"
        );
    }
}
