#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use std::future::Future;

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("benchmark process operation timed out")
}

fn profile(extra_args: Vec<String>) -> EngineProfile {
    let program = fake_katago::binary_path().unwrap();
    EngineProfile {
        remote: None,
        human_model: None,
        model: program.clone(),
        config: program.clone(),
        program,
        extra_args,
        canonical_model_name: None,
    }
}

#[test]
fn quick_arguments_use_analysis_config_with_gtp_alias_and_product_limits() {
    let profile = EngineProfile {
        remote: None,
        human_model: None,
        program: "program with spaces".into(),
        model: "model with spaces".into(),
        config: "config with spaces".into(),
        extra_args: vec![
            "-override-config".into(),
            "numSearchThreads=32,numAnalysisThreads=9,numSearchThreadsPerAnalysisThread=8,gpuToUse=1,logToStderr=true".into(),
            "-override-config=nnMaxBatchSize=64".into(),
        ],
        canonical_model_name: None,
    };
    let TransportSpec::LocalProcess { program, args, .. } = benchmark_transport(&profile).unwrap()
    else {
        panic!("expected local transport")
    };
    assert_eq!(program, profile.program);
    let args: Vec<_> = args.iter().map(|arg| arg.to_str().unwrap()).collect();
    assert_eq!(args[0], "benchmark");
    let option = |name: &str| args[args.iter().position(|arg| *arg == name).unwrap() + 1];
    assert_eq!(option("-config"), "config with spaces");
    assert_eq!(option("-model"), "model with spaces");
    assert_eq!(option("-n"), "4");
    assert_eq!(option("-v"), "400");
    assert_eq!(option("-time"), "5");
    assert!(args.contains(&"-s"));
    assert_eq!(
        args.iter()
            .filter(|arg| **arg == "-override-config")
            .count(),
        1
    );
    let overrides: Vec<_> = option("-override-config").split(',').collect();
    for expected in [
        "numAnalysisThreads=",
        "numSearchThreadsPerAnalysisThread=",
        "numSearchThreads=1",
        "gpuToUse=1",
        "nnMaxBatchSize=64",
        "logToStderr=false",
        "logAllGTPCommunication=false",
        "logSearchInfo=false",
    ] {
        assert!(overrides.contains(&expected), "missing {expected}");
    }
    for stale in [
        "numSearchThreads=32",
        "numAnalysisThreads=9",
        "numSearchThreadsPerAnalysisThread=8",
        "logToStderr=true",
    ] {
        assert!(!overrides.contains(&stale));
    }
}

#[tokio::test]
async fn real_recording_replay_finishes_only_after_reaping_child() {
    let profile = profile(Vec::new());
    let mut process = within(Process::spawn(&profile)).await.unwrap();
    let (result, saw_progress) = within(async {
        let mut saw_progress = false;
        loop {
            match process.next().await.unwrap() {
                Update::Progress { progress, .. } => {
                    saw_progress = true;
                    assert!(progress.threads > 0);
                    assert!(progress.completed_positions <= progress.total_positions);
                }
                Update::Finished {
                    metrics,
                    measurements,
                } => {
                    assert!(
                        measurements
                            .iter()
                            .any(|row| row.threads == metrics.threads)
                    );
                    break (metrics, saw_progress);
                }
            }
        }
    })
    .await;
    assert!(saw_progress);
    assert_eq!(result.threads, 6);
    assert_eq!(result.visits_per_second, 43.92);
    assert_eq!(result.nn_evals_per_second, 43.25);
    assert!(process.exit.unwrap().success());
    assert!(!process.stdout_open);
    assert!(!process.stderr_open);
    within(process.stop()).await.unwrap();
}

#[tokio::test]
async fn stopping_gated_benchmark_reaps_child_and_is_idempotent() {
    let directory = tempfile::tempdir().unwrap();
    let profile = profile(vec![
        "--benchmark-gate-file".into(),
        directory
            .path()
            .join("not-released")
            .to_str()
            .unwrap()
            .into(),
    ]);
    let mut process = within(Process::spawn(&profile)).await.unwrap();
    assert!(matches!(
        within(process.next()).await.unwrap(),
        Update::Progress { .. }
    ));
    assert!(process.child.try_wait().unwrap().is_none());
    within(process.stop()).await.unwrap();
    assert!(process.child.try_wait().unwrap().is_some());
    within(process.stop()).await.unwrap();
}

