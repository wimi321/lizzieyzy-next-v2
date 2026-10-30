#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

#[test]
fn timed_crash_claims_once_and_restarted_search_finishes_after_the_deadline() {
    let directory = MarkerDirectory::new("timed-crash");
    let marker = directory.marker();
    let args = [
        "--crash-after-ms",
        "120",
        "--fault-once-file",
        marker.to_str().unwrap(),
    ];
    let mut first = Engine::spawn(&args);
    assert_eq!(first.raw(), None);
    assert!(first.started.elapsed() >= Duration::from_millis(120));
    let (status, stderr) = first.exited();
    assert_eq!(status.code(), Some(1));
    assert!(stderr.contains("injected crash"));
    assert!(marker.is_file(), "the timed crash must claim its marker");
    drop(first);

    let mut restarted = Engine::spawn(&args);
    restarted.ready();
    let mut request = query("past-the-crash-deadline");
    request["maxVisits"] = 300.into();
    restarted.send(&request);
    // At 1000 visits/sec this final result necessarily crosses the 120ms
    // crash deadline. Wait for the protocol event, never a fixed sleep.
    let result = restarted.recv();
    assert_eq!(result["id"], "past-the-crash-deadline");
    assert_eq!(result["isDuringSearch"], false);
    assert_eq!(result["rootInfo"]["visits"], 300);
    assert!(restarted.started.elapsed() >= Duration::from_millis(300));
    restarted.ready();
    restarted.finish();
    directory.remove();
}

#[test]
fn timed_crash_during_startup_claims_once_and_restart_becomes_ready() {
    let directory = MarkerDirectory::new("startup-timer");
    let marker = directory.marker();
    let args = [
        "--startup-delay-ms",
        "240",
        "--crash-after-ms",
        "120",
        "--fault-once-file",
        marker.to_str().unwrap(),
    ];
    let mut first = Engine::spawn(&args);
    assert_eq!(first.raw(), None);
    assert_eq!(first.exited().0.code(), Some(1));
    assert!(marker.is_file());
    drop(first);

    let mut restarted = Engine::spawn(&args);
    restarted.ready();
    assert!(restarted.started.elapsed() >= Duration::from_millis(240));
    restarted.send(&query("after-startup"));
    assert_eq!(restarted.recv()["id"], "after-startup");
    restarted.finish();
    directory.remove();
}

#[test]
fn normal_exit_before_timed_crash_does_not_consume_marker() {
    let directory = MarkerDirectory::new("untriggered-timer");
    let marker = directory.marker();
    let mut engine = Engine::spawn(&[
        "--crash-after-ms",
        "60000",
        "--fault-once-file",
        marker.to_str().unwrap(),
    ]);
    engine.ready();
    engine.finish();
    assert!(
        !marker.exists(),
        "an untriggered timer must not claim a fault"
    );
    directory.remove();
}

#[test]
fn fail_startup_claims_once_and_restarted_process_serves_queries() {
    let directory = MarkerDirectory::new("fail-startup");
    let marker = directory.marker();
    let args = [
        "--fail-startup",
        "--fault-once-file",
        marker.to_str().unwrap(),
    ];
    let mut first = Engine::spawn(&args);
    assert_eq!(first.raw(), None);
    let (status, stderr) = first.exited();
    assert_eq!(status.code(), Some(1));
    assert!(stderr.contains("模型找不到"));
    assert!(marker.is_file(), "startup failure must claim its marker");
    drop(first);

    let mut restarted = Engine::spawn(&args);
    restarted.ready();
    restarted.send(&query("after-startup-failure"));
    assert_eq!(restarted.recv()["id"], "after-startup-failure");
    restarted.finish();
    directory.remove();
}

#[test]
fn startup_and_timer_marker_errors_are_not_silently_ignored() {
    let directory = MarkerDirectory::new("startup-marker-error");
    for fault in [vec!["--fail-startup"], vec!["--crash-after-ms", "0"]] {
        for marker in [
            directory.0.join("missing-parent/claimed"),
            directory.0.clone(),
        ] {
            let mut args = fault.clone();
            args.extend(["--fault-once-file", marker.to_str().unwrap()]);
            let mut engine = Engine::spawn(&args);
            assert_eq!(engine.raw(), None);
            let (status, stderr) = engine.exited();
            assert_eq!(status.code(), Some(1));
            assert!(
                stderr.contains("could not claim fault-once marker"),
                "{stderr}"
            );
        }
    }
    directory.remove();
}
