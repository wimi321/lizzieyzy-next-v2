use std::time::Duration;

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
};

use super::*;

const BODY: &[u8] = b"tiny pinned test model, never a real weight";

struct Response {
    body: Vec<u8>,
    status: u16,
    content_range: Option<String>,
    gate: Option<oneshot::Receiver<()>>,
}
impl Response {
    fn good() -> Self {
        Self {
            body: BODY.to_vec(),
            status: 200,
            content_range: None,
            gate: None,
        }
    }
}
struct Server {
    url: String,
    responses: mpsc::Sender<Response>,
    requests: mpsc::Receiver<String>,
    task: JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Server {
    async fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/model", listener.local_addr().unwrap());
        let (response_tx, mut responses) = mpsc::channel::<Response>(4);
        let (request_tx, requests) = mpsc::channel(4);
        let task = tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut request = Vec::new();
                let mut buffer = [0; 1024];
                while !request.windows(4).any(|end| end == b"\r\n\r\n") {
                    let size = stream.read(&mut buffer).await.unwrap();
                    if size == 0 {
                        return;
                    }
                    request.extend_from_slice(&buffer[..size]);
                }
                if request_tx
                    .send(String::from_utf8(request).unwrap())
                    .await
                    .is_err()
                {
                    return;
                }
                let Some(response) = responses.recv().await else {
                    return;
                };
                let header = format!(
                    "HTTP/1.1 {} Test\r\nContent-Length: {}\r\n{}Connection: close\r\n\r\n",
                    response.status,
                    response.body.len(),
                    response
                        .content_range
                        .map(|range| format!("Content-Range: {range}\r\n"))
                        .unwrap_or_default()
                );
                if stream.write_all(header.as_bytes()).await.is_err() {
                    continue;
                }
                let split = response.body.len() / 2;
                if stream.write_all(&response.body[..split]).await.is_err() {
                    continue;
                }
                if let Some(gate) = response.gate {
                    let _ = gate.await;
                }
                let _ = stream.write_all(&response.body[split..]).await;
            }
        });
        Self {
            url,
            responses: response_tx,
            requests,
            task,
        }
    }
    async fn requested(&mut self) -> String {
        tokio::time::timeout(Duration::from_secs(5), self.requests.recv())
            .await
            .unwrap()
            .unwrap()
    }
}
async fn phase(
    events: &mut watch::Receiver<AssetDownloadStatus>,
    expected: DownloadPhase,
) -> AssetDownloadStatus {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let status = events.borrow_and_update().clone();
            if status.phase == expected {
                return status;
            }
            events.changed().await.unwrap();
        }
    })
    .await
    .unwrap()
}
async fn progress(events: &mut watch::Receiver<AssetDownloadStatus>) -> AssetDownloadStatus {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let status = events.borrow_and_update().clone();
            if status.downloaded_bytes > 0 {
                return status;
            }
            events.changed().await.unwrap();
        }
    })
    .await
    .unwrap()
}
fn setup(
    directory: &tempfile::TempDir,
    url: &str,
) -> (AssetHandle, watch::Receiver<AssetDownloadStatus>) {
    spawn_with_catalog(
        directory.path().to_path_buf(),
        AssetCatalog::test_weight("small", url, BODY),
    )
}
fn files(directory: &tempfile::TempDir) -> Vec<String> {
    std::fs::read_dir(directory.path().join("weights"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

#[tokio::test]
async fn progress_verified_publication_and_repeated_download_use_one_request() {
    let directory = tempfile::tempdir().unwrap();
    let mut server = Server::new().await;
    let (release, gate) = oneshot::channel();
    server
        .responses
        .send(Response {
            gate: Some(gate),
            ..Response::good()
        })
        .await
        .unwrap();
    let (handle, mut events) = setup(&directory, &server.url);
    assert!(!handle.list_weights().await.unwrap()[0].downloaded);
    assert_eq!(
        handle.start("small".into()).await.unwrap().phase,
        DownloadPhase::Downloading
    );
    server.requested().await;
    let partial = progress(&mut events).await;
    assert_eq!(partial.total_bytes, BODY.len() as u64);
    assert!(partial.downloaded_bytes < partial.total_bytes);
    assert!(!directory.path().join("weights/small.bin.gz").exists());
    assert_eq!(
        handle.start("small".into()).await.unwrap().phase,
        DownloadPhase::Downloading
    );
    release.send(()).unwrap();
    let status = phase(&mut events, DownloadPhase::Completed).await;
    assert_eq!(status.downloaded_bytes, BODY.len() as u64);
    let weight = handle.list_weights().await.unwrap().remove(0);
    assert!(weight.downloaded);
    assert_eq!(weight.published_at.as_deref(), Some("2026-01-01"));
    assert_eq!(
        std::fs::read(handle.verified_weight("small".into()).await.unwrap()).unwrap(),
        BODY
    );
    assert_eq!(files(&directory), ["small.bin.gz"]);
    // Close the server. A repeated valid download must not use the network.
    server.task.abort();
    handle.start("small".into()).await.unwrap();
    phase(&mut events, DownloadPhase::Completed).await;
    assert_eq!(files(&directory), ["small.bin.gz"]);
    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn cancel_cleans_temporary_file_before_reply_and_retry_starts_from_zero() {
    let directory = tempfile::tempdir().unwrap();
    let mut server = Server::new().await;
    let (release, gate) = oneshot::channel();
    server
        .responses
        .send(Response {
            gate: Some(gate),
            ..Response::good()
        })
        .await
        .unwrap();
    let (handle, mut events) = setup(&directory, &server.url);
    handle.start("small".into()).await.unwrap();
    server.requested().await;
    progress(&mut events).await;
    assert_eq!(
        handle.cancel().await.unwrap().phase,
        DownloadPhase::Cancelled
    );
    assert!(files(&directory).is_empty());
    assert!(matches!(
        handle.verified_weight("small".into()).await,
        Err(AssetError::NotDownloaded)
    ));
    release.send(()).unwrap();
    server.responses.send(Response::good()).await.unwrap();
    assert_eq!(
        handle.start("small".into()).await.unwrap().downloaded_bytes,
        0
    );
    phase(&mut events, DownloadPhase::Completed).await;
    assert_eq!(files(&directory), ["small.bin.gz"]);
    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn wrong_checksum_is_removed_and_failure_can_retry() {
    let directory = tempfile::tempdir().unwrap();
    let mut server = Server::new().await;
    let mut bad = Response::good();
    bad.body[0] ^= 1;
    server.responses.send(bad).await.unwrap();
    let (handle, mut events) = setup(&directory, &server.url);
    handle.start("small".into()).await.unwrap();
    server.requested().await;
    let failed = phase(&mut events, DownloadPhase::Failed).await;
    assert!(failed.error.unwrap().contains("SHA-256"));
    assert!(files(&directory).is_empty());
    assert!(!handle.list_weights().await.unwrap()[0].downloaded);
    server.responses.send(Response::good()).await.unwrap();
    handle.start("small".into()).await.unwrap();
    phase(&mut events, DownloadPhase::Completed).await;
    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn wrong_size_and_http_failure_leave_no_published_or_temporary_file() {
    for response in [
        Response {
            body: vec![0; 2],
            ..Response::good()
        },
        Response {
            status: 503,
            ..Response::good()
        },
    ] {
        let directory = tempfile::tempdir().unwrap();
        let server = Server::new().await;
        server.responses.send(response).await.unwrap();
        let (handle, mut events) = setup(&directory, &server.url);
        handle.start("small".into()).await.unwrap();
        phase(&mut events, DownloadPhase::Failed).await;
        assert!(files(&directory).is_empty());
        handle.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn existing_corrupt_weight_is_reverified_and_redownloaded() {
    let directory = tempfile::tempdir().unwrap();
    let server = Server::new().await;
    server.responses.send(Response::good()).await.unwrap();
    std::fs::create_dir(directory.path().join("weights")).unwrap();
    std::fs::write(
        directory.path().join("weights/small.bin.gz"),
        vec![0; BODY.len()],
    )
    .unwrap();
    let (handle, mut events) = setup(&directory, &server.url);
    assert!(!handle.list_weights().await.unwrap()[0].downloaded);
    handle.start("small".into()).await.unwrap();
    phase(&mut events, DownloadPhase::Completed).await;
    assert_eq!(
        std::fs::read(directory.path().join("weights/small.bin.gz")).unwrap(),
        BODY
    );
    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn shutdown_cleans_active_download_and_closes_all_handles() {
    let directory = tempfile::tempdir().unwrap();
    let mut server = Server::new().await;
    let (_release, gate) = oneshot::channel();
    server
        .responses
        .send(Response {
            gate: Some(gate),
            ..Response::good()
        })
        .await
        .unwrap();
    let (handle, mut events) = setup(&directory, &server.url);
    let other = handle.clone();
    handle.start("small".into()).await.unwrap();
    server.requested().await;
    progress(&mut events).await;
    handle.shutdown().await.unwrap();
    assert!(files(&directory).is_empty());
    assert!(matches!(
        other.start("small".into()).await,
        Err(AssetError::Closed)
    ));
    assert!(matches!(
        other.list_weights().await,
        Err(AssetError::Closed)
    ));
}

#[tokio::test]
async fn only_catalog_ids_are_accepted_and_different_active_asset_is_busy() {
    let directory = tempfile::tempdir().unwrap();
    let mut server = Server::new().await;
    let (_release, gate) = oneshot::channel();
    server
        .responses
        .send(Response {
            gate: Some(gate),
            ..Response::good()
        })
        .await
        .unwrap();
    let mut catalog = AssetCatalog::test_weight("small", &server.url, BODY);
    catalog
        .specs
        .extend(AssetCatalog::test_weight("second", &server.url, BODY).specs);
    let (handle, mut events) = spawn_with_catalog(directory.path().to_path_buf(), catalog);
    assert!(matches!(
        handle.start(server.url.clone()).await,
        Err(AssetError::UnknownAsset(_))
    ));
    handle.start("small".into()).await.unwrap();
    server.requested().await;
    progress(&mut events).await;
    assert!(matches!(
        handle.start("second".into()).await,
        Err(AssetError::Busy)
    ));
    handle.shutdown().await.unwrap();
}

#[test]
fn trusted_catalog_lists_pinned_models_and_runtime_packages() {
    let catalog = AssetCatalog::bundled().unwrap();
    assert_eq!(
        catalog
            .specs
            .iter()
            .filter(|spec| spec.weight && !spec.human_style)
            .count(),
        3
    );
    assert_eq!(catalog.specs[0].id, "b11-flagship");
    assert!(
        catalog
            .get("b10-balanced")
            .unwrap()
            .url
            .starts_with("https://github.com/lightvector/KataGo/releases/download/v1.17.1/")
    );
    assert!(
        catalog
            .get("b11-flagship")
            .unwrap()
            .url
            .starts_with("https://media.katagotraining.org/")
    );
    assert_eq!(
        catalog.get("runtime-cuda_nvrtc").unwrap().sha256,
        "e43603b09f8a52d681ceb814c00b655af19da53692ab91671dabbf8071c8f93d"
    );
    assert_eq!(
        catalog
            .get("windows-nvidia")
            .unwrap()
            .runtime_profile
            .as_deref(),
        Some("cuda12.8-cudnn9")
    );
}

#[test]
fn acceleration_selection_matches_java_platform_and_nvidia_capability_rules() {
    let catalog = AssetCatalog::bundled().unwrap();
    for platform in ["linux-x64", "macos-arm64", "macos-amd64", "windows-arm64"] {
        let plan = acceleration::select(platform, true, Some(86), &catalog).unwrap();
        assert_eq!(plan.reason, AccelerationReason::UnsupportedPlatform);
        assert!(plan.recommended_assets.is_empty());
    }
    let no_gpu = acceleration::select("windows-x64", false, None, &catalog).unwrap();
    assert_eq!(no_gpu.reason, AccelerationReason::NoNvidiaGpu);
    assert!(no_gpu.recommended_assets.is_empty());
    for (rank, supported, recommended) in [
        (Some(61), false, false),
        (Some(75), true, true),
        (Some(86), true, true),
        (Some(89), true, false),
        (Some(120), true, false),
        (None, true, false),
    ] {
        let plan = acceleration::select("windows-x64", true, rank, &catalog).unwrap();
        assert_eq!(plan.reason, AccelerationReason::NvidiaDetected);
        assert_eq!(plan.recommended_assets[0], "windows-nvidia");
        assert_eq!(plan.recommended_assets.len(), 7);
        assert_eq!(plan.tensor_rt_supported, supported);
        assert_eq!(plan.tensor_rt_recommended, recommended);
        assert_eq!(plan.optional_assets.len(), if supported { 2 } else { 0 });
        assert!(!plan.archives_only);
    }
    assert_eq!(
        acceleration::parse_gpu_output("NVIDIA RTX 3080, 8.6\nNVIDIA RTX 5090, 12.0\n"),
        (true, Some(120))
    );
    assert_eq!(
        acceleration::parse_gpu_output("NVIDIA GPU, N/A\n"),
        (true, None)
    );
    assert_eq!(acceleration::parse_gpu_output(""), (false, None));
}

#[tokio::test]
async fn existing_directory_is_not_replaced_or_removed() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("weights/small.bin.gz");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("keep"), b"preserve").unwrap();
    let (handle, mut events) = setup(&directory, "http://127.0.0.1:1/unused");
    handle.start("small".into()).await.unwrap();
    phase(&mut events, DownloadPhase::Failed).await;
    assert_eq!(std::fs::read(target.join("keep")).unwrap(), b"preserve");
    assert_eq!(files(&directory), ["small.bin.gz"]);
    handle.shutdown().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn managed_directory_and_target_symlinks_are_rejected() {
    for parent_link in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let keep = outside.path().join("keep");
        std::fs::write(&keep, BODY).unwrap();
        let weights = directory.path().join("weights");
        if parent_link {
            std::os::unix::fs::symlink(outside.path(), &weights).unwrap();
        } else {
            std::fs::create_dir(&weights).unwrap();
            std::os::unix::fs::symlink(&keep, weights.join("small.bin.gz")).unwrap();
        }
        let (handle, mut events) = setup(&directory, "http://127.0.0.1:1/unused");
        handle.start("small".into()).await.unwrap();
        phase(&mut events, DownloadPhase::Failed).await;
        assert_eq!(std::fs::read(&keep).unwrap(), BODY);
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 1);
        handle.shutdown().await.unwrap();
    }
}

#[cfg(unix)]
#[tokio::test]
async fn cleanup_failure_is_reported_by_cancel_and_shutdown() {
    for shutdown in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let mut server = Server::new().await;
        let (_release, gate) = oneshot::channel();
        server
            .responses
            .send(Response {
                gate: Some(gate),
                ..Response::good()
            })
            .await
            .unwrap();
        let (handle, mut events) = setup(&directory, &server.url);
        handle.start("small".into()).await.unwrap();
        server.requested().await;
        progress(&mut events).await;
        let temporary = std::fs::read_dir(directory.path().join("weights"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        // Deterministic I/O fault: replace the temporary pathname with a
        // directory while the actor still owns the original open file.
        std::fs::rename(&temporary, directory.path().join("moved-part")).unwrap();
        std::fs::create_dir(&temporary).unwrap();
        if shutdown {
            assert!(matches!(handle.shutdown().await, Err(AssetError::Io(_))));
        } else {
            assert!(matches!(handle.cancel().await, Err(AssetError::Io(_))));
            handle.shutdown().await.unwrap();
        }
        assert_eq!(handle.status().phase, DownloadPhase::Failed);
        assert!(handle.status().error.is_some());
    }
}

#[test]
fn legacy_gpu_query_falls_back_to_java_model_capabilities() {
    for (name, rank) in [
        ("NVIDIA GeForce GTX 1080", Some(61)),
        ("NVIDIA GTX1660", Some(75)),
        ("NVIDIA RTX 2080 Ti", Some(75)),
        ("NVIDIA RTX3080", Some(86)),
        ("NVIDIA RTX 4090", Some(89)),
        ("NVIDIA RTX5090", Some(120)),
        ("NVIDIA RTX PRO 6000", Some(120)),
        ("NVIDIA A100", Some(80)),
        ("NVIDIA T4", Some(75)),
        ("NVIDIA unknown", None),
    ] {
        assert_eq!(acceleration::parse_gpu_names(name), (true, rank));
        assert_eq!(
            acceleration::parse_gpu_output(&format!("{name}, N/A")),
            (true, rank)
        );
    }
    assert_eq!(acceleration::parse_gpu_names(""), (false, None));
    assert_eq!(
        acceleration::parse_gpu_names("NVIDIA GTX 1080\nNVIDIA RTX 3080"),
        (true, Some(86))
    );
}

#[tokio::test]
async fn human_model_status_import_and_main_model_list_are_separate() {
    let directory = tempfile::tempdir().unwrap();
    let catalog = AssetCatalog::test_weight("human-sl", "http://127.0.0.1:1/unused", BODY);
    let (handle, _) = spawn_with_catalog(directory.path().into(), catalog);
    assert_eq!(
        handle.humansl_status().await.unwrap(),
        HumanSlModelStatus::Missing
    );
    assert!(handle.list_weights().await.unwrap().is_empty());
    assert!(matches!(
        handle.verified_weight("human-sl".into()).await,
        Err(AssetError::UnknownAsset(_))
    ));
    let source = directory.path().join("import.bin.gz");
    std::fs::write(&source, vec![0; BODY.len()]).unwrap();
    assert!(matches!(
        handle.import_humansl(source.clone()).await,
        Err(AssetError::Verification)
    ));
    assert_eq!(
        handle.humansl_status().await.unwrap(),
        HumanSlModelStatus::Missing
    );
    std::fs::write(&source, BODY).unwrap();
    let ready = handle.import_humansl(source.clone()).await.unwrap();
    let HumanSlModelStatus::Ready { path } = ready else {
        panic!("expected ready")
    };
    assert_eq!(std::fs::read(&path).unwrap(), BODY);
    assert_eq!(handle.verified_humansl_at(None).await.unwrap(), path);
    assert_eq!(
        handle
            .verified_humansl_at(Some(source))
            .await
            .unwrap()
            .file_name()
            .unwrap(),
        "import.bin.gz"
    );
    assert_eq!(
        handle.humansl_status().await.unwrap(),
        HumanSlModelStatus::Ready { path: path.clone() }
    );
    std::fs::write(&path, vec![0; BODY.len()]).unwrap();
    assert_eq!(
        handle.humansl_status().await.unwrap(),
        HumanSlModelStatus::Corrupt
    );
    assert!(matches!(
        handle.verified_humansl_at(None).await,
        Err(AssetError::NotDownloaded)
    ));
    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn human_model_pause_resume_uses_verified_range_and_reports_four_state_progress() {
    let directory = tempfile::tempdir().unwrap();
    let mut server = Server::new().await;
    let (release, gate) = oneshot::channel();
    server
        .responses
        .send(Response {
            gate: Some(gate),
            ..Response::good()
        })
        .await
        .unwrap();
    let catalog = AssetCatalog::test_weight("human-sl", &server.url, BODY);
    let (handle, mut events) = spawn_with_catalog(directory.path().into(), catalog);
    handle.start("human-sl".into()).await.unwrap();
    server.requested().await;
    let partial = progress(&mut events).await;
    assert!(matches!(
        handle.humansl_status().await.unwrap(),
        HumanSlModelStatus::Downloading { paused: false, .. }
    ));
    assert_eq!(handle.pause().await.unwrap().phase, DownloadPhase::Paused);
    assert!(matches!(
        handle.humansl_status().await.unwrap(),
        HumanSlModelStatus::Downloading { paused: true, .. }
    ));
    release.send(()).unwrap();
    let received = partial.downloaded_bytes as usize;
    server
        .responses
        .send(Response {
            body: BODY[received..].to_vec(),
            status: 206,
            content_range: Some(format!(
                "bytes {}-{}/{}",
                received,
                BODY.len() - 1,
                BODY.len()
            )),
            gate: None,
        })
        .await
        .unwrap();
    handle.start("human-sl".into()).await.unwrap();
    let request = server.requested().await;
    assert!(
        request
            .to_ascii_lowercase()
            .contains(&format!("range: bytes={received}-"))
    );
    phase(&mut events, DownloadPhase::Completed).await;
    assert!(matches!(
        handle.humansl_status().await.unwrap(),
        HumanSlModelStatus::Ready { .. }
    ));
    assert_eq!(
        std::fs::read(handle.verified_humansl_at(None).await.unwrap()).unwrap(),
        BODY
    );
    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn resumed_server_ignoring_range_restarts_and_cancelled_pause_cleans_up() {
    for cancel in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let mut server = Server::new().await;
        let (release, gate) = oneshot::channel();
        server
            .responses
            .send(Response {
                gate: Some(gate),
                ..Response::good()
            })
            .await
            .unwrap();
        let (handle, mut events) = setup(&directory, &server.url);
        handle.start("small".into()).await.unwrap();
        server.requested().await;
        progress(&mut events).await;
        handle.pause().await.unwrap();
        release.send(()).unwrap();
        if cancel {
            handle.cancel().await.unwrap();
            assert!(files(&directory).is_empty());
        } else {
            server.responses.send(Response::good()).await.unwrap();
            handle.start("small".into()).await.unwrap();
            phase(&mut events, DownloadPhase::Completed).await;
            assert_eq!(
                std::fs::read(handle.verified_weight("small".into()).await.unwrap()).unwrap(),
                BODY
            );
        }
        handle.shutdown().await.unwrap();
    }
}

#[test]
fn human_catalog_pin_has_explicit_separate_purpose() {
    let catalog = AssetCatalog::bundled().unwrap();
    let spec = catalog.get("human-sl").unwrap();
    assert!(spec.human_style && spec.weight);
    assert_eq!(spec.file_name, "b18c384nbt-humanv0.bin.gz");
    assert_eq!(spec.size, 99_066_230);
    assert_eq!(
        spec.sha256,
        "637746e44f0efe00ad1245a50aa9bbf0716efe364c43965ead97bd6835d84ab5"
    );
    assert_eq!(
        spec.url,
        "https://media.katagotraining.org/uploaded/networks/models_extra/b18c384nbt-humanv0.bin.gz"
    );
}

#[tokio::test]
async fn resumed_bad_range_or_checksum_never_publishes_a_weight() {
    for bad_range in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let mut server = Server::new().await;
        let (release, gate) = oneshot::channel();
        server
            .responses
            .send(Response {
                gate: Some(gate),
                ..Response::good()
            })
            .await
            .unwrap();
        let (handle, mut events) = setup(&directory, &server.url);
        handle.start("small".into()).await.unwrap();
        server.requested().await;
        progress(&mut events).await;
        let paused = handle.pause().await.unwrap();
        let received = paused.downloaded_bytes as usize;
        release.send(()).unwrap();
        let mut body = BODY[received..].to_vec();
        if !bad_range {
            body[0] ^= 1;
        }
        server
            .responses
            .send(Response {
                body,
                status: 206,
                content_range: Some(format!(
                    "bytes {}-{}/{}",
                    if bad_range { received + 1 } else { received },
                    BODY.len() - 1,
                    BODY.len()
                )),
                gate: None,
            })
            .await
            .unwrap();
        handle.start("small".into()).await.unwrap();
        phase(&mut events, DownloadPhase::Failed).await;
        assert!(files(&directory).is_empty());
        handle.shutdown().await.unwrap();
    }
}
