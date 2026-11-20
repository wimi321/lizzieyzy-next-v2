#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{Engine, query};
use serde_json::json;

#[test]
fn instant_response_handles_multiple_turns_and_subsequent_positions() {
    // Without the opt-in flag these queries require days of simulated time.
    // The shared five-second timeout is only a deadlock safety net.
    let mut engine = Engine::spawn(&["--instant-response", "--visits-per-sec", "0.001"]);
    engine.ready();
    let mut request = query("initial");
    request["maxVisits"] = json!(1000);
    request["includeOwnership"] = json!(true);
    request["reportDuringSearchEvery"] = json!(1000.0);
    request["firstReportDuringSearchAfter"] = json!(1000.0);
    engine.send(&request);
    let initial = engine.recv();
    assert_eq!(initial["id"], "initial");
    assert_eq!(initial["turnNumber"], 0);
    assert_eq!(initial["isDuringSearch"], false);
    assert_eq!(initial["rootInfo"]["visits"], 1000);
    assert_eq!(initial["ownership"].as_array().unwrap().len(), 361);

    request["id"] = json!("after-move");
    request["moves"] = json!([["B", "D4"]]);
    request["analyzeTurns"] = json!([0, 1]);
    engine.send(&request);
    for turn in [0, 1] {
        let result = engine.recv();
        assert_eq!(result["id"], "after-move");
        assert_eq!(result["turnNumber"], turn);
        assert_eq!(result["isDuringSearch"], false);
        assert_eq!(result["rootInfo"]["visits"], 1000);
    }
    engine.finish();
}
