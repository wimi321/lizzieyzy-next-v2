#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;
use proptest::prelude::*;
use serde_json::{Value, json};
use std::path::PathBuf;

fn recordings() -> Vec<(String, Vec<Value>)> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/katago");
    let mut paths: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "golden recordings must exist");
    paths
        .into_iter()
        .map(|path| {
            let records = std::fs::read_to_string(&path)
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            (path.display().to_string(), records)
        })
        .collect()
}

#[test]
fn every_recorded_output_parses_and_preserves_known_fields() {
    let mut outputs = 0;
    for (name, records) in recordings() {
        let mut scenario_outputs = 0;
        for record in records.iter().filter(|record| record["dir"] == "out") {
            let line = record["line"].as_str().unwrap();
            let parsed = parse_line(line).unwrap_or_else(|error| panic!("{name}: {error}"));
            let original: Value = serde_json::from_str(line).unwrap();
            assert_eq!(serde_json::to_value(parsed).unwrap(), original, "{name}");
            outputs += 1;
            scenario_outputs += 1;
        }
        assert!(scenario_outputs > 0, "{name} has no recorded output");
    }
    assert!(outputs >= 46, "all currently recorded outputs were visited");
}

#[test]
fn every_recorded_request_round_trips_semantically() {
    for (name, records) in recordings() {
        let mut requests = 0;
        for record in records.iter().filter(|record| record["dir"] == "in") {
            let line = record["line"].as_str().unwrap();
            let parsed: Request =
                serde_json::from_str(line).unwrap_or_else(|error| panic!("{name}: {error}"));
            let original: Value = serde_json::from_str(line).unwrap();
            assert_eq!(serde_json::to_value(parsed).unwrap(), original, "{name}");
            requests += 1;
        }
        assert!(requests > 0, "{name} has no recorded request");
    }
}

fn minimal_analysis() -> Value {
    json!({
        "id":"analysis", "turnNumber":0, "isDuringSearch":false,
        "moveInfos":[{
            "move":"D4", "visits":1, "winrate":0.4, "scoreLead":-2.0,
            "prior":0.1, "order":0, "pv":["D4","pass"]
        }],
        "rootInfo":{
            "currentPlayer":"B", "visits":1, "winrate":0.4,
            "scoreLead":-2.0, "utility":-0.2
        }
    })
}

#[test]
fn unknown_response_fields_are_ignored_at_all_levels() {
    let known = minimal_analysis();
    let expected = parse_line(&known.to_string()).unwrap();
    let mut extended = known;
    extended["futureRoot"] = json!({"nested":[1,2,3]});
    extended["rootInfo"]["newStatistic"] = json!(0.5);
    extended["moveInfos"][0]["futureMove"] = json!(["future"]);
    assert_eq!(parse_line(&extended.to_string()).unwrap(), expected);

    for line in [
        r#"{"error":"bad query","future":42}"#,
        r#"{"warning":"unused","id":"x","field":"unused","future":42}"#,
        r#"{"action":"query_version","id":"x","version":"1","git_hash":"x","future":42}"#,
        r#"{"id":"x","turnNumber":0,"isDuringSearch":false,"noResults":true,"future":42}"#,
    ] {
        let result = serde_json::to_value(parse_line(line).unwrap()).unwrap();
        assert!(result.get("future").is_none());
    }
}

