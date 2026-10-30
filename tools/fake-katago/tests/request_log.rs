#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{Engine, query};
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn request_log_records_received_queries_actions_and_appends_across_processes() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("fake-request-log-{}-{unique}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("requests.jsonl");
    let args = ["--request-log", path.to_str().unwrap()];
    let mut engine = Engine::spawn(&args);
    engine.ready();
    let request = query("review");
    engine.send(&request);
    assert_eq!(engine.recv()["id"], "review");
    let terminate = json!({"id":"cancel", "action":"terminate", "terminateId":"review"});
    engine.send(&terminate);
    assert_eq!(engine.recv(), terminate);
    engine.finish();

    let mut restarted = Engine::spawn(&args);
    restarted.ready();
    restarted.finish();
    let lines: Vec<Value> = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0], json!({"id":"ready", "action":"query_version"}));
    assert_eq!(lines[1], request);
    assert_eq!(lines[2], terminate);
    assert_eq!(lines[3], lines[0]);
    std::fs::remove_dir_all(directory).unwrap();
}
