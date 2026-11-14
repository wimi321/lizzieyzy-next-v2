#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[test]
fn script_selects_by_turn_and_preserves_candidate_ratios_and_pv() {
    let script = PlayScript::parse(&json!({
        "default":{"winrate":0.01},
        "turns":{"2":{"winrate":0.7,"scoreLead":3.5,"candidates":[
            {"move":"pass","visitShare":0.8},
            {"move":"D4","visitShare":0.04,"winrate":0.69,"pv":["D4","Q16"]}
        ]}}
    }))
    .unwrap();
    let mut response = json!({"rootInfo":{"winrate":0.5,"scoreLead":0},"moveInfos":[]});
    script.turn(2).unwrap().apply(&mut response, 1000);
    assert_eq!(response["rootInfo"]["winrate"], 0.7);
    assert_eq!(response["rootInfo"]["scoreLead"], 3.5);
    assert_eq!(response["moveInfos"][0]["move"], "pass");
    assert_eq!(response["moveInfos"][0]["visits"], 800);
    assert_eq!(response["moveInfos"][1]["visits"], 40);
    assert_eq!(response["moveInfos"][1]["winrate"], 0.69);
    assert_eq!(response["moveInfos"][1]["pv"], json!(["D4", "Q16"]));
    script.turn(99).unwrap().apply(&mut response, 1000);
    assert_eq!(response["rootInfo"]["winrate"], 0.01);
}

#[test]
fn invalid_scripts_are_rejected_without_fabricating_values() {
    for value in [
        json!([]),
        json!({"turns": []}),
        json!({"turns":{"invalid":{}}}),
        json!({"default":{"winrate":1.1}}),
        json!({"default":{"winrate":-0.1}}),
        json!({"default":{"scoreLead":"bad"}}),
        json!({"default":{"candidates":[{"move":"pass","visitShare":-1}]}}),
        json!({"default":{"gateFile":""}}),
        json!({"default":{"humanPolicy":[-0.1]}}),
        json!({"default":{"humanWinrate":1.1}}),
        json!({"default":{"candidates":[{"move":"pass","humanPrior":2}]}}),
        json!({"profiles":[]}),
    ] {
        assert!(PlayScript::parse(&value).is_err());
    }
}

#[test]
fn human_scripts_override_policy_prior_and_root_without_affecting_normal_queries() {
    let script = PlayScript::parse(&json!({
        "default":{"scoreLead":5.0},
        "profiles":{"rank_3d":{"default":{
            "scoreLead":2.5,"humanPolicy":[0.1,0.2,0.3,0.35,0.05],
            "policy":[0.4,0.2,0.2,0.2,0.0],
            "humanWinrate":0.55,"humanScoreMean":1.0,"humanScoreStdev":15.0,
            "candidates":[{"move":"A2","humanPrior":0.08}]
        }}}
    }))
    .unwrap();
    let request = json!({"id":"human","moves":[],"rules":"chinese", "boardXSize":2,"boardYSize":2,
        "includePolicy":true,"overrideSettings":{"humanSLProfile":"rank_3d"}});
    let mut query = crate::model::Query::parse(&request).unwrap();
    query.human_model = true;
    query.script = script.for_profile(query.human_profile.as_deref()).clone();
    let result = query.response(0, 64, false, crate::cli::ReportPerspective::Black);
    assert_eq!(result["humanPolicy"], json!([0.1, 0.2, 0.3, 0.35, 0.05]));
    assert_eq!(result["policy"], json!([0.4, 0.2, 0.2, 0.2, 0.0]));
    assert_eq!(result["moveInfos"][0]["humanPrior"], 0.08);
    assert_eq!(result["rootInfo"]["humanWinrate"], 0.55);
    assert_eq!(result["rootInfo"]["humanScoreMean"], 1.0);
    assert_eq!(result["rootInfo"]["humanScoreStdev"], 15.0);
    query.human_profile = None;
    query.script = script.for_profile(None).clone();
    let normal = query.response(0, 64, false, crate::cli::ReportPerspective::Black);
    assert_eq!(normal["rootInfo"]["scoreLead"], 5.0);
    assert!(normal.get("humanPolicy").is_none());
}
