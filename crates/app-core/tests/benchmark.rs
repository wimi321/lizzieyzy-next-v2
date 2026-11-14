#![allow(clippy::unwrap_used, clippy::expect_used)]
use app_core::{
    AppCore, AppEvent, AppSubscription, BenchmarkPhase, BenchmarkStatus, EngineProfile, Settings,
    WholeGameOptions, WholeGameStatus,
};
use std::{path::Path, time::Duration};

async fn within<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("deadline")
}

fn profile(dir: &Path, extra: &[&str]) -> EngineProfile {
    let program = fake_katago::binary_path().unwrap();
    let model = dir.join("model.bin.gz");
    let config = dir.join("analysis.cfg");
    std::fs::write(&model, "fake model").unwrap();
    std::fs::write(&config, "numSearchThreadsPerAnalysisThread=1").unwrap();
    let mut args = vec![
        "--launch-log".into(),
        dir.join("launches").to_str().unwrap().into(),
    ];
    args.extend(extra.iter().map(|s| (*s).to_owned()));
    EngineProfile {
        human_model: None,
        program,
        model,
        config,
        extra_args: args,
        canonical_model_name: Some("kata1-tf3-b11c768-s12002M-d6304M.bin.gz".into()),
    }
}

async fn setup(profile: EngineProfile, dir: &Path) -> (AppCore, AppSubscription) {
    let (app, mut events) = app_core::spawn(dir).await.unwrap();
    within(app.configure_engine(Some(profile))).await.unwrap();
    live(&mut events).await;
    (app, events)
}

