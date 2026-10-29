#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::{Duration, Instant};

use common::{Engine, query, stream};
use serde_json::{Value, json};

fn final_response(engine: &Engine) -> Value {
    loop {
        let response = engine.recv();
        if response["isDuringSearch"] == false {
            return response;
        }
    }
}

#[test]
fn visits_per_second_controls_growth_and_finishes_at_max_visits() {
    let mut engine = Engine::spawn(&["--visits-per-sec", "200"]);
    engine.ready();
    let mut request = stream("growth");
    request["maxVisits"] = json!(60);
    let started = Instant::now();
    engine.send(&request);
    let mut previous = 0;
    let mut partials = 0;
    loop {
        let response = engine.recv();
        let visits = response["rootInfo"]["visits"].as_u64().unwrap();
        assert!(visits >= previous);
        assert!(visits <= 60);
        previous = visits;
        if response["isDuringSearch"] == false {
            assert_eq!(visits, 60);
            break;
        }
        partials += 1;
    }
    assert!(partials >= 2);
    assert!(started.elapsed() >= Duration::from_millis(300));
    engine.finish();
}

#[test]
fn time_scale_slows_simulated_search_duration_and_startup_delay() {
    let mut engine = Engine::spawn(&[
        "--visits-per-sec",
        "200",
        "--time-scale",
        "0.5",
        "--startup-delay-ms",
        "80",
    ]);
    engine.ready();
    assert!(engine.started.elapsed() >= Duration::from_millis(160));
    let mut request = query("scaled");
    request["maxVisits"] = json!(40);
    let started = Instant::now();
    engine.send(&request);
    let response = engine.recv();
    assert_eq!(response["rootInfo"]["visits"], 40);
    assert!(started.elapsed() >= Duration::from_millis(400));
    engine.finish();
}

#[test]
fn deterministic_values_depend_on_history_and_turn_but_not_request_id() {
    let mut engine = Engine::spawn(&[]);
    engine.ready();
    let mut request = query("first");
    request["moves"] = json!([["B", "D4"], ["W", "Q16"]]);
    engine.send(&request);
    let mut first = engine.recv();
    request["id"] = json!("second");
    engine.send(&request);
    let mut second = engine.recv();
    first.as_object_mut().unwrap().remove("id");
    second.as_object_mut().unwrap().remove("id");
    assert_eq!(first, second);

    request["id"] = json!("other-history");
    request["moves"] = json!([["B", "Q16"], ["W", "D4"]]);
    engine.send(&request);
    let other = engine.recv();
    assert_ne!(first["moveInfos"], other["moveInfos"]);

    request["id"] = json!("turns");
    request["analyzeTurns"] = json!([0, 1, 2]);
    engine.send(&request);
    let results = [engine.recv(), engine.recv(), engine.recv()];
    assert_eq!(results[0]["turnNumber"], 0);
    assert_eq!(results[1]["turnNumber"], 1);
    assert_eq!(results[2]["turnNumber"], 2);
    assert_ne!(results[0]["moveInfos"], results[1]["moveInfos"]);
    engine.finish();
}

#[test]
fn queued_priority_is_descending_and_equal_priority_is_fifo() {
    let mut engine = Engine::spawn(&["--threads", "1"]);
    engine.ready();
    engine.send(&stream("blocker"));
    assert_eq!(engine.recv()["isDuringSearch"], true);
    for (id, priority) in [
        ("low", -5),
        ("high", 10),
        ("tie-first", 0),
        ("tie-second", 0),
    ] {
        let mut request = query(id);
        request["priority"] = json!(priority);
        engine.send(&request);
    }
    engine.send(&json!({"id":"stop", "action":"terminate", "terminateId":"blocker"}));
    loop {
        if engine.recv()["action"] == "terminate" {
            break;
        }
    }
    assert_eq!(final_response(&engine)["id"], "blocker");
    for expected in ["high", "tie-first", "tie-second", "low"] {
        assert_eq!(engine.recv()["id"], expected);
    }
    engine.finish();
}

