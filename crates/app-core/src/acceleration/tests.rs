#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use crate::{AppEvent, EngineProfile};
use serde_json::json;
use std::{future::Future, time::Duration};
use support::{Fixture, analysis};

mod support;

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("safety deadline")
}

#[tokio::test]
async fn cpu_enables_cuda_preserves_profile_identity_persists_and_disables_to_cpu() {
    let mut fixture = Fixture::new(
        json!({"backend": "cuda", "gate": true}),
        Duration::from_secs(3),
    )
    .await;
    let operation = fixture.enabling();
    fixture.phase(AccelerationPhase::Downloading).await;
    fixture.phase(AccelerationPhase::Starting).await;
    std::fs::write(&fixture.gate, "").unwrap();
    let settings = within(operation).await.unwrap().unwrap();
    assert_eq!(settings.main_engine_id, fixture.original.main_engine_id);
    assert_eq!(settings.engines.len(), fixture.original.engines.len());
    assert_eq!(settings.engines[0].name, "KataGo CUDA");
    let original = fixture.original.engine.as_ref().unwrap().clone();
    let changed = settings.engine.as_ref().unwrap();
    assert_eq!(changed.model, original.model);
    assert_eq!(changed.config, original.config);
    assert_eq!(changed.extra_args, original.extra_args);
    assert_eq!(changed.canonical_model_name, original.canonical_model_name);
    assert_eq!(changed.human_model, original.human_model);
    assert!(
        changed.program.starts_with(
            fixture
                .directory
                .path()
                .join("data/runtimes/cuda12.8-cudnn9")
        )
    );
    assert_eq!(changed.program.file_name().unwrap(), "katago.exe");
    assert_eq!(
        within(Settings::load(fixture.directory.path()))
            .await
            .unwrap(),
        settings
    );
    let ready = fixture.phase(AccelerationPhase::Ready).await;
    assert_eq!(ready.backend, AccelerationBackend::Cuda);
    let output = within(fixture.core.acceleration_output()).await.unwrap();
    assert!(
        output
            .iter()
            .any(|line| line.contains("Cuda backend thread")),
        "{output:?}"
    );
    assert!(output.len() <= 20);
    let report = within(fixture.core.disable_acceleration(RepairResources {
        program: Some(original.program.clone()),
        ..Default::default()
    }))
    .await
    .unwrap();
    assert_eq!(report.profile.as_ref().unwrap().program, original.program);
    let restored = within(fixture.core.settings()).await.unwrap();
    assert_eq!(restored.engine.as_ref().unwrap().program, original.program);
    assert_eq!(restored.engine.as_ref().unwrap().model, original.model);
    assert_eq!(restored.engine.as_ref().unwrap().config, original.config);
    assert_eq!(restored.main_engine_id, fixture.original.main_engine_id);
    analysis(&mut fixture.events, None).await;
    assert_eq!(fixture.close().await, 2);
}

#[tokio::test]
async fn startup_failure_retains_tail_downloads_and_restarts_original_engine() {
    let message = (0..30)
        .map(|i| format!("startup error {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut fixture = Fixture::new(
        json!({"backend": "cuda", "startup_error": message}),
        Duration::from_secs(3),
    )
    .await;
    let error = within(fixture.core.enable_acceleration(AccelerationBackend::Cuda))
        .await
        .unwrap_err();
    let AppError::Acceleration(AccelerationError::Failed { stderr_tail, .. }) = error else {
        panic!("{error:?}")
    };
    assert_eq!(stderr_tail.len(), 20, "{stderr_tail:?}");
    assert!(
        stderr_tail.last().unwrap().contains("startup error 29"),
        "{stderr_tail:?}"
    );
    assert!(fixture.archives.iter().all(|path| path.is_file()));
    fixture.assert_original_and_analyzing().await;
    let error = within(fixture.core.enable_acceleration(AccelerationBackend::Cuda))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        AppError::Acceleration(AccelerationError::Failed { .. })
    ));
    assert_eq!(
        fixture.close().await,
        2,
        "retry must reuse verified archives"
    );
}

