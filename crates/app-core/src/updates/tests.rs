#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

struct Backend {
    gate: Option<oneshot::Receiver<()>>,
    installed: mpsc::UnboundedSender<Vec<u8>>,
    hash: [u8; 32],
    destination: Option<std::path::PathBuf>,
}

impl UpdateBackend for Backend {
    fn check(&mut self) -> UpdateFuture<'_, Option<UpdateRelease>> {
        Box::pin(async move {
            Ok(Some(UpdateRelease {
                view: UpdateCheck {
                    available: true,
                    version: Some("2.0.0".into()),
                    ..Default::default()
                },
                sha256: self.hash,
            }))
        })
    }

    fn download(
        &mut self,
        progress: watch::Sender<UpdateDownloadStatus>,
    ) -> UpdateFuture<'_, Vec<u8>> {
        Box::pin(async move {
            progress.send_modify(|status| {
                status.downloaded_bytes = 2;
                status.total_bytes = Some(4);
            });
            if let Some(gate) = self.gate.take() {
                let _ = gate.await;
            }
            Ok(b"test".to_vec())
        })
    }

    fn install(&mut self, bytes: Vec<u8>) -> UpdateFuture<'_, ()> {
        Box::pin(async move {
            if let Some(path) = &self.destination {
                std::fs::write(path, &bytes).unwrap();
            }
            self.installed.send(bytes).map_err(|_| UpdateError::Closed)
        })
    }
}