#[test]
fn malformed_and_illegal_requests_do_not_stop_following_queries() {
    let mut engine = Engine::spawn(&[]);
    engine.ready();
    for malformed in ["{", "[]", "null", "{\"id\":\"bad\",\"action\":\"unknown\"}"] {
        engine.send_raw(malformed);
        assert!(engine.recv()["error"].is_string());
    }
    for coordinate in ["I4", "T20", "A0", "wrong"] {
        let mut request = query("illegal");
        request["moves"] = json!([["B", coordinate]]);
        engine.send(&request);
        let error = engine.recv();
        assert_eq!(error["id"], "illegal");
        assert_eq!(error["field"], "moves");
        assert!(error["error"].is_string());
    }
    engine.send(&query("valid"));
    assert_eq!(engine.recv()["id"], "valid");
    engine.finish();
}

#[test]
fn ownership_and_policy_have_board_dimensions_and_respect_pass_history() {
    let mut engine = Engine::spawn(&[]);
    let mut request = query("rectangular");
    request["boardXSize"] = json!(9);
    request["boardYSize"] = json!(7);
    request["initialStones"] = json!([["B", "D4"]]);
    request["initialPlayer"] = json!("W");
    request["moves"] = json!([["W", "pass"]]);
    request["includeOwnership"] = json!(true);
    request["includePolicy"] = json!(true);
    engine.send(&request);
    let response = engine.recv();
    assert_eq!(response["turnNumber"], 1);
    assert_eq!(response["rootInfo"]["currentPlayer"], "B");
    assert_eq!(response["ownership"].as_array().unwrap().len(), 63);
    assert_eq!(response["policy"].as_array().unwrap().len(), 64);
    assert!(
        response["moveInfos"]
            .as_array()
            .unwrap()
            .iter()
            .all(|candidate| candidate["move"] != "D4")
    );
    engine.finish();
}

#[test]
fn captures_remove_stones_and_allow_the_vacated_point_to_be_played() {
    let mut engine = Engine::spawn(&[]);
    let mut request = query("capture");
    request["boardXSize"] = json!(5);
    request["boardYSize"] = json!(5);
    request["initialStones"] = json!([["B", "A2"], ["B", "B1"], ["B", "C2"], ["W", "B2"]]);
    request["moves"] = json!([["B", "B3"], ["W", "E5"], ["B", "B2"]]);
    engine.send(&request);
    let response = engine.recv();
    assert!(response.get("error").is_none());
    assert_eq!(response["turnNumber"], 3);
    assert_eq!(response["rootInfo"]["currentPlayer"], "W");
    engine.finish();
}

#[test]
fn suicide_and_immediate_simple_ko_return_move_errors() {
    let mut engine = Engine::spawn(&[]);
    let mut suicide = query("suicide");
    suicide["boardXSize"] = json!(5);
    suicide["boardYSize"] = json!(5);
    suicide["initialStones"] = json!([["B", "A2"], ["B", "B1"], ["B", "C2"], ["B", "B3"]]);
    suicide["moves"] = json!([["W", "B2"]]);
    engine.send(&suicide);
    let response = engine.recv();
    assert_eq!(response["id"], "suicide");
    assert_eq!(response["field"], "moves");
    assert!(response["error"].is_string());

    let mut ko = query("ko");
    ko["boardXSize"] = json!(5);
    ko["boardYSize"] = json!(5);
    ko["initialStones"] = json!([
        ["B", "A2"],
        ["B", "B1"],
        ["B", "B3"],
        ["W", "B2"],
        ["W", "C1"],
        ["W", "C3"],
        ["W", "D2"]
    ]);
    ko["moves"] = json!([["B", "C2"], ["W", "B2"]]);
    engine.send(&ko);
    let response = engine.recv();
    assert_eq!(response["id"], "ko");
    assert_eq!(response["field"], "moves");
    assert!(response["error"].is_string());
    engine.finish();
}