async fn live(events: &mut AppSubscription) {
    within(async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::AnalysisUpdated(view)
                    if view.visits > 0 && view.source == app_core::AnalysisSource::Live =>
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

async fn benchmark_event(
    events: &mut AppSubscription,
    predicate: impl Fn(&BenchmarkStatus) -> bool,
) -> BenchmarkStatus {
    within(async {
        loop {
            if let AppEvent::BenchmarkProgress(status) = events.recv().await.unwrap()
                && predicate(&status)
            {
                return status;
            }
        }
    })
    .await
}

fn launches(dir: &Path) -> Vec<Vec<String>> {
    std::fs::read_to_string(dir.join("launches"))
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}

fn overrides(args: &[String]) -> &str {
    let i = args.iter().position(|s| s == "-override-config").unwrap();
    assert_eq!(args.iter().filter(|s| *s == "-override-config").count(), 1);
    &args[i + 1]
}

#[tokio::test]
async fn complete_saves_restarts_with_recommendation_resumes_and_reset_restores_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let gate = dir.path().join("gate");
    let engine = profile(
        dir.path(),
        &["--benchmark-gate-file", gate.to_str().unwrap()],
    );
    let (app, mut events) = setup(engine.clone(), dir.path()).await;
    app.open_bytes(b"(;SZ[9];B[aa];W[bb])".to_vec())
        .await
        .unwrap();
    app.start_whole_game_analysis(WholeGameOptions {
        visits: 1_000_000,
        ..Default::default()
    })
    .await
    .unwrap();
    assert_eq!(
        app.start_benchmark().await.unwrap().phase,
        BenchmarkPhase::Running
    );
    let progress = benchmark_event(&mut events, |s| s.progress.is_some()).await;
    assert_eq!(progress.phase, BenchmarkPhase::Running);
    assert_eq!(
        app.whole_game_progress().await.unwrap().unwrap().status,
        WholeGameStatus::Paused
    );
    assert!(matches!(
        app.set_pondering(true).await,
        Err(app_core::AppError::Benchmark(
            app_core::BenchmarkError::Busy
        ))
    ));
    assert!(app.start_benchmark().await.is_err());
    assert!(app.configure_engine(None).await.is_err());
    assert!(app.resume_whole_game_analysis().await.is_err());
    std::fs::write(gate, "release").unwrap();
    let done = benchmark_event(&mut events, |s| s.phase != BenchmarkPhase::Running).await;
    assert_eq!(done.phase, BenchmarkPhase::Completed, "{:?}", done.error);
    let result = done.last_result.unwrap();
    assert_eq!(
        (
            result.recommended_threads,
            result.visits_per_second,
            result.nn_evals_per_second
        ),
        (6, 43.92, 43.25)
    );
    assert!(result.measured_at_unix_ms > 0);
    assert_eq!(result.canonical_model_name, engine.canonical_model_name);
    assert!(
        done.measurements
            .iter()
            .any(|row| row.threads == 12 && row.visits_per_second > result.visits_per_second)
    );
    let saved = Settings::load(dir.path()).await.unwrap();
    assert_eq!(saved.engine, Some(engine));
    assert_eq!(
        saved.benchmarks.get(&result.engine_fingerprint),
        Some(&result)
    );
    assert!(saved.pondering);
    assert_eq!(
        app.whole_game_progress().await.unwrap().unwrap().status,
        WholeGameStatus::Running
    );
    live(&mut events).await;
    let runs = launches(dir.path());
    let args = runs
        .iter()
        .rev()
        .find(|args| args[0] == "analysis")
        .unwrap();
    assert!(overrides(args).contains("numSearchThreadsPerAnalysisThread=6"));
    assert!(overrides(args).contains("numAnalysisThreads=2"));
    within(app.shutdown()).await.unwrap();
    // A fresh application applies the persisted result without running a benchmark.
    let (app, mut events) = app_core::spawn(dir.path()).await.unwrap();
    live(&mut events).await;
    assert_eq!(
        app.benchmark_status().await.unwrap().last_result,
        Some(result)
    );
    assert_eq!(
        launches(dir.path())
            .iter()
            .filter(|a| a[0] == "benchmark")
            .count(),
        1
    );
    let reset = app.reset_benchmark().await.unwrap();
    assert_eq!(reset.phase, BenchmarkPhase::Idle);
    assert!(reset.last_result.is_none());
    live(&mut events).await;
    let runs = launches(dir.path());
    assert!(
        !overrides(runs.iter().rev().find(|a| a[0] == "analysis").unwrap())
            .contains("numSearchThreadsPerAnalysisThread=")
    );
    assert!(
        Settings::load(dir.path())
            .await
            .unwrap()
            .benchmarks
            .is_empty()
    );
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn cancel_reaps_benchmark_resumes_analysis_and_leaves_settings_bytes_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let gate = dir.path().join("gate");
    let pid = dir.path().join("pid");
    let engine = profile(
        dir.path(),
        &[
            "--benchmark-gate-file",
            gate.to_str().unwrap(),
            "--benchmark-pid-file",
            pid.to_str().unwrap(),
        ],
    );
    let (app, mut events) = setup(engine, dir.path()).await;
    let before = std::fs::read(dir.path().join("settings.json")).unwrap();
    app.start_benchmark().await.unwrap();
    benchmark_event(&mut events, |s| s.progress.is_some()).await;
    let status = within(app.cancel_benchmark()).await.unwrap();
    assert_eq!(status.phase, BenchmarkPhase::Cancelled);
    assert_eq!(
        std::fs::read(dir.path().join("settings.json")).unwrap(),
        before
    );
    #[cfg(unix)]
    assert!(
        !std::process::Command::new("kill")
            .args(["-0", std::fs::read_to_string(pid).unwrap().trim()])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .success()
    );
    live(&mut events).await;
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn failure_reports_stderr_and_restores_analysis_without_saving() {
    let dir = tempfile::tempdir().unwrap();
    let engine = profile(dir.path(), &["--benchmark-fail"]);
    let (app, mut events) = setup(engine, dir.path()).await;
    let before = app.settings().await.unwrap();
    app.start_benchmark().await.unwrap();
    let status = benchmark_event(&mut events, |s| s.phase == BenchmarkPhase::Failed).await;
    assert!(status.error.unwrap().contains("injected benchmark failure"));
    assert_eq!(app.settings().await.unwrap(), before);
    live(&mut events).await;
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn changed_engine_files_reject_result_and_explicit_review_cancel_is_not_undone() {
    let dir = tempfile::tempdir().unwrap();
    let gate = dir.path().join("gate");
    let engine = profile(
        dir.path(),
        &["--benchmark-gate-file", gate.to_str().unwrap()],
    );
    let (app, mut events) = setup(engine.clone(), dir.path()).await;
    app.start_whole_game_analysis(WholeGameOptions {
        visits: 1_000_000,
        ..Default::default()
    })
    .await
    .unwrap();
    app.start_benchmark().await.unwrap();
    benchmark_event(&mut events, |s| s.progress.is_some()).await;
    app.cancel_whole_game_analysis().await.unwrap();
    std::fs::write(&engine.model, "changed model identity").unwrap();
    std::fs::write(gate, "release").unwrap();
    let status = benchmark_event(&mut events, |s| s.phase == BenchmarkPhase::Failed).await;
    assert!(status.error.unwrap().contains("engine files changed"));
    assert!(app.settings().await.unwrap().benchmarks.is_empty());
    assert_eq!(
        app.whole_game_progress().await.unwrap().unwrap().status,
        WholeGameStatus::Cancelled
    );
    live(&mut events).await;
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn settings_write_failure_preserves_previous_settings_and_resumes() {
    let dir = tempfile::tempdir().unwrap();
    let gate = dir.path().join("gate");
    let engine = profile(
        dir.path(),
        &["--benchmark-gate-file", gate.to_str().unwrap()],
    );
    let (app, mut events) = setup(engine, dir.path()).await;
    let before = app.settings().await.unwrap();
    app.start_benchmark().await.unwrap();
    benchmark_event(&mut events, |s| s.progress.is_some()).await;
    let settings = dir.path().join("settings.json");
    std::fs::remove_file(&settings).unwrap();
    std::fs::create_dir(&settings).unwrap();
    std::fs::write(gate, "release").unwrap();
    let status = benchmark_event(&mut events, |s| s.phase == BenchmarkPhase::Failed).await;
    assert!(status.error.unwrap().contains("settings"));
    assert_eq!(app.settings().await.unwrap(), before);
    live(&mut events).await;
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn dropping_last_handle_reaps_benchmark_without_restarting_analysis() {
    let dir = tempfile::tempdir().unwrap();
    let gate = dir.path().join("gate");
    let pid = dir.path().join("pid");
    let engine = profile(
        dir.path(),
        &[
            "--benchmark-gate-file",
            gate.to_str().unwrap(),
            "--benchmark-pid-file",
            pid.to_str().unwrap(),
        ],
    );
    let (app, mut events) = setup(engine, dir.path()).await;
    app.start_benchmark().await.unwrap();
    benchmark_event(&mut events, |s| s.progress.is_some()).await;
    drop(app);
    within(async { while events.recv().await.is_some() {} }).await;
    #[cfg(unix)]
    assert!(
        !std::process::Command::new("kill")
            .args(["-0", std::fs::read_to_string(pid).unwrap().trim()])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(
        launches(dir.path())
            .iter()
            .filter(|a| a[0] == "analysis")
            .count(),
        1
    );
}

#[tokio::test]
async fn settings_reject_out_of_range_saved_threads_and_mismatched_fingerprint() {
    let dir = tempfile::tempdir().unwrap();
    for (threads, fingerprint) in [(0, "key"), (4097, "key"), (1, "other-key")] {
        let mut settings = Settings::default();
        settings.benchmarks.insert(
            "key".into(),
            app_core::BenchmarkResult {
                engine_fingerprint: fingerprint.into(),
                recommended_threads: threads,
                nn_max_batch_size: None,
                num_nn_server_threads_per_model: None,
                visits_per_second: 100.0,
                nn_evals_per_second: 80.0,
                measured_at_unix_ms: 1,
                canonical_model_name: None,
            },
        );
        std::fs::write(
            dir.path().join("settings.json"),
            serde_json::to_vec(&settings).unwrap(),
        )
        .unwrap();
        assert!(Settings::load(dir.path()).await.is_err());
    }
}

#[tokio::test]
async fn additional_recommendations_apply_on_restart_reload_and_trial_then_reset_together() {
    let replay = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/katago/benchmark-additional-recommendations.txt");
    for explicit in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let mut engine = profile(
            dir.path(),
            &["--benchmark-replay", replay.to_str().unwrap()],
        );
        // This reproduces the production configuration that hid the batch gain.
        let original_config = "numSearchThreadsPerAnalysisThread=1\nnnMaxBatchSize=64\n";
        std::fs::write(&engine.config, original_config).unwrap();
        if explicit {
            engine.extra_args.extend([
                "-override-config".into(),
                "nnMaxBatchSize=8,NUMNNSERVERTHREADSPERMODEL=3".into(),
            ]);
        }
        let (app, mut events) = setup(engine.clone(), dir.path()).await;
        app.start_benchmark().await.unwrap();
        let done = benchmark_event(&mut events, |s| s.phase != BenchmarkPhase::Running).await;
        assert_eq!(done.phase, BenchmarkPhase::Completed, "{:?}", done.error);
        let result = done.last_result.unwrap();
        assert_eq!(
            (
                result.recommended_threads,
                result.nn_max_batch_size,
                result.num_nn_server_threads_per_model
            ),
            (4, Some(2), Some(2))
        );
        assert_eq!(
            (result.visits_per_second, result.nn_evals_per_second),
            if explicit {
                (15.75, 15.99)
            } else {
                (44.95, 44.27)
            }
        );
        assert_eq!(done.measurements.len(), 1);
        assert_eq!(done.measurements[0].visits_per_second, 15.75);
        assert_eq!(
            Settings::load(dir.path()).await.unwrap().benchmarks[&result.engine_fingerprint],
            result
        );
        live(&mut events).await;
        assert_managed_launch(dir.path(), explicit);
        within(app.shutdown()).await.unwrap();
        let (app, mut events) = app_core::spawn(dir.path()).await.unwrap();
        live(&mut events).await;
        assert_managed_launch(dir.path(), explicit);
        // The alternative Ready-before-adopt configuration path must apply all
        // saved suggestions too, without altering the base profile fingerprint.
        within(app.try_configure_engine(
            app_core::EngineSetupProfile {
                program: engine.program.clone(),
                model: engine.model.clone(),
                config: Some(engine.config.clone()),
                extra_args: engine.extra_args.clone(),
            },
            engine.config.clone(),
        ))
        .await
        .unwrap();
        live(&mut events).await;
        assert_managed_launch(dir.path(), explicit);
        app.reset_benchmark().await.unwrap();
        live(&mut events).await;
        let runs = launches(dir.path());
        let args = runs
            .iter()
            .rev()
            .find(|args| args[0] == "analysis")
            .unwrap();
        let entries = overrides(args).to_ascii_lowercase();
        assert!(!entries.contains("numsearchthreadsperanalysisthread="));
        if explicit {
            assert!(entries.contains("nnmaxbatchsize=8"));
            assert!(entries.contains("numnnserverthreadspermodel=3"));
        } else {
            assert!(!entries.contains("nnmaxbatchsize="));
            assert!(!entries.contains("numnnserverthreadspermodel="));
        }
        assert_eq!(
            std::fs::read_to_string(&engine.config).unwrap(),
            original_config
        );
        assert!(app.settings().await.unwrap().benchmarks.is_empty());
        within(app.shutdown()).await.unwrap();
    }
}

fn assert_managed_launch(dir: &Path, explicit: bool) {
    let runs = launches(dir);
    let args = runs
        .iter()
        .rev()
        .find(|args| args[0] == "analysis")
        .unwrap();
    let entries = overrides(args).to_ascii_lowercase();
    assert!(entries.contains("numsearchthreadsperanalysisthread=4"));
    assert!(entries.contains("numanalysisthreads=2"));
    let batch = if explicit { 8 } else { 2 };
    let server = if explicit { 3 } else { 2 };
    assert!(entries.contains(&format!("nnmaxbatchsize={batch}")));
    assert!(entries.contains(&format!("numnnserverthreadspermodel={server}")));
    assert_eq!(entries.matches("nnmaxbatchsize=").count(), 1);
    assert_eq!(entries.matches("numnnserverthreadspermodel=").count(), 1);
}

#[tokio::test]
async fn old_results_load_without_additional_keys_and_invalid_suggestions_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let mut json = serde_json::json!({"version":1,"benchmarks":{"key":{
        "engine_fingerprint":"key", "recommended_threads":4,
        "visits_per_second":15.75, "nn_evals_per_second":15.99,
        "measured_at_unix_ms":1, "canonical_model_name":null
    }}});
    let path = dir.path().join("settings.json");
    std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
    let loaded = Settings::load(dir.path()).await.unwrap();
    let result = &loaded.benchmarks["key"];
    assert_eq!(result.nn_max_batch_size, None);
    assert_eq!(result.num_nn_server_threads_per_model, None);
    loaded.save(dir.path()).await.unwrap();
    assert_eq!(Settings::load(dir.path()).await.unwrap(), loaded);
    for (batch, servers) in [(0, 2), (65537, 2), (2, 0), (2, 257)] {
        json["benchmarks"]["key"]["nn_max_batch_size"] = serde_json::json!(batch);
        json["benchmarks"]["key"]["num_nn_server_threads_per_model"] = serde_json::json!(servers);
        std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
        assert!(Settings::load(dir.path()).await.is_err());
    }
}

#[tokio::test]
async fn reset_cancels_a_pending_trial_so_old_managed_overrides_cannot_return() {
    let dir = tempfile::tempdir().unwrap();
    let gate = dir.path().join("startup-gate");
    let replay = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/katago/benchmark-additional-recommendations.txt");
    std::fs::write(&gate, "ready").unwrap();
    let engine = profile(
        dir.path(),
        &[
            "--startup-gate-file",
            gate.to_str().unwrap(),
            "--benchmark-replay",
            replay.to_str().unwrap(),
        ],
    );
    let (app, mut events) = setup(engine.clone(), dir.path()).await;
    app.start_benchmark().await.unwrap();
    let done = benchmark_event(&mut events, |s| s.phase != BenchmarkPhase::Running).await;
    assert_eq!(done.phase, BenchmarkPhase::Completed);
    live(&mut events).await;
    let count = launches(dir.path()).len();
    std::fs::remove_file(&gate).unwrap();
    let candidate = app_core::EngineSetupProfile {
        program: engine.program,
        model: engine.model,
        config: Some(engine.config.clone()),
        extra_args: engine.extra_args,
    };
    let pending_app = app.clone();
    let pending = tokio::spawn(async move {
        pending_app
            .try_configure_engine(candidate, engine.config)
            .await
    });
    // The launch log is emitted before the external startup gate. The candidate
    // has captured the saved tuning, but cannot become Ready before reset.
    within(async {
        loop {
            let log = std::fs::read_to_string(dir.path().join("launches")).unwrap();
            let complete = log
                .lines()
                .filter_map(|line| serde_json::from_str::<Vec<String>>(line).ok())
                .count();
            if complete > count {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert_managed_launch(dir.path(), false);
    assert_eq!(
        within(app.reset_benchmark()).await.unwrap().phase,
        BenchmarkPhase::Idle
    );
    assert!(matches!(
        within(pending).await.unwrap(),
        Err(app_core::AppError::EngineSetup(
            app_core::EngineSetupError::Cancelled
        ))
    ));
    std::fs::write(&gate, "release").unwrap();
    live(&mut events).await;
    let runs = launches(dir.path());
    let args = overrides(runs.iter().rev().find(|a| a[0] == "analysis").unwrap());
    for key in [
        "numSearchThreadsPerAnalysisThread=",
        "nnMaxBatchSize=",
        "numNNServerThreadsPerModel=",
    ] {
        assert!(!args.contains(key));
    }
    assert!(app.settings().await.unwrap().benchmarks.is_empty());
    within(app.shutdown()).await.unwrap();
}
