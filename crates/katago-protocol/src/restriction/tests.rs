#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use serde_json::json;

#[test]
fn typed_restrictions_round_trip_wire_and_preserve_unrelated_extensions() {
    let mut query: AnalysisQuery = serde_json::from_value(json!({"id":"q", "moves":[], "rules":"chinese", "boardXSize":19, "boardYSize":19, "futureField":42})).unwrap();
    for allow in [true, false] {
        let entries = vec![MoveRestriction {
            player: Player::White,
            moves: vec!["D4".into(), "pass".into()],
            until_depth: 5,
        }];
        let restrictions = MoveRestrictions {
            allow_moves: allow.then(|| entries.clone()),
            avoid_moves: (!allow).then_some(entries),
        };
        query.set_move_restrictions(&restrictions).unwrap();
        let wire = serde_json::to_value(&query).unwrap();
        let key = if allow { "allowMoves" } else { "avoidMoves" };
        assert_eq!(
            wire[key],
            json!([{"player":"W", "moves":["D4","pass"], "untilDepth":5}])
        );
        assert!(
            wire.get(if allow { "avoidMoves" } else { "allowMoves" })
                .is_none()
        );
        assert_eq!(wire["futureField"], 42);
        let parsed: AnalysisQuery = serde_json::from_value(wire).unwrap();
        assert_eq!(parsed.move_restrictions().unwrap(), restrictions);
    }
    query
        .set_move_restrictions(&MoveRestrictions::default())
        .unwrap();
    assert_eq!(query.extensions.len(), 1);
}

#[test]
fn malformed_restrictions_are_rejected_before_mutating_query() {
    let entry = MoveRestriction {
        player: Player::Black,
        moves: vec!["D4".into()],
        until_depth: 1,
    };
    let mut extensions = BTreeMap::from([("future".into(), json!(1))]);
    let invalid = MoveRestrictions {
        allow_moves: Some(vec![entry.clone(), entry]),
        avoid_moves: None,
    };
    assert!(matches!(
        invalid.write_extensions(&mut extensions),
        Err(RestrictionError::DuplicatePlayer)
    ));
    assert_eq!(extensions, BTreeMap::from([("future".into(), json!(1))]));
    extensions.insert(
        "avoidMoves".into(),
        json!([{"player":"W","moves":[],"untilDepth":0}]),
    );
    assert!(matches!(
        MoveRestrictions::from_extensions(&extensions),
        Err(RestrictionError::InvalidDepth)
    ));
}
