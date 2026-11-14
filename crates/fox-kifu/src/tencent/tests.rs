#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use crate::{FoxColor, FoxResultKind, FoxScoreUnit};
use serde_json::{Value, json};

// Recorded 2026-11-14 from TXWQFetchChessList (two pages) and TXWQFetchChess.
// List recordings are cropped to five games; names and UIDs are consistently
// anonymized across lists and SGF. The detail is an older game of the same
// public account and is not one of the recent list entries.
pub(super) const GAMES: &str = include_str!("../../../../fixtures/tencent/games.json");
pub(super) const NEXT: &str = include_str!("../../../../fixtures/tencent/games-next.json");
pub(super) const SGF: &str = include_str!("../../../../fixtures/tencent/sgf.json");

fn recorded() -> Value {
    serde_json::from_str(GAMES).unwrap()
}

#[test]
fn recorded_short_pages_keep_advancing_until_an_empty_page() {
    let first = parse::page(&recorded(), "0").unwrap();
    assert_eq!(first.games.len(), 5);
    assert_eq!(first.next_cursor.as_deref(), Some("810004"));
    let next = parse::page(&serde_json::from_str(NEXT).unwrap(), "810004").unwrap();
    assert_eq!(next.games.len(), 5);
    assert_eq!(next.next_cursor.as_deref(), Some("810054"));
    assert_ne!(first.games[0].id, next.games[0].id);
    assert!(parse::page(&recorded(), "810004").is_err());
    let empty = parse::page(&json!({"result":0,"chesslist":[]}), "810054").unwrap();
    assert!(empty.games.is_empty());
    assert!(empty.next_cursor.is_none());
}

#[test]
fn full_page_limit_and_repeated_cursor_are_checked() {
    let mut data = recorded();
    let row = data["chesslist"][0].clone();
    data["chesslist"] = (0..100)
        .map(|i| {
            let mut row = row.clone();
            row["chessid"] = json!(format!("game-{i}"));
            row
        })
        .collect::<Vec<_>>()
        .into();
    assert_eq!(
        parse::page(&data, "0").unwrap().next_cursor.as_deref(),
        Some("game-99")
    );
    assert!(matches!(
        parse::page(&data, "game-99"),
        Err(TencentError::Common(FoxError::InvalidResponse(
            "non-advancing cursor"
        )))
    ));
    data["chesslist"].as_array_mut().unwrap().push(row);
    assert!(parse::page(&data, "0").is_err());
}

#[test]
fn ranks_are_structured_and_cover_all_three_java_ranges() {
    assert_eq!(TencentRank::from_raw(17), TencentRank::Kyu(1));
    assert_eq!(TencentRank::from_raw(0), TencentRank::Kyu(18));
    assert_eq!(TencentRank::from_raw(18), TencentRank::Dan(1));
    assert_eq!(TencentRank::from_raw(26), TencentRank::Dan(9));
    assert_eq!(TencentRank::from_raw(100), TencentRank::Professional(1));
    assert_eq!(TencentRank::from_raw(108), TencentRank::Professional(9));
    assert_eq!(
        TencentRank::from_raw(i32::MIN),
        TencentRank::Kyu(2147483666)
    );
    assert_eq!(
        serde_json::to_value(TencentRank::Dan(3)).unwrap(),
        json!({"type":"Dan","data":3})
    );
}

#[test]
fn result_kinds_and_name_precedence_follow_the_wire_contract() {
    for (winner, point, rule, kind, score, unit) in [
        (
            1,
            250,
            0,
            FoxResultKind::Score,
            Some(2.5),
            Some(FoxScoreUnit::Points),
        ),
        (
            2,
            125,
            1,
            FoxResultKind::Score,
            Some(1.25),
            Some(FoxScoreUnit::Stones),
        ),
        (1, -1, 1, FoxResultKind::Resignation, None, None),
        (2, -2, 1, FoxResultKind::Timeout, None, None),
    ] {
        let mut data = recorded();
        let row = &mut data["chesslist"][0];
        row["winner"] = json!(winner);
        row["point"] = json!(point);
        row["rule"] = json!(rule);
        row["blacknick"] = json!("");
        row["blackenname"] = json!("Sample English");
        row["blackname"] = json!("Sample fallback");
        let game = parse::page(&data, "0").unwrap().games.remove(0);
        assert_eq!(
            game.result.winner,
            Some(if winner == 1 {
                FoxColor::Black
            } else {
                FoxColor::White
            })
        );
        assert_eq!(game.result.kind, kind);
        assert_eq!(game.result.score, score);
        assert_eq!(game.result.unit, unit);
        assert_eq!(game.black.nickname, "Sample English");
    }
}

