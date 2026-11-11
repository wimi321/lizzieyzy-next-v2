#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{Engine, TempDirectory, query};
use serde_json::json;

#[test]
fn externally_created_startup_gate_releases_queued_version_and_analysis() {
    let directory = TempDirectory::new("gate-release");
    let gate = directory.path().join("ready");
    let mut engine = Engine::spawn(&["--startup-gate-file", gate.to_str().unwrap()]);
    engine.send(&json!({"id": "version", "action": "query_version"}));
    engine.send(&query("queued"));
    assert!(
        !gate.exists(),
        "fake engine must not create its startup gate"
    );
    std::fs::write(&gate, b"release").unwrap();
    assert_eq!(engine.recv()["action"], "query_version");
    assert_eq!(engine.recv()["id"], "queued");
    engine.finish();
    assert_eq!(std::fs::read(&gate).unwrap(), b"release");
}

#[test]
fn unopened_startup_gate_defers_input_until_independent_crash_deadline() {
    let directory = TempDirectory::new("gate-crash");
    let gate = directory.path().join("unreleased");
    let fault = directory.path().join("fault");
    let mut engine = Engine::spawn(&[
        "--startup-gate-file",
        gate.to_str().unwrap(),
        "--crash-after-ms",
        "300",
        "--hang-after-queries",
        "1",
        "--fault-once-file",
        fault.to_str().unwrap(),
    ]);
    engine.send(&json!({"id": "version", "action": "query_version"}));
    engine.send(&query("must-not-be-consumed"));
    assert_eq!(
        engine.raw(),
        None,
        "gate must prevent even the version reply"
    );
    let (status, stderr) = engine.exited();
    assert_eq!(status.code(), Some(1));
    assert!(stderr.contains("injected crash"));
    assert!(!gate.exists());
    assert!(
        fault.is_file(),
        "the timer must claim its one-shot fault even behind the gate"
    );
}

#[test]
fn startup_gate_rejects_a_directory_with_a_typed_error() {
    let directory = TempDirectory::new("gate-invalid");
    let mut engine = Engine::spawn(&["--startup-gate-file", directory.path().to_str().unwrap()]);
    assert_eq!(engine.raw(), None);
    let (status, stderr) = engine.exited();
    assert_eq!(status.code(), Some(1));
    assert!(stderr.contains("could not read startup gate"));
}
