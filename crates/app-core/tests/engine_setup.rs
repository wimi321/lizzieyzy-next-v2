#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{AppConfig, AppEvent, EngineSetupProfile, EngineStatus, Settings};
use std::{future::Future, pin::Pin, task::Poll, time::Duration};
use tempfile::TempDir;

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("safety deadline")
}

async fn start(future: Pin<&mut impl Future<Output = Result<(), app_core::AppError>>>) {
    let mut future = future;
    std::future::poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}

fn config() -> AppConfig {
    let mut config = AppConfig::default();
    config.analysis.live.max_visits = 1;
    config.analysis.live.report_interval = Duration::from_secs(60);
    config.session.write_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(60);
    config
}

fn profile(directory: &std::path::Path) -> EngineSetupProfile {
    let model = directory.join("model.bin.gz");
    std::fs::write(&model, "fake model").unwrap();
    EngineSetupProfile {
        program: fake_katago::binary_path().unwrap(),
        model,
        config: None,
        extra_args: Vec::new(),
    }
}

#[tokio::test]
async fn default_configuration_is_copied_once_and_explicit_config_needs_no_template() {
    let directory = TempDir::new().unwrap();
    let template = directory.path().join("bundled.cfg");
    let settings_dir = directory.path().join("settings");
    std::fs::write(&template, "reportAnalysisWinratesAs = BLACK\n").unwrap();
    let candidate = profile(directory.path());
    let (core, _output) = within(app_core::spawn_with_config(&settings_dir, config()))
        .await
        .unwrap();
    within(core.try_configure_engine(candidate.clone(), template.clone()))
        .await
        .unwrap();
    let copied = settings_dir.join("analysis.cfg");
    assert_eq!(
        std::fs::read(&copied).unwrap(),
        std::fs::read(&template).unwrap()
    );
    assert_eq!(
        within(core.settings())
            .await
            .unwrap()
            .engine
            .unwrap()
            .config,
        copied
    );
    std::fs::write(&copied, "user changes\n").unwrap();
    std::fs::remove_file(&template).unwrap();
    within(core.try_configure_engine(candidate.clone(), template.clone()))
        .await
        .unwrap();
    assert_eq!(std::fs::read_to_string(&copied).unwrap(), "user changes\n");
    let explicit = directory.path().join("explicit.cfg");
    std::fs::write(&explicit, "explicit user file").unwrap();
    let candidate = EngineSetupProfile {
        config: Some(explicit.clone()),
        ..candidate
    };
    within(core.try_configure_engine(candidate, template))
        .await
        .unwrap();
    assert_eq!(
        within(core.settings())
            .await
            .unwrap()
            .engine
            .unwrap()
            .config,
        explicit
    );
    within(core.clone().shutdown()).await.unwrap();
}

#[tokio::test]
async fn pending_trial_keeps_commands_and_old_analysis_live_then_saves_after_ready() {
    let directory = TempDir::new().unwrap();
    let template = directory.path().join("bundled.cfg");
    std::fs::write(&template, "default").unwrap();
    let candidate = profile(directory.path());
    let (core, mut output) = within(app_core::spawn_with_config(directory.path(), config()))
        .await
        .unwrap();
    within(core.try_configure_engine(candidate.clone(), template.clone()))
        .await
        .unwrap();
    let before = std::fs::read(directory.path().join("settings.json")).unwrap();
    let gate = directory.path().join("gate");
    let candidate = EngineSetupProfile {
        extra_args: vec!["--startup-gate-file".into(), gate.to_str().unwrap().into()],
        ..candidate
    };
    let attempt = core.try_configure_engine(candidate.clone(), template);
    tokio::pin!(attempt);
    start(attempt.as_mut()).await;
    // Settings is queued after the attempt, so this is a command barrier, not
    // a timing guess that the candidate must already have started.
    let old = within(core.settings()).await.unwrap();
    assert_eq!(serde_json::from_slice::<Settings>(&before).unwrap(), old);
    let revision = within(core.play(go_core::Move::Pass(go_core::Color::Black)))
        .await
        .unwrap();
    within(async {
        loop {
            match output.recv().await.unwrap() {
                AppEvent::AnalysisUpdated(view)
                    if view.revision == revision && !view.is_during_search =>
                {
                    break;
                }
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("{reason}")
                }
                _ => {}
            }
        }
    })
    .await;
    assert_eq!(
        std::fs::read(directory.path().join("settings.json")).unwrap(),
        before
    );
    std::fs::write(gate, "release").unwrap();
    within(attempt).await.unwrap();
    assert_eq!(
        within(core.settings())
            .await
            .unwrap()
            .engine
            .unwrap()
            .extra_args,
        candidate.extra_args
    );
    within(core.clone().shutdown()).await.unwrap();
}

#[tokio::test]
async fn default_copy_error_and_invalid_timeout_do_not_save_a_profile() {
    let directory = TempDir::new().unwrap();
    let candidate = profile(directory.path());
    let missing = directory.path().join("missing.cfg");
    let (core, _output) = within(app_core::spawn_with_config(directory.path(), config()))
        .await
        .unwrap();
    let error = within(core.try_configure_engine(candidate.clone(), missing.clone()))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("default engine configuration"));
    assert!(within(core.settings()).await.unwrap().engine.is_none());
    assert!(!directory.path().join("settings.json").exists());
    assert!(!directory.path().join("analysis.cfg").exists());
    within(core.clone().shutdown()).await.unwrap();
    let mut invalid = config();
    invalid.engine_setup_timeout = Duration::ZERO;
    let (core, _output) = within(app_core::spawn_with_config(directory.path(), invalid))
        .await
        .unwrap();
    assert!(matches!(
        within(core.try_configure_engine(candidate, missing)).await,
        Err(app_core::AppError::EngineSetup(
            app_core::EngineSetupError::InvalidTimeout
        ))
    ));
    within(core.clone().shutdown()).await.unwrap();
}

#[tokio::test]
async fn replacing_or_shutting_down_pending_setup_cancels_the_command() {
    for shutdown in [false, true] {
        let directory = TempDir::new().unwrap();
        let template = directory.path().join("bundled.cfg");
        std::fs::write(&template, "default").unwrap();
        let marker = directory.path().join("hung");
        let candidate = EngineSetupProfile {
            extra_args: vec![
                "--hang-after-queries".into(),
                "0".into(),
                "--fault-once-file".into(),
                marker.to_str().unwrap().into(),
            ],
            ..profile(directory.path())
        };
        let mut config = config();
        // This test starts only a deliberately hung candidate. Its shutdown
        // deadline is the target failure cleanup, never a healthy-session timer.
        config.session.shutdown_timeout = Duration::from_millis(20);
        let (core, _output) = within(app_core::spawn_with_config(directory.path(), config))
            .await
            .unwrap();
        let attempt = core.try_configure_engine(candidate, template);
        tokio::pin!(attempt);
        start(attempt.as_mut()).await;
        within(core.settings()).await.unwrap();
        within(async {
            while !marker.is_file() {
                tokio::task::yield_now().await;
            }
        })
        .await;
        if shutdown {
            within(core.clone().shutdown()).await.unwrap();
        } else {
            within(core.configure_engine(None)).await.unwrap();
        }
        assert!(matches!(
            within(attempt).await,
            Err(app_core::AppError::EngineSetup(
                app_core::EngineSetupError::Cancelled
            ))
        ));
        if !shutdown {
            assert!(within(core.settings()).await.unwrap().engine.is_none());
            within(core.clone().shutdown()).await.unwrap();
        }
    }
}
