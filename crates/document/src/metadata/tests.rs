#![allow(clippy::unwrap_used, clippy::expect_used)]

use go_core::{BoardSize, Color, RulePreset};

use crate::{Command, DocumentWarning, GameDocument, GamePosition, NodeId, WarningKind};

fn document(text: &str) -> GameDocument {
    GameDocument::from_sgf(&sgf::parse(text.as_bytes()).unwrap().games[0]).unwrap()
}

fn mainline_end(document: &GameDocument) -> NodeId {
    let mut end = document.root();
    while let Some(child) = document.node(end).unwrap().children().first() {
        end = *child;
    }
    end
}

fn warning(node: u32, property: &str, kind: WarningKind) -> DocumentWarning {
    DocumentWarning {
        node: NodeId(node),
        property: property.into(),
        kind,
    }
}

fn invalid(node: u32, property: &str, fallback: &str) -> DocumentWarning {
    warning(
        node,
        property,
        WarningKind::InvalidValue {
            fallback: fallback.into(),
        },
    )
}

fn out_of_range(node: u32, property: &str, fallback: &str) -> DocumentWarning {
    warning(
        node,
        property,
        WarningKind::OutOfRange {
            fallback: fallback.into(),
        },
    )
}

/// Every recovery case must still open its final position and preserve raw SGF.
fn recovered(text: &str, expected: Vec<DocumentWarning>) -> GamePosition {
    let source = sgf::parse(text.as_bytes()).unwrap().games.remove(0);
    let document = GameDocument::from_sgf(&source).unwrap();
    let position = document.position_at(mainline_end(&document)).unwrap();
    assert_eq!(document.warnings(), expected, "{text}");
    assert_eq!(
        document.warnings(),
        expected,
        "diagnostics must not accumulate"
    );
    assert_eq!(document.to_sgf(), source, "{text}");
    position
}

#[test]
fn empty_or_unparseable_komi_uses_ko_then_default_without_blocking_moves() {
    for value in ["", "abc", " ", "NaN", "inf", "-inf"] {
        let text = format!("(;SZ[9]KM[{value}];B[aa];W[bb])");
        assert_eq!(recovered(&text, vec![invalid(0, "KM", "7.5")]).komi, 7.5);
        let text = format!("(;SZ[9]KM[{value}]KO[3.25];B[aa];W[bb])");
        let expected = vec![
            invalid(0, "KM", "KO"),
            warning(0, "KO", WarningKind::KomiFromKo),
        ];
        assert_eq!(recovered(&text, expected).komi, 6.5);
    }
    assert_eq!(
        recovered(
            "(;SZ[9]KM[abc]KO[bad];B[aa])",
            vec![invalid(0, "KM", "7.5"), invalid(0, "KO", "7.5")],
        )
        .komi,
        7.5
    );
}

#[test]
fn ko_only_komi_is_normalized_and_warned_but_never_overrides_valid_km() {
    for (metadata, expected) in [
        ("KO[6.5]", 6.5),
        ("KO[3.75]", 7.5),
        ("AP[foxwq]RU[Chinese]KO[375]", 7.5),
        ("AP[foxwq]RU[Japanese]KO[375]", 3.75),
    ] {
        assert_eq!(
            recovered(
                &format!("(;SZ[9]{metadata};B[aa];W[bb])"),
                vec![warning(0, "KO", WarningKind::KomiFromKo)],
            )
            .komi,
            expected
        );
    }
    let valid = recovered("(;SZ[9]KM[6.5]KO[bad];B[aa])", vec![]);
    assert_eq!(valid.komi, 6.5);
    assert_eq!(recovered("(;SZ[9];B[aa])", vec![]).komi, 7.5);
}

#[test]
fn komi_area_limit_is_checked_after_normalization_including_negative_and_rectangle() {
    for (size, raw) in [
        ("9", "81"),
        ("9", "-81"),
        ("9:13", "117"),
        ("9:13", "-117"),
        ("19", "1000000025"),
    ] {
        let text = format!("(;SZ[{size}]KM[{raw}]KO[6.5];B[aa];W[bb])");
        assert_eq!(
            recovered(&text, vec![out_of_range(0, "KM", "7.5")]).komi,
            7.5
        );
    }
    assert_eq!(recovered("(;SZ[9]KM[375];B[aa])", vec![]).komi, 7.5);
    assert_eq!(recovered("(;SZ[9]KM[80.5];B[aa])", vec![]).komi, 80.5);
    assert_eq!(
        recovered("(;SZ[9]KO[81];B[aa])", vec![out_of_range(0, "KO", "7.5")],).komi,
        7.5
    );
    assert_eq!(
        recovered(
            "(;SZ[9]KM[]KO[-81];B[aa])",
            vec![invalid(0, "KM", "7.5"), out_of_range(0, "KO", "7.5")],
        )
        .komi,
        7.5
    );
}

#[test]
fn malformed_handicap_and_size_fall_back_with_node_and_property_diagnostics() {
    for value in ["", "abc", "-1", "99999999999999"] {
        let result = recovered(
            &format!("(;HA[{value}];B[aa];W[bb])"),
            vec![invalid(0, "HA", "0")],
        );
        assert_eq!(result.handicap, 0);
        assert_eq!(result.initial_player, Color::Black);
    }
    for value in ["", "x", "9:bad", "9:9:9"] {
        let result = recovered(
            &format!("(;SZ[{value}];B[aa];W[bb])"),
            vec![invalid(0, "SZ", "19x19")],
        );
        assert_eq!(result.board.size(), BoardSize::new(19, 19).unwrap());
    }
    for value in ["1", "26", "9:26", "99"] {
        let result = recovered(
            &format!("(;SZ[{value}];B[aa];W[bb])"),
            vec![out_of_range(0, "SZ", "19x19")],
        );
        assert_eq!(result.board.size(), BoardSize::new(19, 19).unwrap());
    }
}

