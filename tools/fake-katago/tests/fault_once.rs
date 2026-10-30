#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use common::{Engine, SAFETY_TIMEOUT, query};

struct MarkerDirectory(PathBuf);

impl MarkerDirectory {
    fn new(label: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "fake-katago-fault-{label}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn marker(&self) -> PathBuf {
        self.0.join("claimed")
    }

    fn remove(self) {
        std::fs::remove_dir_all(&self.0).unwrap();
        assert!(!self.0.exists(), "marker directory must not leak");
    }
}

impl Drop for MarkerDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn wait_for_marker(path: &Path) {
    let deadline = Instant::now() + SAFETY_TIMEOUT;
    while !path.is_file() {
        assert!(
            Instant::now() < deadline,
            "engine did not claim the fault marker"
        );
        std::thread::yield_now();
    }
}

#[test]
fn first_query_hangs_once_and_same_marker_allows_restarted_queries() {
    let directory = MarkerDirectory::new("hang");
    let marker = directory.marker();
    let args = [
        "--hang-after-queries",
        "1",
        "--fault-once-file",
        marker.to_str().unwrap(),
    ];
    let mut first = Engine::spawn(&args);
    // The first request itself triggers the fault: no successful warmup query.
    first.send(&query("hung"));
    wait_for_marker(&marker);
    first.kill();
    assert_eq!(first.raw(), None, "a faulting query must emit no result");
    let (status, stderr) = first.exited();
    assert!(!status.success());
    assert!(stderr.is_empty());
    drop(first);

    let mut restarted = Engine::spawn(&args);
    restarted.ready();
    for id in ["replayed", "next"] {
        restarted.send(&query(id));
        let result = restarted.recv();
        assert_eq!(result["id"], id);
        assert_eq!(result["isDuringSearch"], false);
    }
    restarted.finish();
    assert!(marker.is_file());
    directory.remove();
}

#[test]
fn crash_marker_is_not_consumed_by_startup_or_queries_below_threshold() {
    let directory = MarkerDirectory::new("crash");
    let marker = directory.marker();
    let args = [
        "--crash-after-queries",
        "2",
        "--fault-once-file",
        marker.to_str().unwrap(),
    ];
    let mut first = Engine::spawn(&args);
    first.ready();
    assert!(
        !marker.exists(),
        "normal readiness must not consume the fault"
    );
    first.send(&query("below-threshold"));
    assert_eq!(first.recv()["id"], "below-threshold");
    assert!(!marker.exists());
    first.send(&query("crash"));
    assert_eq!(first.raw(), None);
    let (status, stderr) = first.exited();
    assert_eq!(status.code(), Some(1));
    assert!(stderr.contains("injected crash"));
    assert!(marker.is_file());
    drop(first);

    let mut restarted = Engine::spawn(&args);
    for id in ["first", "second", "third"] {
        restarted.send(&query(id));
        assert_eq!(restarted.recv()["id"], id);
    }
    restarted.finish();
    directory.remove();
}

#[test]
fn zero_query_threshold_claims_once_and_restart_reaches_readiness() {
    for option in ["--hang-after-queries", "--crash-after-queries"] {
        let directory = MarkerDirectory::new(option);
        let marker = directory.marker();
        let args = [option, "0", "--fault-once-file", marker.to_str().unwrap()];
        let mut first = Engine::spawn(&args);
        wait_for_marker(&marker);
        if option == "--hang-after-queries" {
            first.kill();
        }
        assert_eq!(first.raw(), None);
        assert!(!first.exited().0.success());
        drop(first);
        let mut restarted = Engine::spawn(&args);
        restarted.ready();
        restarted.send(&query("normal"));
        assert_eq!(restarted.recv()["id"], "normal");
        restarted.finish();
        directory.remove();
    }
}

#[test]
fn fault_marker_write_errors_are_reported_instead_of_disabling_the_fault() {
    let directory = MarkerDirectory::new("error");
    for marker in [
        directory.0.join("missing-parent/claimed"),
        directory.0.clone(),
    ] {
        let mut engine = Engine::spawn(&[
            "--hang-after-queries",
            "1",
            "--fault-once-file",
            marker.to_str().unwrap(),
        ]);
        engine.ready();
        engine.send(&query("trigger"));
        assert_eq!(engine.raw(), None);
        let (status, stderr) = engine.exited();
        assert_eq!(status.code(), Some(1));
        assert!(
            stderr.contains("could not claim fault-once marker"),
            "{stderr}"
        );
    }
    directory.remove();
}
