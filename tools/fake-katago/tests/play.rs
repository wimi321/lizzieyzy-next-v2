#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{Engine, TempDirectory, query};
use serde_json::{Value, json};

#[test]
fn bounded_seconds_visits_and_single_evaluation_all_finish() {
    let mut engine = Engine::spawn(&["--time-scale", "1000", "--visits-per-sec", "1000"]);
    engine.ready();
    for (visits, seconds, expected) in [(10_000, 2.0, 2000), (1500, 2.0, 1500), (1, 2.0, 1)] {
        let mut request = query("bounded");
        request["maxVisits"] = json!(visits);
        request["overrideSettings"] = json!({"maxTime":seconds});
        engine.send(&request);
        let response = engine.recv();
        assert_eq!(response["isDuringSearch"], false);
        assert_eq!(response["rootInfo"]["visits"], expected);
    }
    engine.finish();
}

#[test]
fn per_turn_script_overrides_pass_candidates_and_black_perspective_evaluations() {
    let directory = TempDirectory::new("play-script");
    let path = directory.path().join("script.json");
    std::fs::write(
        &path,
        json!({
            "default":{"winrate":0.01,"scoreLead":-30.0},
            "turns":{
                "0":{"winrate":0.6,"scoreLead":4.5,"candidates":[
                    {"move":"pass","visitShare":0.8},
                    {"move":"D4","winrate":0.599,"visitShare":0.04,"pv":["D4","Q16"]}
                ]},
                "1":{"winrate":0.8,"scoreLead":9.5},
                "2":{"winrate":0.5,"scoreLead":0.0}
            }
        })
        .to_string(),
    )
    .unwrap();
    let mut engine = Engine::spawn(&["--play-script", path.to_str().unwrap()]);
    engine.ready();
    let mut request = query("scripted");
    request["maxVisits"] = json!(1000);
    engine.send(&request);
    let response = engine.recv();
    assert_eq!(response["moveInfos"][0]["move"], "pass");
    assert_eq!(response["moveInfos"][0]["visits"], 800);
    assert_eq!(response["moveInfos"][1]["visits"], 40);
    assert_eq!(response["moveInfos"][1]["pv"], json!(["D4", "Q16"]));
    request["maxVisits"] = json!(1);
    for (moves, winrate, score, player) in [
        (json!([["B", "pass"]]), 0.8, 9.5, "W"),
        (json!([["B", "pass"], ["W", "pass"]]), 0.5, 0.0, "B"),
        (
            json!([["B", "pass"], ["W", "pass"], ["B", "pass"]]),
            0.01,
            -30.0,
            "W",
        ),
    ] {
        request["moves"] = moves;
        engine.send(&request);
        let response = engine.recv();
        assert_eq!(response["rootInfo"]["winrate"], winrate);
        assert_eq!(response["rootInfo"]["scoreLead"], score);
        assert_eq!(response["rootInfo"]["currentPlayer"], player);
    }
    engine.finish();
}

#[test]
fn scripted_gate_blocks_search_but_not_commands_and_error_is_local_to_a_query() {
    let directory = TempDirectory::new("play-gate");
    let path = directory.path().join("script.json");
    let gate = directory.path().join("released");
    std::fs::write(
        &path,
        json!({"turns":{
            "0":{"gateFile":gate,"candidates":[{"move":"pass"}]},
            "1":{"error":"score unavailable"},
            "2":{"hang":true}
        }})
        .to_string(),
    )
    .unwrap();
    let mut engine = Engine::spawn(&["--play-script", path.to_str().unwrap()]);
    engine.ready();
    let mut request = query("gated");
    request["maxVisits"] = json!(1);
    engine.send(&request);
    // The ordered action reply proves the earlier search was received and is
    // still blocked; an incorrectly emitted final would fail ready().
    engine.ready();
    std::fs::write(&gate, []).unwrap();
    assert_eq!(engine.recv()["moveInfos"][0]["move"], "pass");
    request["moves"] = json!([["B", "pass"]]);
    engine.send(&request);
    let error = engine.recv();
    assert_eq!(error["id"], "gated");
    assert_eq!(error["error"], "score unavailable");
    engine.ready();
    request["id"] = json!("hanging");
    request["moves"] = json!([["B", "pass"], ["W", "pass"]]);
    engine.send(&request);
    engine.ready();
    engine.send(&json!({"id":"cancel","action":"terminate","terminateId":"hanging"}));
    assert_eq!(engine.recv()["id"], "cancel");
    let final_result: Value = engine.recv();
    assert_eq!(final_result["id"], "hanging");
    assert_eq!(final_result["isDuringSearch"], false);
    engine.finish();
}
