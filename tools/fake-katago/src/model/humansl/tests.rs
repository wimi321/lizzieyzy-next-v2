#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;

fn request(profile: Option<&str>) -> Value {
    let mut value = json!({
        "id":"q", "moves":[["B","A3"]], "rules":"chinese", "boardXSize":3,
        "boardYSize":3, "includePolicy":true, "maxVisits":64,
    });
    if let Some(profile) = profile {
        value["overrideSettings"] = json!({"humanSLProfile":profile});
    }
    value
}

fn response(value: Value, loaded: bool, perspective: ReportPerspective) -> Value {
    let mut query = Query::parse(&value).unwrap();
    query.human_model = loaded;
    query.response(1, 64, false, perspective)
}

#[test]
fn loaded_model_only_changes_humansl_queries_and_profiles_are_deterministic() {
    let normal = response(request(None), false, ReportPerspective::Black);
    assert_eq!(
        normal,
        response(request(None), true, ReportPerspective::Black)
    );
    assert!(normal.get("humanPolicy").is_none());
    assert!(normal["rootInfo"].get("humanScoreStdev").is_none());
    let human = response(request(Some("rank_3d")), true, ReportPerspective::Black);
    assert_eq!(
        human,
        response(request(Some("rank_3d")), true, ReportPerspective::Black)
    );
    let other = response(request(Some("rank_20k")), true, ReportPerspective::Black);
    assert_ne!(human["humanPolicy"], other["humanPolicy"]);
    let probabilities = human["humanPolicy"].as_array().unwrap();
    assert_eq!(probabilities.len(), 10);
    assert_eq!(probabilities[0], -1.0);
    let sum: f64 = probabilities
        .iter()
        .filter_map(Value::as_f64)
        .filter(|v| *v >= 0.0)
        .sum();
    assert!((sum - 1.0).abs() < 1e-12);
    for info in human["moveInfos"].as_array().unwrap() {
        assert!(info["humanPrior"].as_f64().is_some());
    }
}

#[test]
fn human_probabilities_keep_point_order_but_evaluations_follow_perspective() {
    let black = response(request(Some("rank_9d")), true, ReportPerspective::Black);
    let white = response(request(Some("rank_9d")), true, ReportPerspective::ToPlay);
    assert_eq!(black["humanPolicy"], white["humanPolicy"]);
    assert_eq!(
        black["moveInfos"][0]["humanPrior"],
        white["moveInfos"][0]["humanPrior"]
    );
    assert_eq!(
        white["rootInfo"]["humanScoreMean"].as_f64(),
        black["rootInfo"]["humanScoreMean"].as_f64().map(|v| -v)
    );
    assert_eq!(
        white["rootInfo"]["humanWinrate"].as_f64(),
        black["rootInfo"]["humanWinrate"].as_f64().map(|v| 1.0 - v)
    );
    assert_eq!(
        black["rootInfo"]["humanScoreStdev"],
        white["rootInfo"]["humanScoreStdev"]
    );
}

#[test]
fn allow_moves_searches_requested_legal_candidates_and_pass_for_matching_player() {
    let mut value = request(Some("rank_6d"));
    value["allowMoves"] = json!([{"player":"W", "moves":["A3","B2","C1","pass"], "untilDepth":1}]);
    let result = response(value.clone(), true, ReportPerspective::Black);
    let mut moves: Vec<_> = result["moveInfos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["move"].as_str().unwrap())
        .collect();
    moves.sort();
    assert_eq!(moves, ["B2", "C1", "pass"]);
    value["allowMoves"][0]["player"] = json!("B");
    assert_eq!(
        response(value, true, ReportPerspective::Black),
        response(request(Some("rank_6d")), true, ReportPerspective::Black)
    );
}

#[test]
fn malformed_humansl_profiles_and_allowances_are_typed_errors() {
    for profile in ["rank_0k", "rank_21k", "rank_10d", "rank_", ""] {
        assert_eq!(
            Query::parse(&request(Some(profile))).err().unwrap().field,
            "humanSLProfile"
        );
    }
    for allowance in [
        json!({}),
        json!([{"player":"W","moves":["Z99"],"untilDepth":1}]),
        json!([{"player":"W","moves":[],"untilDepth":-1}]),
    ] {
        let mut value = request(None);
        value["allowMoves"] = allowance;
        assert_eq!(Query::parse(&value).err().unwrap().field, "allowMoves");
    }
}
