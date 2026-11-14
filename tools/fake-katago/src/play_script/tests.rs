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
    ] {
        assert!(PlayScript::parse(&value).is_err());
    }
}