#[tokio::test]
async fn benchmark_failure_retains_stderr_reason() {
    let profile = profile(vec!["--benchmark-fail".into()]);
    let mut process = within(Process::spawn(&profile)).await.unwrap();
    let error = match within(process.next()).await {
        Err(error) => error.to_string(),
        Ok(_) => panic!("failing benchmark produced a successful update"),
    };
    assert!(error.contains("exited with"), "{error}");
    assert!(error.contains("stderr:"), "{error}");
    assert!(error.to_lowercase().contains("benchmark"), "{error}");
    assert!(!process.stderr_tail.is_empty());
    within(process.stop()).await.unwrap();
}

#[tokio::test]
async fn exit_zero_without_recommendation_is_not_success() {
    let directory = tempfile::tempdir().unwrap();
    let replay = directory.path().join("incomplete.txt");
    tokio::fs::write(
        &replay,
        b"numSearchThreads = 2: 6 / 6 positions, visits/s = 100 nnEvals/s = 90\r",
    )
    .await
    .unwrap();
    let profile = profile(vec![
        "--benchmark-replay".into(),
        replay.to_str().unwrap().into(),
    ]);
    let mut process = within(Process::spawn(&profile)).await.unwrap();
    let error = within(async {
        loop {
            match process.next().await {
                Ok(Update::Progress { .. }) => {}
                Ok(Update::Finished { .. }) => panic!("missing recommendation accepted"),
                Err(error) => break error.to_string(),
            }
        }
    })
    .await;
    assert!(error.contains("recommended thread count"), "{error}");
    assert!(process.exit.unwrap().success());
    within(process.stop()).await.unwrap();
}

#[tokio::test]
async fn long_b11_run_is_allowed_past_thirty_minutes_but_still_times_out_and_reaps() {
    let directory = tempfile::tempdir().unwrap();
    let profile = profile(vec![
        "--benchmark-gate-file".into(),
        directory.path().join("gate").to_str().unwrap().into(),
    ]);
    let mut process = within(Process::spawn(&profile)).await.unwrap();
    assert!(matches!(
        within(process.next()).await.unwrap(),
        Update::Progress { .. }
    ));
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(31 * 60)).await;
    assert!(process.deadline > Instant::now());
    tokio::time::advance(Duration::from_secs(30 * 60)).await;
    tokio::time::resume();
    let error = match within(process.next()).await {
        Err(error) => error,
        Ok(_) => panic!("expired benchmark did not fail"),
    };
    assert!(error.to_string().contains("60 minute limit"));
    within(process.stop()).await.unwrap();
    assert!(process.child.try_wait().unwrap().is_some());
}

#[tokio::test]
async fn reported_speed_matches_settings_that_survive_explicit_cli_overrides() {
    let replay = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/katago/benchmark-additional-recommendations.txt");
    let cases = [
        (vec![], Some((44.95, 44.27))),
        (
            vec!["-override-config=nnMaxBatchSize=8"],
            Some((20.0, 19.0)),
        ),
        (
            vec!["-override-config=nnMaxBatchSize=8,numNNServerThreadsPerModel=3"],
            Some((15.75, 15.99)),
        ),
        (
            vec![
                "-override-config=NNMAXBATCHSIZE=8",
                "-override-config=nnMaxBatchSize=2,numNNServerThreadsPerModel=2",
            ],
            Some((44.95, 44.27)),
        ),
        // The batch comparison was measured with the accepted server setting;
        // it says nothing about batch=2 with a conflicting manual server count.
        (vec!["-override-config=numNNServerThreadsPerModel=3"], None),
    ];
    for (overrides, expected) in cases {
        let mut args = vec!["--benchmark-replay".into(), replay.to_str().unwrap().into()];
        args.extend(overrides.into_iter().map(str::to_owned));
        let mut process = within(Process::spawn(&profile(args))).await.unwrap();
        let result = within(async {
            loop {
                match process.next().await {
                    Ok(Update::Finished { metrics, .. }) => break Ok(metrics),
                    Ok(Update::Progress { .. }) => {}
                    Err(error) => break Err(error),
                }
            }
        })
        .await;
        within(process.stop()).await.unwrap();
        match expected {
            Some(expected) => {
                let result = result.unwrap();
                assert_eq!(
                    (result.visits_per_second, result.nn_evals_per_second),
                    expected
                );
                assert_eq!(
                    (
                        result.nn_max_batch_size,
                        result.num_nn_server_threads_per_model
                    ),
                    (Some(2), Some(2))
                );
            }
            None => assert!(result.is_err()),
        }
    }
}
