#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{future::Future, time::Duration};

use app_core::updates::{
    self, UpdateBackend, UpdateCheck, UpdateDownloadStatus, UpdateError, UpdateFuture, UpdatePhase,
    UpdateRelease,
};
use app_core::{
    AppConfig, AppCore, AppError, AppEvent, AppSubscription, BenchmarkPhase, EngineProfile,
    UiLanguage,
};
use sha2::{Digest, Sha256};
use tokio::sync::{oneshot, watch};

struct FailingInstaller {
    entered: Option<oneshot::Sender<()>>,
    finish: Option<oneshot::Receiver<()>>,
}

impl UpdateBackend for FailingInstaller {
    fn check(&mut self) -> UpdateFuture<'_, Option<UpdateRelease>> {
        Box::pin(async {
            Ok(Some(UpdateRelease {
                view: UpdateCheck {
                    available: true,
                    version: Some("2.0.0".into()),
                    ..Default::default()
                },
                sha256: Sha256::digest(b"test").into(),
            }))
        })
    }

    fn download(&mut self, _: watch::Sender<UpdateDownloadStatus>) -> UpdateFuture<'_, Vec<u8>> {
        Box::pin(async { Ok(b"test".to_vec()) })
    }

    fn install(&mut self, _: Vec<u8>) -> UpdateFuture<'_, ()> {
        Box::pin(async move {
            if let Some(entered) = self.entered.take() {
                let _ = entered.send(());
            }
            if let Some(finish) = self.finish.take() {
                let _ = finish.await;
            }
            Err(UpdateError::Backend("scripted installer failure".into()))
        })
    }
}

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("update lifecycle safety deadline")
}

async fn ready_update(core: &AppCore) -> (oneshot::Receiver<()>, oneshot::Sender<()>) {
    let (entered, observed) = oneshot::channel();
    let (finish, waiting) = oneshot::channel();
    let handle = updates::spawn(Box::new(FailingInstaller {
        entered: Some(entered),
        finish: Some(waiting),
    }));
    let mut progress = handle.subscribe_status();
    core.attach_updates(handle).await.unwrap();
    core.check_update().await.unwrap();
    core.download_update().await.unwrap();
    within(async {
        while progress.borrow_and_update().phase != UpdatePhase::Downloaded {
            progress.changed().await.unwrap();
        }
    })
    .await;
    (observed, finish)
}

fn busy<T>(result: Result<T, AppError>) {
    assert!(matches!(result, Err(AppError::Update(UpdateError::Busy))));
}

async fn live(events: &mut AppSubscription) {
    within(async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::AnalysisUpdated(view)
                    if view.source == app_core::AnalysisSource::Live && view.visits > 0 =>
                {
                    return;
                }
                AppEvent::EngineStatusChanged(app_core::EngineStatus::Failed {
                    reason, ..
                }) => panic!("{reason}"),
                _ => {}
            }
        }
    })
    .await;
}

#[tokio::test]
async fn install_excludes_concurrent_mutations_and_failure_restores_live_analysis() {
    let directory = tempfile::TempDir::new().unwrap();
    // Binary construction and fixture writes happen before asynchronous safety
    // deadlines. The benchmark is held by a protocol gate, never a short timer.
    let program = fake_katago::binary_path().unwrap();
    let model = directory.path().join("model.bin");
    let config_path = directory.path().join("analysis.cfg");
    std::fs::write(&model, "fake model").unwrap();
    std::fs::write(&config_path, "reportAnalysisWinratesAs=BLACK").unwrap();
    let profile = EngineProfile {
        human_model: None,
        program,
        model,
        config: config_path,
        extra_args: vec![
            "--benchmark-gate-file".into(),
            directory
                .path()
                .join("benchmark-gate")
                .to_string_lossy()
                .into_owned(),
        ],
        canonical_model_name: None,
    };
    let mut config = AppConfig::default();
    config.analysis.live.max_visits = 1;
    config.session.startup_timeout = Duration::from_secs(60);
    config.session.response_timeout = Duration::from_secs(60);
    config.session.write_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(60);
    let (core, mut events) = app_core::spawn_with_config(directory.path(), config)
        .await
        .unwrap();
    core.set_cache_enabled(false).await.unwrap();
    core.configure_engine(Some(profile)).await.unwrap();
    live(&mut events).await;
    core.asset_download_status().await.unwrap();
    assert_eq!(
        core.start_benchmark().await.unwrap().phase,
        BenchmarkPhase::Running
    );
    let original = std::fs::read(directory.path().join("settings.json")).unwrap();
    let (entered, finish) = ready_update(&core).await;
    let installing = tokio::spawn({
        let core = core.clone();
        async move { core.install_update().await }
    });
    within(entered).await.unwrap();
    assert_eq!(
        core.update_download_status().await.unwrap().phase,
        UpdatePhase::Installing
    );
    assert_eq!(
        core.benchmark_status().await.unwrap().phase,
        BenchmarkPhase::Cancelled
    );
    busy(within(core.install_update()).await);
    busy(core.configure_engine(None).await);
    busy(core.start_benchmark().await);
    busy(
        core.start_board_sync(directory.path().join("absent-helper"), Default::default())
            .await,
    );
    busy(core.set_auto_check_updates(false).await);
    busy(core.cancel_update_download().await);
    finish.send(()).unwrap();
    assert!(within(installing).await.unwrap().is_err());
    live(&mut events).await;
    assert_eq!(
        std::fs::read(directory.path().join("settings.json")).unwrap(),
        original
    );
    core.cancel_asset_download().await.unwrap();
    core.set_language(UiLanguage::En).await.unwrap();
    assert_eq!(
        core.start_benchmark().await.unwrap().phase,
        BenchmarkPhase::Running
    );
    core.cancel_benchmark().await.unwrap();
    core.configure_engine(None).await.unwrap();
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn failed_install_recovers_even_when_original_caller_disappears() {
    let directory = tempfile::TempDir::new().unwrap();
    let (core, mut events) = app_core::spawn(directory.path()).await.unwrap();
    let (entered, finish) = ready_update(&core).await;
    let installing = tokio::spawn({
        let core = core.clone();
        async move { core.install_update().await }
    });
    within(entered).await.unwrap();
    installing.abort();
    let _ = installing.await;
    finish.send(()).unwrap();
    within(async {
        loop {
            if let AppEvent::UpdateDownloadProgress(progress) = events.recv().await.unwrap()
                && progress.phase == UpdatePhase::Failed
            {
                break;
            }
        }
    })
    .await;
    core.set_language(UiLanguage::En).await.unwrap();
    core.configure_engine(None).await.unwrap();
    core.cancel_asset_download().await.unwrap();
    core.shutdown().await.unwrap();
}