#[tokio::test]
async fn startup_timeout_preserves_diagnostics_downloads_and_restores_analysis() {
    let mut fixture = Fixture::new(
        json!({"backend": "cuda", "gate": true}),
        Duration::from_millis(400),
    )
    .await;
    let error = within(fixture.core.enable_acceleration(AccelerationBackend::Cuda))
        .await
        .unwrap_err();
    let AppError::Acceleration(AccelerationError::TimedOut { stderr_tail }) = error else {
        panic!("{error:?}")
    };
    assert!(
        stderr_tail
            .iter()
            .any(|line| line.contains("Cuda backend thread")),
        "{stderr_tail:?}"
    );
    assert!(fixture.archives.iter().all(|path| path.is_file()));
    fixture.assert_original_and_analyzing().await;
    assert_eq!(fixture.close().await, 2);
}

#[tokio::test]
async fn activation_excludes_another_activation_and_benchmark_and_cancellation_rolls_back() {
    let mut fixture = Fixture::new(
        json!({"backend": "cuda", "gate": true}),
        Duration::from_secs(3),
    )
    .await;
    let operation = fixture.enabling();
    fixture.phase(AccelerationPhase::Starting).await;
    assert!(matches!(
        within(fixture.core.enable_acceleration(AccelerationBackend::Cuda)).await,
        Err(AppError::Acceleration(AccelerationError::Busy))
    ));
    assert!(within(fixture.core.start_benchmark()).await.is_err());
    within(fixture.core.cancel_asset_download()).await.unwrap();
    let error = within(operation).await.unwrap().unwrap_err();
    assert!(
        matches!(
            error,
            AppError::Acceleration(AccelerationError::Cancelled { .. })
        ),
        "{error:?}"
    );
    fixture.assert_original_and_analyzing().await;
    assert!(fixture.archives.iter().all(|path| path.is_file()));
    fixture.close().await;
}

#[tokio::test]
async fn failed_download_preserves_original_engine_and_returns_typed_failure() {
    let mut fixture = Fixture::with_download(
        json!({"backend":"cuda"}),
        Duration::from_secs(3),
        true,
        None,
    )
    .await;
    let error = within(fixture.core.enable_acceleration(AccelerationBackend::Cuda))
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            AppError::Acceleration(AccelerationError::Failed { .. })
        ),
        "{error:?}"
    );
    fixture.assert_original_and_analyzing().await;
    fixture.close().await;
}

#[tokio::test]
async fn cancelled_download_and_dismissed_hint_are_persisted_without_changing_engine() {
    let (release, wait) = tokio::sync::oneshot::channel();
    let mut fixture = Fixture::with_download(
        json!({"backend":"cuda"}),
        Duration::from_secs(3),
        false,
        Some(wait),
    )
    .await;
    let operation = fixture.enabling();
    fixture.phase(AccelerationPhase::Downloading).await;
    within(fixture.core.cancel_asset_download()).await.unwrap();
    let error = within(operation).await.unwrap().unwrap_err();
    assert!(
        matches!(
            error,
            AppError::Acceleration(AccelerationError::Cancelled { .. })
        ),
        "{error:?}"
    );
    fixture.assert_original_and_analyzing().await;
    drop(release);
    assert!(
        !within(fixture.core.settings())
            .await
            .unwrap()
            .acceleration_hint_dismissed
    );
    within(fixture.core.dismiss_acceleration_hint())
        .await
        .unwrap();
    assert!(
        within(fixture.core.settings())
            .await
            .unwrap()
            .acceleration_hint_dismissed
    );
    assert!(
        within(Settings::load(fixture.directory.path()))
            .await
            .unwrap()
            .acceleration_hint_dismissed
    );
    fixture.close().await;
}