#[test]
fn handicap_setup_infers_white_only_with_multiple_black_stones() {
    let mut engine = Engine::spawn(&[]);
    for (id, stones, expected) in [
        ("empty", json!([]), "B"),
        ("one-black", json!([["B", "D4"]]), "B"),
        ("handicap", json!([["B", "D4"], ["B", "Q16"]]), "W"),
        (
            "mixed",
            json!([["B", "D4"], ["B", "Q16"], ["W", "D16"]]),
            "B",
        ),
        ("white-only", json!([["W", "D4"], ["W", "Q16"]]), "B"),
    ] {
        let mut request = query(id);
        request["initialStones"] = stones;
        engine.send(&request);
        let response = engine.recv();
        assert_eq!(response["id"], id);
        assert_eq!(response["rootInfo"]["currentPlayer"], expected);
    }
    let mut request = query("explicit-player");
    request["initialStones"] = json!([["B", "D4"], ["B", "Q16"]]);
    request["initialPlayer"] = json!("B");
    engine.send(&request);
    assert_eq!(engine.recv()["rootInfo"]["currentPlayer"], "B");

    request.as_object_mut().unwrap().remove("initialPlayer");
    request["id"] = json!("first-move-player");
    request["moves"] = json!([["B", "D16"]]);
    request["analyzeTurns"] = json!([0]);
    engine.send(&request);
    assert_eq!(engine.recv()["rootInfo"]["currentPlayer"], "B");
    engine.finish();
}

#[test]
fn first_report_without_periodic_interval_emits_one_partial_then_final() {
    let mut engine = Engine::spawn(&["--visits-per-sec", "200"]);
    let mut request = query("one-report");
    request["maxVisits"] = json!(60);
    request["firstReportDuringSearchAfter"] = json!(0.01);
    engine.send(&request);
    let partial = engine.recv();
    assert_eq!(partial["id"], "one-report");
    assert_eq!(partial["isDuringSearch"], true);
    assert!(partial["rootInfo"]["visits"].as_u64().unwrap() < 60);
    let final_result = engine.recv();
    assert_eq!(final_result["id"], "one-report");
    assert_eq!(final_result["isDuringSearch"], false);
    assert_eq!(final_result["rootInfo"]["visits"], 60);
    engine.finish();
}

#[test]
fn priority_and_priorities_together_are_rejected_without_poisoning_the_process() {
    let mut engine = Engine::spawn(&[]);
    let mut request = query("conflicting-priorities");
    request["analyzeTurns"] = json!([0]);
    request["priority"] = json!(0);
    request["priorities"] = json!([0]);
    engine.send(&request);
    let error = engine.recv();
    assert_eq!(error["id"], "conflicting-priorities");
    assert_eq!(error["field"], "priority");
    assert!(error["error"].is_string());

    request.as_object_mut().unwrap().remove("priority");
    request["id"] = json!("valid-priorities");
    engine.send(&request);
    assert_eq!(engine.recv()["id"], "valid-priorities");
    engine.finish();
}

#[test]
fn analyze_turns_is_a_sorted_set_and_duplicate_priorities_use_the_last_value() {
    let mut engine = Engine::spawn(&["--threads", "1"]);
    let mut request = query("deduplicated");
    request["moves"] = json!([["B", "D4"], ["W", "Q16"]]);
    request["analyzeTurns"] = json!([2, 0, 2, 1]);
    engine.send(&request);
    for turn in [0, 1, 2] {
        let response = engine.recv();
        assert_eq!(response["id"], "deduplicated");
        assert_eq!(response["turnNumber"], turn);
        assert_eq!(response["isDuringSearch"], false);
    }

    request["id"] = json!("last-priority");
    request["priorities"] = json!([100, 0, -10, 10]);
    engine.send(&request);
    for turn in [1, 0, 2] {
        let response = engine.recv();
        assert_eq!(response["id"], "last-priority");
        assert_eq!(response["turnNumber"], turn);
        assert_eq!(response["isDuringSearch"], false);
    }
    engine.finish();
}