async fn phase(handle: &UpdateHandle, wanted: UpdatePhase) -> UpdateDownloadStatus {
    let mut status = handle.subscribe_status();
    tokio::time::timeout(std::time::Duration::from_secs(5), async move {
        loop {
            let current = status.borrow_and_update().clone();
            if current.phase == wanted {
                return current;
            }
            status.changed().await.unwrap();
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn verified_bytes_only_reach_installer_and_are_consumed_once() {
    let (installed, mut received) = mpsc::unbounded_channel();
    let handle = spawn(Box::new(Backend {
        gate: None,
        installed,
        hash: Sha256::digest(b"test").into(),
        destination: None,
    }));
    assert!(matches!(
        handle.install().await,
        Err(UpdateError::NotDownloaded)
    ));
    assert!(handle.check().await.unwrap().available);
    handle.download().await.unwrap();
    assert_eq!(
        phase(&handle, UpdatePhase::Downloaded)
            .await
            .downloaded_bytes,
        4
    );
    assert_eq!(
        handle.install().await.unwrap().phase,
        UpdatePhase::Completed
    );
    assert_eq!(received.recv().await.unwrap(), b"test");
    assert!(matches!(
        handle.install().await,
        Err(UpdateError::NotDownloaded)
    ));
    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn wrong_sha256_cannot_reach_installer() {
    let (installed, mut received) = mpsc::unbounded_channel();
    let handle = spawn(Box::new(Backend {
        gate: None,
        installed,
        hash: [0; 32],
        destination: None,
    }));
    handle.check().await.unwrap();
    handle.download().await.unwrap();
    assert!(
        phase(&handle, UpdatePhase::Failed)
            .await
            .error
            .unwrap()
            .contains("SHA-256")
    );
    assert!(matches!(
        handle.install().await,
        Err(UpdateError::NotDownloaded)
    ));
    assert!(received.try_recv().is_err());
    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn cancellation_drops_transfer_before_reply_and_retry_starts_from_zero() {
    let (release, gate) = oneshot::channel();
    let (installed, mut received) = mpsc::unbounded_channel();
    let handle = spawn(Box::new(Backend {
        gate: Some(gate),
        installed,
        hash: Sha256::digest(b"test").into(),
        destination: None,
    }));
    handle.check().await.unwrap();
    handle.download().await.unwrap();
    let mut progress = handle.subscribe_status();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while progress.borrow_and_update().downloaded_bytes != 2 {
            progress.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
    assert!(matches!(handle.check().await, Err(UpdateError::Busy)));
    assert!(matches!(
        handle.check_automatically().await,
        Err(UpdateError::Busy)
    ));
    assert_eq!(handle.status().phase, UpdatePhase::Downloading);
    assert_eq!(handle.cancel().await.unwrap().phase, UpdatePhase::Cancelled);
    assert!(release.is_closed());
    assert!(matches!(
        handle.install().await,
        Err(UpdateError::NotDownloaded)
    ));
    assert!(received.try_recv().is_err());
    assert_eq!(handle.download().await.unwrap().downloaded_bytes, 0);
    phase(&handle, UpdatePhase::Downloaded).await;
    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn new_check_and_cancel_invalidate_previously_verified_package() {
    let (installed, _) = mpsc::unbounded_channel();
    let handle = spawn(Box::new(Backend {
        gate: None,
        installed,
        hash: Sha256::digest(b"test").into(),
        destination: None,
    }));
    handle.check().await.unwrap();
    handle.download().await.unwrap();
    phase(&handle, UpdatePhase::Downloaded).await;
    handle.check().await.unwrap();
    assert!(matches!(
        handle.install().await,
        Err(UpdateError::NotDownloaded)
    ));
    handle.download().await.unwrap();
    phase(&handle, UpdatePhase::Downloaded).await;
    handle.cancel().await.unwrap();
    assert!(matches!(
        handle.install().await,
        Err(UpdateError::NotDownloaded)
    ));
    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn automatic_failure_stays_idle_and_invalidates_old_bytes_while_manual_failure_is_reported() {
    struct ScriptedChecks {
        checks: std::collections::VecDeque<Result<Option<UpdateRelease>, UpdateError>>,
        transfer: Backend,
    }
    impl UpdateBackend for ScriptedChecks {
        fn check(&mut self) -> UpdateFuture<'_, Option<UpdateRelease>> {
            Box::pin(async move { self.checks.pop_front().expect("scripted check") })
        }
        fn download(
            &mut self,
            progress: watch::Sender<UpdateDownloadStatus>,
        ) -> UpdateFuture<'_, Vec<u8>> {
            self.transfer.download(progress)
        }
        fn install(&mut self, bytes: Vec<u8>) -> UpdateFuture<'_, ()> {
            self.transfer.install(bytes)
        }
    }
    let (installed, mut received) = mpsc::unbounded_channel();
    let hash = Sha256::digest(b"test").into();
    let handle = spawn(Box::new(ScriptedChecks {
        checks: [
            Ok(Some(UpdateRelease {
                view: UpdateCheck {
                    available: true,
                    version: Some("2.0.0".into()),
                    ..Default::default()
                },
                sha256: hash,
            })),
            Err(UpdateError::Backend("background failure".into())),
            Err(UpdateError::Backend("manual failure".into())),
        ]
        .into(),
        transfer: Backend {
            gate: None,
            installed,
            hash,
            destination: None,
        },
    }));
    handle.check().await.unwrap();
    handle.download().await.unwrap();
    phase(&handle, UpdatePhase::Downloaded).await;
    assert!(handle.check_automatically().await.is_err());
    assert_eq!(handle.status(), UpdateDownloadStatus::default());
    assert!(matches!(
        handle.install().await,
        Err(UpdateError::NotDownloaded)
    ));
    assert!(matches!(
        handle.download().await,
        Err(UpdateError::NotAvailable)
    ));
    assert!(received.try_recv().is_err());
    assert!(handle.check().await.is_err());
    assert_eq!(handle.status().phase, UpdatePhase::Failed);
    assert!(handle.status().error.unwrap().contains("manual failure"));
    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn successful_installer_boundary_preserves_settings_cache_and_game_records() {
    let directory = tempfile::TempDir::new().unwrap();
    let user_data = directory.path().join("user-data");
    std::fs::create_dir(&user_data).unwrap();
    for name in ["settings.json", "analysis-cache.sqlite3", "game.sgf"] {
        std::fs::write(user_data.join(name), format!("preserve {name}")).unwrap();
    }
    let executable = directory.path().join("application");
    std::fs::write(&executable, "old application").unwrap();
    let (installed, mut received) = mpsc::unbounded_channel();
    let handle = spawn(Box::new(Backend {
        gate: None,
        installed,
        hash: Sha256::digest(b"test").into(),
        destination: Some(executable.clone()),
    }));
    handle.check().await.unwrap();
    handle.download().await.unwrap();
    phase(&handle, UpdatePhase::Downloaded).await;
    handle.install().await.unwrap();
    assert_eq!(received.recv().await.unwrap(), b"test");
    assert_eq!(std::fs::read(executable).unwrap(), b"test");
    for name in ["settings.json", "analysis-cache.sqlite3", "game.sgf"] {
        assert_eq!(
            std::fs::read_to_string(user_data.join(name)).unwrap(),
            format!("preserve {name}")
        );
    }
    assert_eq!(std::fs::read_dir(user_data).unwrap().count(), 3);
    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn shutdown_cancels_pending_check_without_waiting_for_its_network_deadline() {
    struct PendingCheck {
        entered: Option<oneshot::Sender<()>>,
        release: oneshot::Receiver<()>,
    }
    impl UpdateBackend for PendingCheck {
        fn check(&mut self) -> UpdateFuture<'_, Option<UpdateRelease>> {
            Box::pin(async move {
                if let Some(entered) = self.entered.take() {
                    let _ = entered.send(());
                }
                let _ = (&mut self.release).await;
                Ok(None)
            })
        }
        fn download(
            &mut self,
            _: watch::Sender<UpdateDownloadStatus>,
        ) -> UpdateFuture<'_, Vec<u8>> {
            panic!("unexpected download")
        }
        fn install(&mut self, _: Vec<u8>) -> UpdateFuture<'_, ()> {
            panic!("unexpected install")
        }
    }
    let (entered, observed) = oneshot::channel();
    let (release, waiting) = oneshot::channel();
    let handle = spawn(Box::new(PendingCheck {
        entered: Some(entered),
        release: waiting,
    }));
    let checking = tokio::spawn({
        let handle = handle.clone();
        async move { handle.check().await }
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), observed)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(handle.download().await, Err(UpdateError::Busy)));
    assert!(matches!(
        handle.check_automatically().await,
        Err(UpdateError::Busy)
    ));
    tokio::time::timeout(std::time::Duration::from_secs(5), handle.shutdown())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(checking.await.unwrap(), Err(UpdateError::Closed)));
    assert!(release.is_closed());
}
