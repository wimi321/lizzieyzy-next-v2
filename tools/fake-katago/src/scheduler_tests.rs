#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

fn engine() -> Engine<Vec<u8>> {
    Engine {
        options: Options {
            visits_per_sec: 1000.0,
            ..Options::default()
        },
        output: Vec::new(),
        started: Instant::now(),
        pending: Vec::new(),
        running: Vec::new(),
        sequence: 0,
        queries: 0,
        lines: 0,
        hung: false,
        script: PlayScript::default(),
    }
}

fn query(max_visits: u64, max_time: f64) -> Value {
    json!({
        "id":"play", "moves":[], "rules":"chinese", "komi":7.5,
        "boardXSize":19, "boardYSize":19, "maxVisits":max_visits,
        "overrideSettings":{"maxTime":max_time},
        "reportDuringSearchEvery":1.0, "firstReportDuringSearchAfter":1.0
    })
}

#[test]
fn max_time_and_max_visits_finish_at_the_first_simulated_limit() {
    for (max_visits, seconds, expected_visits) in [(10_000, 2.0, 2000), (1500, 2.0, 1500)] {
        let mut engine = engine();
        engine
            .receive(&query(max_visits, seconds).to_string())
            .unwrap();
        engine.start_pending();
        // Explicitly advance the scheduler's simulated timestamps, without wall time waits.
        engine.running[0].start = 0.0;
        engine.running[0].next_report = 1.0;
        engine.advance_at(0.999).unwrap();
        assert!(engine.output.is_empty());
        engine.advance_at(1.0).unwrap();
        let partial: Value = serde_json::from_slice(&engine.output).unwrap();
        assert_eq!(partial["isDuringSearch"], true);
        assert_eq!(partial["rootInfo"]["visits"], 1000);
        engine.output.clear();
        engine.advance_at(2.0).unwrap();
        let final_result: Value = serde_json::from_slice(&engine.output).unwrap();
        assert_eq!(final_result["isDuringSearch"], false);
        assert_eq!(final_result["rootInfo"]["visits"], expected_visits);
        assert!(engine.running.is_empty());
    }
}

#[test]
fn malformed_max_time_is_an_analysis_error_instead_of_an_unbounded_search() {
    for invalid in [json!(0), json!(-1), json!("2"), Value::Null] {
        let mut engine = engine();
        let mut request = query(10, 2.0);
        request["overrideSettings"]["maxTime"] = invalid;
        engine.receive(&request.to_string()).unwrap();
        let error: Value = serde_json::from_slice(&engine.output).unwrap();
        assert_eq!(error["field"], "maxTime");
        assert!(engine.pending.is_empty());
    }
}