#[test]
fn numeric_time_precedes_text_and_beijing_fallback_is_host_independent() {
    let mut data = recorded();
    for (primary, fallback, expected) in [
        (json!(123456), json!("2026-11-14 08:00:00"), Some(123456)),
        (json!("123456"), json!("2026-11-14 08:00:00"), Some(123456)),
        (Value::Null, json!(123456), Some(123456)),
        (json!("broken"), json!("123456"), Some(123456)),
        (Value::Null, json!("1970-01-01 08:00:00"), Some(0)),
        (json!(-1), json!("2026-11-14 08:00:00"), Some(-1)),
        (Value::Null, json!("2026-11-14 08:00:00"), Some(1794614400)),
        (json!(i64::MAX), json!("invalid"), None),
        (Value::Null, json!("2026-02-30 08:00:00"), None),
        (Value::Null, Value::Null, None),
    ] {
        data["chesslist"][0]["gamestarttime"] = primary;
        data["chesslist"][0]["starttime"] = fallback;
        let page = parse::page(&data, "0").unwrap();
        assert_eq!(page.games.len(), 5);
        assert_eq!(page.games[0].start_time, expected);
    }
    data["chesslist"][0]
        .as_object_mut()
        .unwrap()
        .remove("gamestarttime");
    data["chesslist"][0]["starttime"] = json!("2026-11-14 08:00:00");
    assert_eq!(
        parse::page(&data, "0").unwrap().games[0].start_time,
        Some(1794614400)
    );
    data["chesslist"][0]
        .as_object_mut()
        .unwrap()
        .remove("starttime");
    let missing = parse::page(&data, "0").unwrap();
    assert_eq!(missing.games.len(), 5);
    assert_eq!(missing.games[0].start_time, None);
    for invalid in [
        "2026/11/14 08:00:00",
        "2026-11-14T08:00:00",
        "2026-1-14 08:00:00",
        "2026-11-14  8:00:00",
        "2026-11-14 24:00:00",
    ] {
        data["chesslist"][0]["starttime"] = json!(invalid);
        assert_eq!(parse::page(&data, "0").unwrap().games[0].start_time, None);
    }
    let original = recorded();
    assert_eq!(
        parse::page(&original, "0").unwrap().games[0].start_time,
        original["chesslist"][0]["gamestarttime"].as_i64()
    );
}

#[test]
fn api_errors_preserve_result_ret_and_message() {
    for (result, ret) in [(7, None), (0, Some(8)), (7, Some(8))] {
        let mut data = json!({"result":result,"resultstr":"sample API error"});
        if let Some(ret) = ret {
            data["ret"] = json!(ret);
        }
        assert!(
            matches!(parse::page(&data,"0"), Err(TencentError::Api { result: r, ret: t, message }) if r==result && t==ret && message=="sample API error")
        );
    }
    assert!(parse::page(&json!({"result":0,"ret":"broken","chesslist":[]}), "0").is_err());
}

#[test]
fn normalized_queries_are_plain_strings_and_reject_empty_or_control_input() {
    assert_eq!(
        TencentQuery(" 001234 ".into()).normalized().unwrap().0,
        "001234"
    );
    assert_eq!(
        serde_json::to_value(TencentQuery("sample".into())).unwrap(),
        json!("sample")
    );
    for query in ["", "  ", "sample\nname"] {
        assert!(TencentQuery(query.into()).normalized().is_err());
    }
}

proptest::proptest! {
    #[test]
    fn structured_rank_roundtrips(raw in proptest::prelude::any::<i32>()) {
        let rank = TencentRank::from_raw(raw);
        let encoded = serde_json::to_vec(&rank).unwrap();
        proptest::prop_assert_eq!(serde_json::from_slice::<TencentRank>(&encoded).unwrap(), rank);
    }
}
