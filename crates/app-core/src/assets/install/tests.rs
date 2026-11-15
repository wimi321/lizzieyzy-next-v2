#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::assets::{
    AssetDownloadStatus, AssetError, AssetHandle, DownloadPhase, runtime_test_support,
    spawn_with_catalog,
};
use std::{future::Future, path::PathBuf, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::watch,
};

const PROFILE: &str = "cuda12.8-cudnn9";

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("runtime pause test timed out")
}

struct Fixture {
    directory: tempfile::TempDir,
    listener: TcpListener,
    handle: AssetHandle,
    events: watch::Receiver<AssetDownloadStatus>,
    runtime: Vec<u8>,
    cached_path: PathBuf,
    cached: Vec<u8>,
}

impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let listener = within(TcpListener::bind("127.0.0.1:0")).await.unwrap();
        let (mut catalog, archives) =
            runtime_test_support::test_runtime_catalog(b"fake engine, never executed");
        for spec in &mut catalog.specs {
            spec.url = format!("http://{}/{}", listener.local_addr().unwrap(), spec.id);
        }
        let cached_path = directory.path().join(&archives[0].0);
        std::fs::create_dir_all(cached_path.parent().unwrap()).unwrap();
        std::fs::write(&cached_path, &archives[0].1).unwrap();
        let (handle, events) = spawn_with_catalog(directory.path().into(), catalog);
        Self {
            directory,
            listener,
            handle,
            events,
            runtime: archives[1].1.clone(),
            cached_path,
            cached: archives[0].1.clone(),
        }
    }

    async fn request(&self) -> (TcpStream, String) {
        within(async {
            let (mut stream, _) = self.listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0; 1024];
            while !bytes.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                let count = stream.read(&mut buffer).await.unwrap();
                assert_ne!(count, 0);
                bytes.extend_from_slice(&buffer[..count]);
            }
            (stream, String::from_utf8(bytes).unwrap())
        })
        .await
    }

    async fn partial(&mut self) -> u64 {
        within(self.handle.install_runtime(PROFILE.into()))
            .await
            .unwrap();
        let (mut stream, request) = self.request().await;
        assert!(request.starts_with("GET /runtime-cuda_cudart "));
        assert!(!request.to_ascii_lowercase().contains("range:"));
        let prefix = self.runtime.len() / 2;
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.runtime.len()
        );
        within(stream.write_all(header.as_bytes())).await.unwrap();
        within(stream.write_all(&self.runtime[..prefix]))
            .await
            .unwrap();
        let cached_size = self.cached.len() as u64;
        self.status(|status| {
            status.downloaded_bytes > cached_size && status.downloaded_bytes < status.total_bytes
        })
        .await;
        let paused = within(self.handle.pause()).await.unwrap();
        assert_eq!(paused.phase, DownloadPhase::Paused);
        assert_eq!(paused.asset_id.as_deref(), Some(PROFILE));
        assert_eq!(paused.downloaded_bytes, (self.cached.len() + prefix) as u64);
        assert_eq!(within(self.handle.pause()).await.unwrap(), paused);
        assert!(matches!(
            within(self.handle.install_runtime("different-profile".into())).await,
            Err(AssetError::Busy)
        ));
        // pause has closed the HTTP response and drained all pending writes.
        drop(stream);
        paused.downloaded_bytes - self.cached.len() as u64
    }

    async fn status(
        &mut self,
        predicate: impl Fn(&AssetDownloadStatus) -> bool,
    ) -> AssetDownloadStatus {
        within(async {
            loop {
                let status = self.events.borrow_and_update().clone();
                assert_ne!(status.phase, DownloadPhase::Failed, "{status:?}");
                if predicate(&status) {
                    return status;
                }
                self.events.changed().await.unwrap();
            }
        })
        .await
    }

    async fn complete(&mut self, mut stream: TcpStream, offset: usize) {
        let header = if offset == 0 {
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                self.runtime.len()
            )
        } else {
            format!(
                "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n",
                self.runtime.len() - offset,
                offset,
                self.runtime.len() - 1,
                self.runtime.len()
            )
        };
        within(stream.write_all(header.as_bytes())).await.unwrap();
        within(stream.write_all(&self.runtime[offset..]))
            .await
            .unwrap();
        drop(stream);
        self.status(|status| status.phase == DownloadPhase::Completed)
            .await;
        assert!(
            within(self.handle.runtime_installation(PROFILE.into()))
                .await
                .unwrap()
                .ready
        );
        assert_eq!(std::fs::read(&self.cached_path).unwrap(), self.cached);
    }

    async fn close(self) {
        within(self.handle.shutdown()).await.unwrap();
        drop(self.listener);
        within(tokio::task::spawn_blocking(move || self.directory.close()))
            .await
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn runtime_pause_resume_retains_prefix_and_verified_archives() {
    for resume_by_asset in [false, true] {
        let mut fixture = Fixture::new().await;
        let offset = fixture.partial().await;
        let resumed = if resume_by_asset {
            within(fixture.handle.start("windows-nvidia".into())).await
        } else {
            within(fixture.handle.install_runtime(PROFILE.into())).await
        }
        .unwrap();
        assert_eq!(resumed.phase, DownloadPhase::Downloading);
        let (stream, request) = fixture.request().await;
        assert!(request.starts_with("GET /runtime-cuda_cudart "));
        assert!(
            request
                .to_ascii_lowercase()
                .contains(&format!("range: bytes={offset}-"))
        );
        fixture.complete(stream, offset as usize).await;
        fixture.close().await;
    }
}

#[tokio::test]
async fn cancelling_paused_runtime_cleans_prefix_but_keeps_verified_archives() {
    let mut fixture = Fixture::new().await;
    fixture.partial().await;
    let cancelled = within(fixture.handle.cancel()).await.unwrap();
    assert_eq!(cancelled.phase, DownloadPhase::Cancelled);
    assert_eq!(std::fs::read(&fixture.cached_path).unwrap(), fixture.cached);
    let archive_directory = fixture.cached_path.parent().unwrap();
    assert_eq!(std::fs::read_dir(archive_directory).unwrap().count(), 1);
    within(fixture.handle.install_runtime(PROFILE.into()))
        .await
        .unwrap();
    let (stream, request) = fixture.request().await;
    assert!(request.starts_with("GET /runtime-cuda_cudart "));
    assert!(!request.to_ascii_lowercase().contains("range:"));
    fixture.complete(stream, 0).await;
    fixture.close().await;
}

#[tokio::test]
async fn immediate_resume_then_pause_acknowledges_the_new_request() {
    let (events, status) = watch::channel(AssetDownloadStatus::default());
    let (_cancel, cancellation) = watch::channel(false);
    let (requested, mut acknowledged, pause) = super::pause::Pause::new(&events);
    requested.send_replace(true);
    let operation = pause.wait(&cancellation, Some(17));
    tokio::pin!(operation);
    within(async {
        tokio::select! {
            result = &mut operation => panic!("pause finished before resume: {result:?}"),
            result = acknowledged.changed() => result.unwrap(),
        }
    })
    .await;
    assert_eq!(status.borrow().phase, DownloadPhase::Paused);
    requested.send_replace(false);
    requested.send_replace(true);
    within(async {
        tokio::select! {
            result = &mut operation => panic!("second pause was lost: {result:?}"),
            result = acknowledged.changed() => result.unwrap(),
        }
    })
    .await;
    requested.send_replace(false);
    within(operation).await.unwrap();
}
