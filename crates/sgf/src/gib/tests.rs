#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use proptest::prelude::*;

fn fixture(name: &str) -> Collection {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/gib")
        .join(name);
    parse_gib(&std::fs::read(path).unwrap()).unwrap()
}

fn values<'a>(node: &'a Node, key: &str) -> Vec<&'a str> {
    node.properties
        .iter()
        .filter(|p| p.identifier == key)
        .flat_map(|p| p.values.iter().map(String::as_str))
        .collect()
}

#[test]
fn utf8_golden_keeps_players_ranks_result_date_moves_and_implicit_pass_color() {
    let game = fixture("utf8-even.gib");
    assert_eq!(game.encoding, "UTF-8");
    assert!(game.warnings.is_empty());
    let sequence = &game.games[0].sequence;
    let root = &sequence[0];
    for (key, expected) in [
        ("PB", "Black Player"),
        ("BR", "3d"),
        ("PW", "White Player"),
        ("WR", "2d"),
        ("KM", "6.5"),
        ("HA", "0"),
        ("RE", "B+2.5"),
        ("DT", "2026-11-01"),
    ] {
        assert_eq!(get(root, key), Some(expected), "{key}");
    }
    assert!(values(root, "AB").is_empty());
    assert_eq!(
        sequence[1..],
        [
            move_node("B", "dd".into()),
            move_node("W", "pp".into()),
            move_node("B", String::new()),
            move_node("W", "dp".into())
        ]
    );
}

#[test]
fn gb18030_golden_keeps_unicode_metadata_and_java_five_stone_topology() {
    let game = fixture("gb18030-handicap.gib");
    assert_eq!(game.encoding, "GB18030");
    let sequence = &game.games[0].sequence;
    let root = &sequence[0];
    for (key, expected) in [
        ("PB", "黑方棋手"),
        ("BR", "5段"),
        ("PW", "白方棋手"),
        ("WR", "3段"),
        ("KM", "0.5"),
        ("HA", "5"),
        ("RE", "W+R"),
        ("DT", "2026-11-02"),
        ("PL", "W"),
    ] {
        assert_eq!(get(root, key), Some(expected), "{key}");
    }
    assert_eq!(values(root, "AB"), ["jj", "dp", "pd", "pp", "dd"]);
    assert_eq!(
        sequence[1..],
        [
            move_node("W", "kk".into()),
            move_node("B", "mm".into()),
            move_node("W", String::new())
        ]
    );
}

#[test]
fn all_handicap_placements_match_java_and_above_nine_is_clamped() {
    let expected = [
        vec![],
        vec![],
        vec!["dp", "pd"],
        vec!["dp", "pd", "pp"],
        vec!["dp", "pd", "pp", "dd"],
        vec!["jj", "dp", "pd", "pp", "dd"],
        vec!["dp", "pd", "pp", "dd", "dj", "pj"],
        vec!["jj", "dp", "pd", "pp", "dd", "dj", "pj"],
        vec!["dp", "pd", "pp", "dd", "dj", "pj", "jd", "jp"],
        vec!["dp", "pd", "pp", "dd", "dj", "pj", "jd", "jp", "jj"],
    ];
    for handicap in 0..=10 {
        let input = format!("INI 0 0 {handicap}\nSKI 0 1\n");
        let game = parse_gib(input.as_bytes()).unwrap();
        assert_eq!(
            values(&game.games[0].sequence[0], "AB"),
            expected[handicap.min(9)]
        );
        assert_eq!(
            game.games[0].sequence[1],
            move_node(if handicap >= 2 { "W" } else { "B" }, String::new())
        );
        assert_eq!(game.warnings.len(), usize::from(handicap > 9));
    }
}

#[test]
fn gametag_metadata_fallback_and_main_fields_have_priority() {
    let game = parse_gib(b"\\[GAMETAG=C2025:02:03 10:00:00,W8,Z0,G75,\\]\n").unwrap();
    let root = &game.games[0].sequence[0];
    assert_eq!(get(root, "KM"), Some("7.5"));
    assert_eq!(get(root, "RE"), Some("W+T"));
    assert_eq!(get(root, "DT"), Some("2025-02-03"));
    let input =
        b"\\[GAMETAG=C2025:02:03,W8,Z0,G75,\\]\n\\[GAMEINFOMAIN=GONGJE:65,GRLT:1,ZIPSU:15,\\]\n";
    let game = parse_gib(input).unwrap();
    assert_eq!(get(&game.games[0].sequence[0], "KM"), Some("6.5"));
    assert_eq!(get(&game.games[0].sequence[0], "RE"), Some("W+1.5"));
    for (kind, result) in [("3", "B+R"), ("4", "W+R"), ("7", "B+T"), ("8", "W+T")] {
        let input = format!("\\[GAMEINFOMAIN=GRLT:{kind},\\]\n");
        assert_eq!(
            get(
                &parse_gib(input.as_bytes()).unwrap().games[0].sequence[0],
                "RE"
            ),
            Some(result)
        );
    }
}

#[test]
fn line_endings_bom_unicode_and_name_punctuation_are_preserved() {
    let input = "\\[GAMEBLACKNAME=围棋 (A)(B)\\]\nSTO 0 1 1 3 3\n";
    for text in [
        input.to_owned(),
        input.replace('\n', "\r\n"),
        format!("\u{feff}{input}"),
    ] {
        let game = parse_gib(text.as_bytes()).unwrap();
        assert_eq!(get(&game.games[0].sequence[0], "PB"), Some("围棋 (A)(B)"));
        assert_eq!(get(&game.games[0].sequence[0], "BR"), None);
        assert_eq!(game.games[0].sequence[1], move_node("B", "dd".into()));
    }
}

#[test]
fn malformed_records_fail_without_panics_or_partial_imports() {
    for input in [
        "",
        "garbage",
        "STO",
        "INI",
        "STO 0 1 1 -1 3",
        "STO 0 1 1 19 3",
        "STO 0 1 1 3 3\nINI 0 0 2",
        "INI 0 0 2\nINI 0 0 3",
        "\\[GAMEINFOMAIN=GONGJE:bad,\\]\n",
    ] {
        let error = parse_gib(input.as_bytes()).unwrap_err();
        assert!(error.position <= input.len());
        assert!(matches!(error.kind, ErrorKind::InvalidGib { .. }));
    }
    assert!(matches!(
        parse_gib(b"\xef\xbb\xbf\xff").unwrap_err().kind,
        ErrorKind::InvalidEncoding { .. }
    ));
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn gib_import_round_trips_as_sgf(
        moves in prop::collection::vec((any::<bool>(), 0u8..19, 0u8..19, any::<bool>()), 0..100),
        handicap in 0u8..10,
    ) {
        let mut input = format!("INI 0 0 {handicap}\n");
        for (i, (black, x, y, pass)) in moves.into_iter().enumerate() {
            if pass {
                input.push_str(&format!("SKI 0 {i}\n"));
            } else {
                input.push_str(&format!("STO 0 {i} {} {x} {y}\n", if black { 1 } else { 2 }));
            }
        }
        let imported = parse_gib(input.as_bytes()).unwrap();
        let sgf = crate::serialize(&imported);
        let restored = crate::parse(sgf.as_bytes()).unwrap();
        prop_assert_eq!(imported.games, restored.games);
    }

    #[test]
    fn arbitrary_gib_input_never_panics(input in prop::collection::vec(any::<u8>(), 0..2048)) {
        let _ = parse_gib(&input);
    }
}