#[test]
fn player_is_case_insensitive_and_invalid_player_is_ignored_at_any_node() {
    for (value, color) in [("w", Color::White), ("b", Color::Black)] {
        let result = recovered(
            &format!("(;SZ[9]PL[{value}];C[still navigable])"),
            vec![warning(0, "PL", WarningKind::NormalizedCase)],
        );
        assert_eq!(result.board.to_play(), color);
        let result = recovered(
            &format!("(;SZ[9];B[aa];PL[{value}])"),
            vec![warning(2, "PL", WarningKind::NormalizedCase)],
        );
        assert_eq!(result.board.to_play(), color);
    }
    for value in ["", "white", "?", " W "] {
        let result = recovered(
            &format!("(;SZ[9]PL[{value}];C[still navigable])"),
            vec![warning(0, "PL", WarningKind::IgnoredPlayer)],
        );
        assert_eq!(result.board.to_play(), Color::Black);
        let result = recovered(
            &format!("(;SZ[9];B[aa];PL[{value}])"),
            vec![warning(2, "PL", WarningKind::IgnoredPlayer)],
        );
        assert_eq!(result.board.to_play(), Color::White);
        assert_eq!(result.analysis_moves.len(), 1);
    }
}

#[test]
fn repeated_or_multivalued_root_metadata_uses_first_value() {
    for (name, first, second) in [
        ("SZ", "9", "13"),
        ("KM", "6.5", "7.5"),
        ("RU", "Japanese", "Chinese"),
        ("HA", "2", "0"),
        ("PL", "W", "B"),
    ] {
        for raw in [
            format!("{name}[{first}]{name}[{second}]"),
            format!("{name}[{first}][{second}]"),
        ] {
            let result = recovered(
                &format!("(;{raw};W[aa];B[bb])"),
                vec![warning(0, name, WarningKind::FirstValueUsed)],
            );
            match name {
                "SZ" => assert_eq!(result.board.size(), BoardSize::new(9, 9).unwrap()),
                "KM" => assert_eq!(result.komi, 6.5),
                "RU" => assert_eq!(result.rules, RulePreset::Japanese),
                "HA" => assert_eq!(result.handicap, 2),
                "PL" => assert_eq!(result.initial_player, Color::White),
                _ => unreachable!(),
            }
        }
    }
    // Choosing the first value is independent of whether a later value parses.
    assert_eq!(
        recovered(
            "(;KM[]KM[6.5];B[aa])",
            vec![
                warning(0, "KM", WarningKind::FirstValueUsed),
                invalid(0, "KM", "7.5")
            ],
        )
        .komi,
        7.5
    );
    for raw in ["PL[W]PL[B]", "PL[W][B]"] {
        let result = recovered(
            &format!("(;SZ[9];B[aa];{raw})"),
            vec![warning(2, "PL", WarningKind::FirstValueUsed)],
        );
        assert_eq!(result.board.to_play(), Color::White);
    }
}

#[test]
fn non_root_game_metadata_is_ignored_without_poisoning_descendants() {
    for (name, raw) in [("SZ", "x"), ("KM", "abc"), ("RU", "Japanese"), ("HA", "2")] {
        let result = recovered(
            &format!("(;SZ[9]KM[6.5]RU[Chinese];B[aa]{name}[{raw}];W[bb])"),
            vec![warning(1, name, WarningKind::IgnoredNonRoot)],
        );
        assert_eq!(result.board.size(), BoardSize::new(9, 9).unwrap());
        assert_eq!(result.komi, 6.5);
        assert_eq!(result.rules, RulePreset::Chinese);
        assert_eq!(result.handicap, 0);
    }
}

#[test]
fn unknown_rules_are_warned_and_raw_rule_text_is_retained() {
    let result = recovered(
        "(;RU[unrecognized rules];B[aa];W[bb])",
        vec![warning(
            0,
            "RU",
            WarningKind::UnknownRules {
                fallback: "chinese".into(),
            },
        )],
    );
    assert_eq!(result.rules, RulePreset::Chinese);
    assert_eq!(result.rules_raw.as_deref(), Some("unrecognized rules"));
}

#[test]
fn warnings_cover_all_live_branches_even_after_illegal_moves_and_follow_undo() {
    let mut document = document("(;SZ[9]KM[];B[aa](;W[aa];KM[bad]PL[?])(;W[bb]HA[bad]))");
    let first = document.node(document.root()).unwrap().children()[0];
    let illegal_branch = document.node(first).unwrap().children()[0];
    let legal_branch = document.node(first).unwrap().children()[1];
    let diagnostics = vec![
        invalid(0, "KM", "7.5"),
        warning(3, "KM", WarningKind::IgnoredNonRoot),
        warning(3, "PL", WarningKind::IgnoredPlayer),
        warning(4, "HA", WarningKind::IgnoredNonRoot),
    ];
    assert!(document.position_at(legal_branch).is_ok());
    assert!(document.position_at(illegal_branch).is_err());
    assert_eq!(document.warnings(), diagnostics);
    document.apply(Command::GoTo(legal_branch)).unwrap();
    assert_eq!(document.warnings(), diagnostics);
    document.apply(Command::DeleteNode(illegal_branch)).unwrap();
    assert_eq!(
        document.warnings(),
        vec![diagnostics[0].clone(), diagnostics[3].clone()]
    );
    document.undo().unwrap();
    assert_eq!(document.warnings(), diagnostics);
    document.redo().unwrap();
    assert_eq!(
        document.warnings(),
        vec![diagnostics[0].clone(), diagnostics[3].clone()]
    );
}