#[test]
fn cancellation_result_is_distinct_from_analysis() {
    let result =
        parse_line(r#"{"id":"queued","turnNumber":2,"isDuringSearch":false,"noResults":true}"#)
            .unwrap();
    let EngineLine::NoResults(response) = result else {
        panic!("queued cancellation should not fabricate analysis data");
    };
    assert_eq!(response.id, "queued");
    assert_eq!(response.turn_number, 2);
    assert!(response.no_results);
    assert!(!response.is_during_search);
}

#[test]
fn errors_warnings_and_action_acknowledgements_are_distinct() {
    assert!(matches!(
        parse_line(r#"{"error":"bad JSON"}"#).unwrap(),
        EngineLine::Error(ErrorLine {
            id: None,
            field: None,
            ..
        })
    ));
    assert!(matches!(
        parse_line(r#"{"warning":"unused field","id":"q","field":"unused"}"#).unwrap(),
        EngineLine::Warning(_)
    ));
    for (line, expected) in [
        (
            r#"{"action":"terminate","id":"stop","terminateId":"q"}"#,
            ActionResponse::Terminate {
                id: "stop".into(),
                terminate_id: "q".into(),
            },
        ),
        (
            r#"{"action":"terminate_all","id":"stop"}"#,
            ActionResponse::TerminateAll { id: "stop".into() },
        ),
        (
            r#"{"action":"query_version","id":"v","version":"1.16.5","git_hash":"hash"}"#,
            ActionResponse::QueryVersion {
                id: "v".into(),
                version: "1.16.5".into(),
                git_hash: "hash".into(),
            },
        ),
    ] {
        assert_eq!(parse_line(line).unwrap(), EngineLine::Action(expected));
    }
}

#[test]
fn invalid_lines_and_incomplete_responses_return_errors() {
    let invalid = [
        "",
        "not JSON",
        "null",
        "[]",
        "42",
        "true",
        "{}",
        "{",
        "{} {}",
        r#"{"error":42}"#,
        r#"{"warning":null}"#,
        r#"{"action":"future_action","id":"x"}"#,
        r#"{"action":"terminate","id":"x"}"#,
        r#"{"action":"query_version","id":"x","version":"1"}"#,
        r#"{"id":"x","turnNumber":0,"isDuringSearch":false}"#,
        r#"{"id":"x","turnNumber":0,"isDuringSearch":false,"noResults":false}"#,
        r#"{"id":"x","turnNumber":0,"isDuringSearch":false,"noResults":"true"}"#,
        r#"{"id":"x","turnNumber":0,"isDuringSearch":true,"noResults":true}"#,
        r#"{"id":"x","turnNumber":0,"isDuringSearch":false,"noResults":true,"moveInfos":[]}"#,
        r#"{"id":"x","turnNumber":0,"isDuringSearch":false,"noResults":true,"rootInfo":{}}"#,
        r#"{"id":"x","turnNumber":-1,"isDuringSearch":false,"noResults":true}"#,
        r#"{"error":"bad","id":[]}"#,
        r#"{"error":"bad","field":{}}"#,
    ];
    for line in invalid {
        assert!(parse_line(line).is_err(), "accepted {line}");
    }
    let deeply_nested = format!("{}0{}", "[".repeat(2000), "]".repeat(2000));
    assert!(parse_line(&deeply_nested).is_err());
    let mut invalid_analysis = minimal_analysis();
    invalid_analysis["rootInfo"]["visits"] = json!(-1);
    assert!(parse_line(&invalid_analysis.to_string()).is_err());
    invalid_analysis["rootInfo"]["visits"] = json!(1);
    invalid_analysis["moveInfos"][0]["winrate"] = json!("NaN");
    assert!(parse_line(&invalid_analysis.to_string()).is_err());
}

#[test]
fn malformed_action_cannot_fall_back_to_analysis_request() {
    let mut value = json!({
        "id":"q", "moves":[], "rules":"chinese", "boardXSize":19,
        "boardYSize":19, "action":"unknown"
    });
    assert!(serde_json::from_value::<Request>(value.clone()).is_err());
    value["action"] = json!("terminate");
    assert!(serde_json::from_value::<Request>(value.clone()).is_err());
    value["action"] = Value::Null;
    assert!(serde_json::from_value::<Request>(value).is_err());
}

#[test]
fn optional_query_fields_and_custom_rules_preserve_wire_shape() {
    let value = json!({
        "id":"handicap", "moves":[["W","pass"]], "initialStones":[["B","D4"]],
        "initialPlayer":"W", "rules":{"ko":"SITUATIONAL","scoring":"AREA"},
        "komi":0.5, "boardXSize":9, "boardYSize":13, "analyzeTurns":[0,1],
        "maxVisits":250, "includeOwnership":false, "includePolicy":true,
        "reportDuringSearchEvery":0.15, "firstReportDuringSearchAfter":0.01,
        "priority":-5, "priorities":[1,2],
        "overrideSettings":{"reportAnalysisWinratesAs":"BLACK","cpuctExploration":1.2},
        "futureOption":{"enabled":true}
    });
    let request: Request = serde_json::from_value(value.clone()).unwrap();
    let Request::Analysis(query) = &request else {
        panic!("expected query")
    };
    assert!(matches!(query.rules, Rules::Custom(_)));
    assert_eq!(query.initial_player, Some(Player::White));
    assert_eq!(query.extensions["futureOption"], json!({"enabled":true}));
    assert_eq!(serde_json::to_value(request).unwrap(), value);

    let minimal = json!({"id":"q","moves":[],"rules":"chinese","boardXSize":19,"boardYSize":19});
    let query: AnalysisQuery = serde_json::from_value(minimal.clone()).unwrap();
    assert_eq!(query.max_visits, None);
    assert_eq!(query.komi, None);
    assert_eq!(serde_json::to_value(query).unwrap(), minimal);
}

proptest! {
    #[test]
    fn arbitrary_text_never_panics(text in prop::collection::vec(any::<char>(), 0..2048)) {
        let text: String = text.into_iter().collect();
        let _ = parse_line(&text);
    }

    #[test]
    fn query_round_trip(
        id in ".{0,64}",
        width in 2_u8..=25,
        height in 2_u8..=25,
        komi in -1000.0_f64..1000.0,
        visits in prop::option::of(1_u64..u64::MAX),
        turns in prop::collection::vec(0_u32..1000, 0..20),
        moves in prop::collection::vec((any::<bool>(), "[A-HJ-Z][1-9]|pass"), 0..30),
        priority in any::<i64>(),
    ) {
        let query = AnalysisQuery {
            id, moves: moves.into_iter().map(|(black, coordinate)|
                (if black {Player::Black} else {Player::White}, coordinate)).collect(),
            rules: Rules::Named("chinese".into()), board_x_size: width, board_y_size: height,
            initial_stones: Some(vec![(Player::Black,"D4".into())]), initial_player: Some(Player::White),
            komi: Some(komi), analyze_turns: Some(turns), max_visits: visits,
            include_ownership: Some(true), include_policy: Some(false),
            report_during_search_every: Some(0.15), first_report_during_search_after: None,
            priority: Some(priority), priorities: None, override_settings: None,
            extensions: Default::default(),
        };
        let wire = serde_json::to_string(&query).unwrap();
        prop_assert_eq!(serde_json::from_str::<AnalysisQuery>(&wire).unwrap(), query);
    }

    #[test]
    fn analysis_round_trip_preserves_evaluations(
        visits in 1_u64..u64::MAX, winrate in 0.0_f64..=1.0,
        score in -1000.0_f64..1000.0, during in any::<bool>(), turn in any::<u32>(),
    ) {
        let mut value = minimal_analysis();
        value["rootInfo"]["visits"] = json!(visits);
        value["rootInfo"]["winrate"] = json!(winrate);
        value["rootInfo"]["scoreLead"] = json!(score);
        value["moveInfos"][0]["scoreLead"] = json!(score);
        value["isDuringSearch"] = json!(during);
        value["turnNumber"] = json!(turn);
        let parsed = parse_line(&value.to_string()).unwrap();
        let serialized = serde_json::to_string(&parsed).unwrap();
        prop_assert_eq!(parse_line(&serialized).unwrap(), parsed);
        prop_assert_eq!(serde_json::from_str::<Value>(&serialized).unwrap(), value);
    }
}
