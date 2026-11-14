#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{Engine, query};
use serde_json::{Value, json};

#[test]
fn extras_are_requested_independently_and_policy_keeps_illegal_points_and_pass() {
    let mut engine = Engine::spawn(&[]);
    engine.ready();
    let mut request = query("extras");
    request["boardXSize"] = json!(5);
    request["boardYSize"] = json!(3);
    request["maxVisits"] = json!(1);
    request["moves"] = json!([["B", "A3"]]);
    for (policy, moves_ownership) in [
        (None, None),
        (Some(false), Some(false)),
        (Some(true), Some(false)),
        (Some(false), Some(true)),
        (Some(true), Some(true)),
    ] {
        for (field, enabled) in [
            ("includePolicy", policy),
            ("includeMovesOwnership", moves_ownership),
        ] {
            if let Some(enabled) = enabled {
                request[field] = json!(enabled);
            } else {
                request.as_object_mut().unwrap().remove(field);
            }
        }
        engine.send(&request);
        let response = engine.recv();
        assert_eq!(response["rootInfo"]["visits"], 1);
        assert_eq!(response["rootInfo"]["currentPlayer"], "W");
        assert!(response.get("ownership").is_none());
        if policy == Some(true) {
            let values = response["policy"].as_array().unwrap();
            assert_eq!(values.len(), 16);
            assert_eq!(values[0], -1.0);
            assert!(values[15].as_f64().unwrap() > 0.0);
            let sum: f64 = values
                .iter()
                .filter_map(Value::as_f64)
                .filter(|p| *p >= 0.0)
                .sum();
            assert!((sum - 1.0).abs() < 1e-12);
        } else {
            assert!(response.get("policy").is_none());
        }
        for candidate in response["moveInfos"].as_array().unwrap() {
            if moves_ownership == Some(true) {
                let values = candidate["ownership"].as_array().unwrap();
                assert_eq!(values.len(), 15);
                assert_eq!(values[0], 1.0);
                let coordinate = candidate["move"].as_str().unwrap();
                let x = usize::from(coordinate.as_bytes()[0] - b'A');
                let y = 3 - coordinate[1..].parse::<usize>().unwrap();
                assert_eq!(
                    values[y * 5 + x],
                    -1.0,
                    "White's candidate is placed on its ownership board"
                );
            } else {
                assert!(candidate.get("ownership").is_none());
            }
        }
    }
    request["includeMovesOwnership"] = json!("invalid");
    engine.send(&request);
    assert_eq!(engine.recv()["field"], "includeMovesOwnership");
    engine.finish();
}
