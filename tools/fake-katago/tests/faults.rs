#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::Duration;

use common::{Engine, query, stream};
use serde_json::json;

#[test]
fn startup_delay_defers_the_version_reply() {
    let mut engine = Engine::spawn(&["--startup-delay-ms", "120"]);
    engine.ready();
    assert!(engine.started.elapsed() >= Duration::from_millis(120));
    engine.finish();
}

#[test]
fn fail_startup_reports_missing_model_and_exit_one() {
    let mut engine = Engine::spawn(&["--fail-startup"]);
    assert_eq!(engine.raw(), None);
    let (status, stderr) = engine.exited();
    assert_eq!(status.code(), Some(1));
    assert!(stderr.contains("模型找不到"));
}

#[test]
fn crash_after_queries_counts_analysis_requests_and_exits_on_threshold() {
    let mut engine = Engine::spawn(&["--crash-after-queries", "2"]);
    engine.ready();
    engine.ready();
    engine.send(&query("first"));
    assert_eq!(engine.recv()["id"], "first");
    engine.send(&query("second"));
    assert_eq!(engine.raw(), None);
    assert_eq!(engine.exited().0.code(), Some(1));
}

#[test]
fn crash_after_ms_exits_without_input_at_the_deadline() {
    let mut engine = Engine::spawn(&["--crash-after-ms", "120"]);
    assert_eq!(engine.raw(), None);
    assert!(engine.started.elapsed() >= Duration::from_millis(120));
    assert_eq!(engine.exited().0.code(), Some(1));
}

#[test]
fn hang_after_queries_suppresses_analysis_and_actions_until_process_crashes() {
    // A real process crash supplies the observation endpoint: silence is never
    // inferred from a sleep or a timed receive used as a synchronization step.
    let mut engine = Engine::spawn(&["--hang-after-queries", "2", "--crash-after-ms", "500"]);
    engine.ready();
    engine.send(&query("first"));
    assert_eq!(engine.recv()["id"], "first");
    engine.send(&stream("hung"));
    engine.send(&json!({"id":"silent", "action":"query_version"}));
    engine.send(&json!({"id":"stop", "action":"terminate_all"}));
    assert_eq!(engine.raw(), None, "hung engine must emit no further lines");
    assert!(engine.started.elapsed() >= Duration::from_millis(500));
    assert_eq!(engine.exited().0.code(), Some(1));
}

#[test]
fn garbage_every_inserts_invalid_lines_without_corrupting_protocol_lines() {
    let mut engine = Engine::spawn(&["--garbage-every", "2"]);
    for index in 0..6 {
        engine.send(&json!({"id":format!("v{index}"), "action":"query_version"}));
        assert_eq!(engine.recv()["id"], format!("v{index}"));
        if index % 2 == 1 {
            let garbage = engine.raw().unwrap();
            assert!(katago_protocol::parse_line(&garbage).is_err());
        }
    }
    engine.finish();
}

#[test]
fn threads_allows_two_queries_to_report_before_either_finishes() {
    let mut engine = Engine::spawn(&["--threads", "2"]);
    engine.ready();
    engine.send(&stream("a"));
    engine.send(&stream("b"));
    let mut ids = std::collections::BTreeSet::new();
    while ids.len() < 2 {
        let response = engine.recv();
        assert_eq!(response["isDuringSearch"], true);
        ids.insert(response["id"].as_str().unwrap().to_owned());
    }
    assert_eq!(ids, ["a".to_owned(), "b".to_owned()].into());
    engine.send(&json!({"id":"stop", "action":"terminate_all"}));
    let mut final_ids = std::collections::BTreeSet::new();
    let mut acknowledged = false;
    while final_ids.len() < 2 || !acknowledged {
        let response = engine.recv();
        if response["action"] == "terminate_all" {
            acknowledged = true;
        } else if response["isDuringSearch"] == false {
            assert!(acknowledged, "running cancellation follows acknowledgment");
            assert!(response.get("noResults").is_none());
            final_ids.insert(response["id"].as_str().unwrap().to_owned());
        }
    }
    assert_eq!(final_ids, ids);
    engine.finish();
}
