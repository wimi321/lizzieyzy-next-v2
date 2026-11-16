#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use app_core::{
    AccelerationBackend, AppCore, AppError, AppEvent, AppSubscription, EngineBackend,
    EngineSetupProfile, RepairResources, WholeGameOptions, WholeGameStatus,
};
use common::within;
use futures_util::FutureExt;
use std::{future::Future, path::PathBuf, time::Duration};
use tokio::time::Instant;

fn required_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os(name).filter(|value| !value.is_empty());
    if path.is_none() {
        eprintln!("SKIP acceleration live: set {name}");
    }
    path.map(PathBuf::from)
}

// Real model initialization and downloads can take minutes. Every idle event
// wait is bounded to five seconds; an idle interval prints a heartbeat, never
// synchronizes the test by sleeping. Production commands own their deadlines.
async fn observe_stage<T>(
    name: &str,
    events: &mut AppSubscription,
    operation: impl Future<Output = Result<T, AppError>>,
) -> Result<T, AppError> {
    let started = Instant::now();
    eprintln!("[{name}] starting");
    tokio::pin!(operation);
    loop {
        tokio::select! {
            result = &mut operation => {
                eprintln!("[{name}] finished after {:.2}s: {}", started.elapsed().as_secs_f64(),
                    if result.is_ok() { "OK" } else { "FAILED" });
                return result;
            }
            event = tokio::time::timeout(Duration::from_secs(5), events.recv()) => {
                match event {
                    Ok(Some(AppEvent::AccelerationProgress(progress))) => {
                        eprintln!("[{name}] +{:.2}s {:?}: {:?}", started.elapsed().as_secs_f64(), progress.backend, progress.phase);
                        for line in progress.output_tail {
                            eprintln!("  {line}");
                        }
                    }
                    Ok(Some(AppEvent::EngineStatusChanged(status))) => {
                        eprintln!("[{name}] +{:.2}s engine {status:?}", started.elapsed().as_secs_f64());
                    }
                    Ok(None) => return Err(AppError::Closed),
                    Err(_) => eprintln!("[{name}] +{:.2}s waiting", started.elapsed().as_secs_f64()),
                    _ => {}
                }
            }
        }
    }
}

async fn analyze_empty_board(app: &AppCore, events: &mut AppSubscription, label: &str) -> f64 {
    let started = Instant::now();
    eprintln!("[{label}] analyzing empty board, 1000 visits");
    within(app.start_whole_game_analysis(WholeGameOptions {
        visits: 1000,
        ..Default::default()
    }))
    .await
    .unwrap();
    loop {
        let progress = within(app.whole_game_progress()).await.unwrap().unwrap();
        match progress.status {
            WholeGameStatus::Completed => break,
            WholeGameStatus::Error { reason } => panic!("{label}: {reason}"),
            WholeGameStatus::Cancelled | WholeGameStatus::Paused => {
                panic!("{label}: analysis stopped")
            }
            WholeGameStatus::Running => {}
        }
        assert!(
            started.elapsed() < Duration::from_secs(3600),
            "{label}: live analysis exceeded one hour"
        );
        match tokio::time::timeout(Duration::from_secs(5), events.recv()).await {
            Ok(Some(AppEvent::EngineStatusChanged(status))) => {
                eprintln!("[{label}] engine {status:?}")
            }
            Ok(None) => panic!("{label}: application closed"),
            Err(_) => eprintln!(
                "[{label}] +{:.2}s still analyzing",
                started.elapsed().as_secs_f64()
            ),
            _ => {}
        }
    }
    let root = within(app.variation_tree()).await.unwrap().root;
    let result = within(app.whole_game_result(root)).await.unwrap().unwrap();
    assert!(result.visits >= 1000);
    let seconds = started.elapsed().as_secs_f64();
    let speed = result.visits as f64 / seconds;
    eprintln!(
        "[{label}] visits={} elapsed={seconds:.3}s visits/s={speed:.2} (wall time includes dispatch)",
        result.visits
    );
    speed
}