#[tokio::test]
async fn cancellation_does_not_cancel_an_activation_queued_immediately_after_it() {
    let (release, wait) = tokio::sync::oneshot::channel();
    let mut fixture = Fixture::with_download(
        json!({"backend":"cuda"}),
        Duration::from_secs(3),
        false,
        Some(wait),
    )
    .await;
    let first = fixture.enabling();
    fixture.phase(AccelerationPhase::Downloading).await;
    let mut cancel = Box::pin(fixture.core.cancel_asset_download());
    // Queue the public cancellation without giving the actor a turn, then
    // enqueue the next activation behind it. No scheduler timing assumptions.
    within(std::future::poll_fn(|cx| {
        assert!(cancel.as_mut().poll(cx).is_pending());
        std::task::Poll::Ready(())
    }))
    .await;
    let (reply, response) = tokio::sync::oneshot::channel();
    within(
        fixture
            .core
            .commands
            .send(crate::actor::Command::EnableAcceleration(
                AccelerationBackend::Cuda,
                reply,
            )),
    )
    .await
    .unwrap();
    within(cancel).await.unwrap();
    assert!(matches!(
        within(first).await.unwrap(),
        Err(AppError::Acceleration(AccelerationError::Cancelled { .. }))
    ));
    release.send(()).unwrap();
    let settings = within(response).await.unwrap().unwrap();
    assert_eq!(settings.engines[0].name, "KataGo CUDA");
    assert_eq!(settings.main_engine_id, fixture.original.main_engine_id);
    fixture.close().await;
}

#[test]
fn old_settings_default_acceleration_hint_to_false() {
    let settings: Settings = serde_json::from_value(json!({"version":1})).unwrap();
    assert!(!settings.acceleration_hint_dismissed);
}

#[tokio::test]
async fn existing_benchmark_and_manual_trial_reject_acceleration() {
    let fixture = Fixture::new(json!({"backend":"cuda"}), Duration::from_secs(3)).await;
    let status = within(fixture.core.start_benchmark()).await.unwrap();
    assert_eq!(status.phase, crate::BenchmarkPhase::Running);
    assert!(matches!(
        within(fixture.core.enable_acceleration(AccelerationBackend::Cuda)).await,
        Err(AppError::Acceleration(AccelerationError::Busy))
    ));
    within(fixture.core.cancel_benchmark()).await.unwrap();
    let profile = fixture.original.engine.as_ref().unwrap().clone();
    let trial_gate = fixture.directory.path().join("manual-trial-ready");
    let trial = crate::EngineSetupProfile {
        program: profile.program,
        model: profile.model,
        config: Some(profile.config.clone()),
        extra_args: vec![
            "--startup-gate-file".into(),
            trial_gate.to_string_lossy().into_owned(),
            "--launch-log".into(),
            fixture
                .directory
                .path()
                .join("trial-launch.jsonl")
                .to_string_lossy()
                .into_owned(),
        ],
    };
    let core = fixture.core.clone();
    let operation =
        tokio::spawn(async move { core.try_configure_engine(trial, profile.config).await });
    within(async {
        let path = fixture.directory.path().join("trial-launch.jsonl");
        loop {
            if std::fs::read_to_string(&path).is_ok_and(|text| text.contains("manual-trial-ready"))
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(matches!(
        within(fixture.core.enable_acceleration(AccelerationBackend::Cuda)).await,
        Err(AppError::Acceleration(AccelerationError::Busy))
    ));
    operation.abort();
    assert!(within(operation).await.unwrap_err().is_cancelled());
    fixture.close().await;
}

#[tokio::test]
async fn opencl_tuning_event_extends_real_trial_and_adopts_it_after_gate_release() {
    let mut fixture = Fixture::new(
        json!({"backend":"opencl", "tuning_lines":1, "tuning_interval_ms":50, "gate":true}),
        Duration::from_millis(600),
    )
    .await;
    let core = fixture.core.clone();
    let mut operation =
        tokio::spawn(async move { core.enable_acceleration(AccelerationBackend::OpenCl).await });
    let progress = fixture.phase(AccelerationPhase::Tuning).await;
    assert_eq!(progress.backend, AccelerationBackend::OpenCl);
    assert!(
        progress
            .output_tail
            .iter()
            .any(|line| line.contains("Performing autotuning"))
    );
    // This timeout measures the behavior being tested: the operation must remain
    // pending beyond its original 600ms startup limit after a tuning line.
    assert!(
        within(tokio::time::timeout(
            Duration::from_millis(800),
            &mut operation
        ))
        .await
        .is_err()
    );
    std::fs::write(&fixture.gate, "").unwrap();
    let settings = within(operation).await.unwrap().unwrap();
    assert!(
        settings
            .engine
            .as_ref()
            .unwrap()
            .program
            .starts_with(fixture.directory.path().join("data/runtimes/opencl"))
    );
    assert_eq!(settings.engines[0].name, "KataGo OpenCL");
    fixture.phase(AccelerationPhase::Ready).await;
    fixture.close().await;
}
