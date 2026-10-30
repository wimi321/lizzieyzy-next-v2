#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use common::{Engine, query};
use serde_json::json;

struct GateDirectory(PathBuf);

impl GateDirectory {
    fn new(label: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "fake-katago-gate-{label}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for GateDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn externally_created_startup_gate_releases_queued_version_and_analysis() {
    let directory = GateDirectory::new("release");
    let gate = directory.0.join("ready");
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
    let directory = GateDirectory::new("crash");
    let gate = directory.0.join("unreleased");
    let fault = directory.0.join("fault");
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
        !fault.exists(),
        "queries must not be consumed behind the gate"
    );
}

#[test]
fn startup_gate_rejects_a_directory_with_a_typed_error() {
    let directory = GateDirectory::new("invalid");
    let mut engine = Engine::spawn(&["--startup-gate-file", directory.0.to_str().unwrap()]);
    assert_eq!(engine.raw(), None);
    let (status, stderr) = engine.exited();
    assert_eq!(status.code(), Some(1));
    assert!(stderr.contains("could not read startup gate"));
}
