#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;
use common::{Engine, TempDirectory};

#[test]
fn backend_and_all_tuning_lines_precede_readiness_and_reach_the_selected_log() {
    let directory = TempDirectory::new("startup-output");
    let log = directory.path().join("startup.log");
    let override_config = format!("logFile={}", log.display());
    let mut engine = Engine::spawn(&[
        "--backend",
        "opencl",
        "--tuning-lines",
        "4",
        "--tuning-interval-ms",
        "1",
        "-override-config",
        &override_config,
    ]);
    engine.ready();
    // The version response is the barrier: all tuning must have finished first.
    let lines = std::fs::read_to_string(&log).unwrap();
    assert_eq!(lines.lines().count(), 5);
    assert!(lines.contains("OpenCL backend thread"));
    for marker in [
        "Performing autotuning",
        "Tuning xGemmDirect",
        "Started OpenCL SGEMM",
        "This will take a while",
    ] {
        assert!(lines.contains(marker));
    }
    engine.close_input();
    assert_eq!(engine.raw(), None);
    let (status, stderr) = engine.exited();
    assert!(status.success());
    assert_eq!(stderr, lines.trim_end());
}

#[test]
fn custom_startup_error_keeps_backend_and_error_in_stderr_and_log() {
    let directory = TempDirectory::new("startup-error");
    let log = directory.path().join("startup.log");
    let override_config = format!("logFile={}", log.display());
    let mut engine = Engine::spawn(&[
        "--backend",
        "cuda",
        "--startup-error",
        "CUDA driver version is insufficient",
        "-override-config",
        &override_config,
    ]);
    assert_eq!(engine.raw(), None);
    let (status, stderr) = engine.exited();
    assert_eq!(status.code(), Some(1));
    assert!(stderr.contains("Cuda backend thread 0: Found GPU"));
    assert!(stderr.contains("CUDA driver version is insufficient"));
    assert_eq!(std::fs::read_to_string(log).unwrap().trim_end(), stderr);
}

#[test]
fn per_executable_script_changes_the_candidate_without_changing_shared_arguments() {
    let directory = TempDirectory::new("startup-sidecar");
    let candidate = directory
        .path()
        .join(format!("candidate{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(env!("CARGO_BIN_EXE_fake-katago"), &candidate).unwrap();
    std::fs::write(
        directory.path().join("startup.json"),
        r#"{
        "backend":"tensorrt", "startup_error":"candidate-only failure"
    }"#,
    )
    .unwrap();
    let args = ["--startup-script-relative", "startup.json"];
    let mut cpu = Engine::spawn(&args);
    cpu.ready();
    cpu.finish();
    let mut gpu = Engine::spawn_at(&candidate, &args);
    assert_eq!(gpu.raw(), None);
    let (status, stderr) = gpu.exited();
    assert_eq!(status.code(), Some(1));
    assert!(stderr.contains("TensorRT backend thread"));
    assert!(stderr.contains("candidate-only failure"));
}

#[test]
fn cpu_fixture_is_a_distinct_binary_with_the_same_analysis_protocol() {
    let cpu_bytes = std::fs::read(env!("CARGO_BIN_EXE_fake-katago-cpu")).unwrap();
    let gpu_bytes = std::fs::read(env!("CARGO_BIN_EXE_fake-katago")).unwrap();
    assert_ne!(cpu_bytes, gpu_bytes);
    let mut engine = Engine::spawn_at(env!("CARGO_BIN_EXE_fake-katago-cpu"), &[]);
    engine.send(&serde_json::json!({"id":"version", "action":"query_version"}));
    assert_eq!(engine.recv()["version"], "1.16.5-fake-cpu");
    engine.send(&common::query("cpu"));
    assert_eq!(engine.recv()["id"], "cpu");
    engine.finish();
}