// Explicit opt-in: only this ignored acceptance test can contact the pinned
// production download hosts. Ordinary tests use fake-katago and loopback HTTP.
#[tokio::test]
#[ignore = "manual Windows GPU acceptance; requires LIZZIE_LIVE_ACCEL, LIZZIE_LIVE_MODEL and LIZZIE_LIVE_CPU_ENGINE"]
async fn enable_analyze_and_return_to_bundled_cpu() {
    let backend = match std::env::var("LIZZIE_LIVE_ACCEL").ok().as_deref() {
        Some("cuda") => AccelerationBackend::Cuda,
        Some("tensorrt") => AccelerationBackend::TensorRt,
        Some("opencl") => AccelerationBackend::OpenCl,
        _ => {
            eprintln!("SKIP acceleration live: set LIZZIE_LIVE_ACCEL=cuda|tensorrt|opencl");
            return;
        }
    };
    let Some(model) = required_path("LIZZIE_LIVE_MODEL") else {
        return;
    };
    let Some(cpu_engine) = required_path("LIZZIE_LIVE_CPU_ENGINE") else {
        return;
    };
    if !cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        eprintln!("SKIP acceleration live: requires Windows x64");
        return;
    }
    let total = Instant::now();
    let model = model.canonicalize().expect("model path must exist");
    let cpu_engine = cpu_engine
        .canonicalize()
        .expect("bundled CPU engine path must exist");
    let directory = tempfile::Builder::new()
        .prefix("acceleration-live-")
        .tempdir()
        .unwrap();
    // Reuse only downloaded components, keeping this test's engine selection and
    // settings isolated even when the supplied data directory already exists.
    let data = std::env::var_os("LIZZIE_LIVE_DATA")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| directory.path().join("data"));
    let config_template =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../resources/katago/analysis.cfg");
    let config = app_core::AppConfig {
        cache_directory: Some(data),
        ..Default::default()
    };
    let (app, mut events) = within(app_core::spawn_with_config(directory.path(), config))
        .await
        .unwrap();
    let outcome = std::panic::AssertUnwindSafe(async {
        within(app.set_pondering(false)).await.unwrap();
        within(app.set_cache_enabled(false)).await.unwrap();
        observe_stage(
            "1 CPU startup",
            &mut events,
            app.try_configure_engine(
                EngineSetupProfile {
                    program: cpu_engine.clone(),
                    model: model.clone(),
                    config: None,
                    extra_args: vec![],
                },
                config_template.clone(),
            ),
        )
        .await
        .unwrap();
        let original = within(app.settings()).await.unwrap().engine.unwrap();
        let plan = observe_stage("2 plan", &mut events, app.acceleration_plan())
            .await
            .unwrap();
        eprintln!("[2 plan] {}", serde_json::to_string_pretty(&plan).unwrap());
        let accelerated = observe_stage("3 enable", &mut events, app.enable_acceleration(backend))
            .await
            .unwrap();
        let accelerated = accelerated.engine.unwrap();
        eprintln!("[3 enable] Ready: {:?}", accelerated.program);
        assert_ne!(accelerated.program, cpu_engine);
        assert_eq!(accelerated.model, original.model);
        assert_eq!(accelerated.config, original.config);
        let gpu_speed = analyze_empty_board(&app, &mut events, "4 GPU").await;
        let output = within(app.acceleration_output()).await.unwrap();
        let expected = match backend {
            AccelerationBackend::Cuda => EngineBackend::Cuda,
            AccelerationBackend::TensorRt => EngineBackend::TensorRt,
            AccelerationBackend::OpenCl => EngineBackend::OpenCl,
        };
        let backend_lines: Vec<_> = output
            .iter()
            .filter(|line| app_core::reported_acceleration_backend(line).is_some())
            .collect();
        eprintln!("[4 GPU] reported backend lines: {backend_lines:?}");
        assert!(
            output
                .iter()
                .any(|line| app_core::reported_acceleration_backend(line) == Some(expected)),
            "expected {expected:?} in KataGo output: {output:?}"
        );
        observe_stage(
            "5 CPU restore",
            &mut events,
            app.disable_acceleration(RepairResources {
                program: Some(cpu_engine.clone()),
                model: Some(model),
                config_template: Some(config_template),
            }),
        )
        .await
        .unwrap();
        let restored = within(app.settings()).await.unwrap().engine.unwrap();
        assert_eq!(restored.program, cpu_engine);
        assert_eq!(restored.model, original.model);
        assert_eq!(restored.config, original.config);
        let cpu_speed = analyze_empty_board(&app, &mut events, "5 CPU").await;
        eprintln!(
            "[comparison] GPU={gpu_speed:.2} visits/s CPU={cpu_speed:.2} visits/s ratio={:.2}",
            gpu_speed / cpu_speed
        );
        let plan_started = Instant::now();
        let plan = observe_stage("6 installed plan", &mut events, app.acceleration_plan())
            .await
            .unwrap();
        eprintln!(
            "[6 installed plan] elapsed={:.3}s {}",
            plan_started.elapsed().as_secs_f64(),
            serde_json::to_string_pretty(&plan).unwrap()
        );
        assert!(
            plan.options
                .iter()
                .any(|option| option.backend == backend && option.installed),
            "the backend must remain installed after tuning and analysis: {plan:?}"
        );
    })
    .catch_unwind()
    .await;
    let cleanup = within(app.shutdown()).await;
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
    cleanup.unwrap();
    eprintln!("[complete] total {:.2}s", total.elapsed().as_secs_f64());
}
