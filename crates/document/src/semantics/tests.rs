#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use go_core::{IllegalMove, KoRule, RulePreset, SetupError};

fn position(input: &str) -> Result<GamePosition, DocError> {
    let collection = sgf::parse(input.as_bytes()).unwrap();
    replay(
        collection.games[0]
            .sequence
            .iter()
            .enumerate()
            .map(|(index, node)| (NodeId(index as u32), node.properties.as_slice())),
    )
}

#[test]
fn fox_komi_matches_java_nested_threshold_and_source_detection() {
    for (properties, expected) in [
        ("AP[foxwq]RU[Chinese]KM[375]", 7.5),
        ("AP[foxwq]RU[Chinese]KM[-375]", -7.5),
        ("AP[foxwq]RU[Chinese]KM[50]", 1.0),
        ("AP[foxwq]RU[Chinese]KM[-50]", -1.0),
        ("AP[foxwq]RU[Chinese]KM[400]", 8.0),
        ("AP[foxwq]RU[Chinese]KM[-400]", -8.0),
        ("AP[foxwq]RU[Chinese]KM[425]", 4.25),
        ("AP[foxwq]RU[Chinese]KM[750]", 7.5),
        ("AP[foxwq]RU[Chinese]KM[49.75]", 49.75),
        ("AP[foxwq]RU[Chinese]KM[3.75]", 3.75),
        ("AP[foxwq]RU[Japanese]KM[375]", 3.75),
        ("AP[foxwq]KM[375]", 3.75),
        ("AP[Prefix FoXwQ 1.0]RU[Chinese rules]KM[375]", 7.5),
        ("SO[foxwq]RU[Chinese]KM[375]", 7.5),
        ("SO[foxwq]RU[Chinese]KM[3.75]", 7.5),
        ("AP[fox]RU[Chinese]KM[3.75]", 7.5),
    ] {
        let result = position(&format!("(;{properties})")).unwrap();
        assert_eq!(result.komi, expected, "{properties}");
    }
}

#[test]
fn legacy_komi_matches_java_scaling_and_quarter_suffixes() {
    for (raw, expected) in [
        ("7.5", 7.5),
        ("3.5", 3.5),
        ("3.75", 7.5),
        ("3.25", 6.5),
        ("-3.25", -6.5),
        ("-3.75", -7.5),
        ("0.25", 0.5),
        ("-0.75", -1.5),
        ("200", 4.0),
        ("375", 7.5),
        ("400", 8.0),
        ("425", 8.5),
        ("450", 4.5),
        ("750", 7.5),
        ("199", 199.0),
        ("-375", -375.0),
        ("3.75000001", 3.75000001),
        ("1000000025", 10_000_000.25),
        ("-10000000.25", -10_000_000.25),
        (" 6.5 ", 6.5),
    ] {
        assert_eq!(
            metadata::normalize_komi(raw.trim().parse().unwrap(), &[]),
            expected,
            "{raw}"
        );
    }
}

#[test]
fn metadata_defaults_presets_unknown_rules_and_rectangular_size() {
    let default = position("(;)").unwrap();
    assert_eq!(default.board.size(), BoardSize::new(19, 19).unwrap());
    assert_eq!(default.rules, RulePreset::Chinese);
    assert_eq!(default.rules_raw, None);
    assert_eq!(default.komi, 7.5);
    assert_eq!(default.handicap, 0);
    assert_eq!(default.board.to_play(), Color::Black);
    for preset in [
        "chinese",
        "japanese",
        "korean",
        "aga",
        "tromp-taylor",
        "new-zealand",
    ] {
        let result = position(&format!("(;SZ[13:9]RU[ {preset} ])")).unwrap();
        assert_eq!(result.board.size(), BoardSize::new(13, 9).unwrap());
        assert_eq!(result.rules, RulePreset::from_name(preset).unwrap());
        assert_eq!(result.rules_raw, Some(format!(" {preset} ")));
    }
    let unknown = position("(;RU[unrecognized rules])").unwrap();
    assert_eq!(unknown.rules, RulePreset::Chinese);
    assert_eq!(unknown.rules_raw.as_deref(), Some("unrecognized rules"));
    assert_eq!(
        position("(;RU[Japanese])").unwrap().rules,
        RulePreset::Japanese
    );
}

#[test]
fn handicap_and_player_override_are_respected() {
    let handicap = position("(;SZ[9]HA[2]AB[cc][gg])").unwrap();
    assert_eq!(handicap.handicap, 2);
    assert_eq!(handicap.board.to_play(), Color::White);
    assert_eq!(handicap.initial_player, Color::White);
    assert_eq!(handicap.initial_stones.len(), 2);
    assert_eq!(
        position("(;HA[2]PL[B])").unwrap().board.to_play(),
        Color::Black
    );
    assert_eq!(position("(;HA[1])").unwrap().board.to_play(), Color::Black);
    assert_eq!(position("(;PL[W])").unwrap().board.to_play(), Color::White);
    let midgame = position("(;SZ[9];B[aa];PL[B];W[bb])").unwrap();
    assert_eq!(midgame.moves.len(), 2);
    assert_eq!(
        midgame.analysis_moves,
        [Move::Play(Color::White, Point { x: 1, y: 1 })]
    );
    assert_eq!(
        midgame.initial_stones,
        [(Color::Black, Point { x: 0, y: 0 })]
    );
    assert_eq!(midgame.initial_player, Color::Black);
    assert_eq!(midgame.board.to_play(), Color::Black);
    let unchanged = position("(;SZ[9];B[aa];PL[W])").unwrap();
    assert_eq!(unchanged.analysis_moves.len(), 1);
    assert!(unchanged.initial_stones.is_empty());
}

#[test]
fn setup_at_any_node_rebases_analysis_but_keeps_recorded_moves() {
    let result = position("(;SZ[9]AB[aa:bb];W[gg];AE[aa]AB[cc]AW[dd];B[ee])").unwrap();
    assert_eq!(result.moves.len(), 2);
    assert_eq!(result.analysis_moves.len(), 1);
    assert_eq!(result.initial_stones.len(), 6);
    assert_eq!(result.board.stone_at(Point { x: 0, y: 0 }), None);
    assert_eq!(
        result.board.stone_at(Point { x: 2, y: 2 }),
        Some(Color::Black)
    );
    assert_eq!(
        result.board.stone_at(Point { x: 3, y: 3 }),
        Some(Color::White)
    );
    assert_eq!(result.captured_by, Captures::default());
    let rebuilt = Position::with_setup(
        result.board.size(),
        result.rules.rules(),
        result.initial_player,
        result.initial_stones.clone(),
    )
    .unwrap();
    let mut replayed = rebuilt;
    for mv in &result.analysis_moves {
        replayed.play_unchecked_ko(*mv).unwrap();
    }
    assert_eq!(replayed, result.board);
}

#[test]
fn setup_overwrites_existing_stones_without_captures() {
    let result = position("(;SZ[9]AB[aa];AW[aa])").unwrap();
    assert_eq!(
        result.board.stone_at(Point { x: 0, y: 0 }),
        Some(Color::White)
    );
    assert_eq!(result.captured_by, Captures::default());
    assert!(result.analysis_moves.is_empty());
}

#[test]
fn replay_captures_and_ko_recapture_while_checked_play_rejects_ko() {
    let root = "SZ[5]AB[ab][ba][bc]AW[bb][ca][cc][db]";
    let mut capture = position(&format!("(;{root};B[cb])")).unwrap();
    assert_eq!(capture.captured_by, Captures { black: 1, white: 0 });
    assert!(matches!(
        capture
            .board
            .play(Move::Play(Color::White, Point { x: 1, y: 1 })),
        Err(IllegalMove::Ko {
            rule: KoRule::Simple,
            ..
        })
    ));
    let recapture = position(&format!("(;{root};B[cb];W[bb])")).unwrap();
    assert_eq!(recapture.captured_by, Captures { black: 1, white: 1 });
    assert_eq!(recapture.board.stone_at(Point { x: 2, y: 1 }), None);
}

#[test]
fn multi_stone_self_capture_credits_opponent() {
    let result = position("(;SZ[3]RU[tromp-taylor]AB[aa]AW[ba][bb][ac];B[ab])").unwrap();
    assert_eq!(result.captured_by, Captures { black: 0, white: 2 });
    assert_eq!(result.board.stone_at(Point { x: 0, y: 0 }), None);
    assert_eq!(result.board.stone_at(Point { x: 0, y: 1 }), None);
}

#[test]
fn illegal_moves_and_descendants_report_the_original_node() {
    for input in ["(;SZ[9];B[aa];W[aa])", "(;SZ[9];B[aa];W[aa];B[cc])"] {
        assert!(matches!(
            position(input),
            Err(DocError::IllegalMove {
                node: NodeId(2),
                reason: IllegalMove::Occupied {
                    point: Point { x: 0, y: 0 }
                }
            })
        ));
    }
    assert!(matches!(
        position("(;SZ[3]AW[ba][ab];B[aa])"),
        Err(DocError::IllegalMove {
            node: NodeId(1),
            reason: IllegalMove::Suicide { .. }
        })
    ));
}

#[test]
fn root_moves_passes_and_consecutive_colors_are_replayed() {
    let result = position("(;SZ[9]B[aa];B[bb];W[];W[tt])").unwrap();
    assert_eq!(result.moves.len(), 4);
    assert_eq!(result.moves[2..], [Move::Pass(Color::White); 2]);
    assert_eq!(result.board.to_play(), Color::Black);
    assert_eq!(
        result.board.stone_at(Point { x: 1, y: 1 }),
        Some(Color::Black)
    );
    let large = position("(;SZ[20];B[tt])").unwrap();
    assert_eq!(
        large.board.stone_at(Point { x: 19, y: 19 }),
        Some(Color::Black)
    );
}

#[test]
fn malformed_semantics_report_node_and_property_without_panicking() {
    for (input, name, id) in [
        ("(;SZ[9];B[jj])", "B", 1),
        ("(;B[AA])", "B", 0),
        ("(;B[aa]W[bb])", "B/W", 0),
        ("(;B[aa][bb])", "B", 0),
        ("(;B[aa]B[bb])", "B", 0),
        ("(;SZ[9];AB[jj])", "AB", 1),
        ("(;AB[bb:aa])", "AB", 0),
        ("(;AB[aa:bb:cc])", "AB", 0),
        ("(;AE[])", "AE", 0),
        ("(;AB[aa]AW[aa])", "AW", 0),
        ("(;AE[aa]AB[aa])", "AB", 0),
        ("(;AB[aa][aa])", "AB", 0),
        ("(;AB[aa:bb][bb])", "AB", 0),
    ] {
        match position(input) {
            Err(DocError::InvalidProperty { node, property, .. }) => {
                assert_eq!(node, NodeId(id), "{input}");
                assert_eq!(property, name, "{input}");
            }
            result => panic!("{input}: unexpected result {result:?}"),
        }
    }
    assert!(matches!(
        position("(;SZ[3]AB[aa]AW[ba][ab])"),
        Err(DocError::InvalidSetup {
            node: NodeId(0),
            reason: SetupError::NoLiberties { .. }
        })
    ));
    assert!(replay([]).is_err());
}

fn assert_stone_origins(position: &GamePosition, expected: &[(u8, u8, usize, NodeId)]) {
    let mut numbers = vec![None; position.board.stones().len()];
    let mut nodes = vec![None; position.board.stones().len()];
    for &(x, y, number, node) in expected {
        let index = usize::from(y) * usize::from(position.board.size().width) + usize::from(x);
        assert!(position.board.stone_at(Point { x, y }).is_some());
        numbers[index] = Some(number);
        nodes[index] = Some(node);
    }
    assert_eq!(position.stone_move_numbers, numbers);
    assert_eq!(position.stone_node_ids, nodes);
}

#[test]
fn stone_origins_clear_captured_moves_and_keep_rectangular_row_order() {
    let result = position("(;SZ[5:4];B[aa];W[ba];W[ab])").unwrap();
    assert_eq!(result.captured_by.white, 1);
    assert_eq!(result.board.stone_at(Point { x: 0, y: 0 }), None);
    assert_stone_origins(&result, &[(1, 0, 2, NodeId(2)), (0, 1, 3, NodeId(3))]);
}

#[test]
fn stone_origins_follow_recorded_ko_recaptures_instead_of_reusing_old_sources() {
    let result = position("(;SZ[5]AB[ab][ba][bc]AW[ca][cc][db];W[bb];B[cb];W[bb])").unwrap();
    assert_eq!(result.captured_by, Captures { black: 1, white: 1 });
    assert_stone_origins(&result, &[(1, 1, 3, NodeId(3))]);
}

#[test]
fn stone_origins_clear_only_explicit_setup_points_and_survive_player_resets() {
    let result =
        position("(;SZ[5:4]AB[dd]AW[ed];B[aa];W[ba];B[cc];B[ad];AB[aa]AE[ba]AW[cc];PL[B];B[bd])")
            .unwrap();
    assert_eq!(result.moves.len(), 5);
    assert_stone_origins(&result, &[(0, 3, 4, NodeId(4)), (1, 3, 5, NodeId(7))]);
    for (point, color) in [
        (Point { x: 0, y: 0 }, Color::Black),
        (Point { x: 2, y: 2 }, Color::White),
        (Point { x: 3, y: 3 }, Color::Black),
        (Point { x: 4, y: 3 }, Color::White),
    ] {
        assert_eq!(result.board.stone_at(point), Some(color));
    }
}

#[test]
fn stone_origins_count_root_moves_and_passes_without_counting_empty_nodes() {
    let result = position("(;SZ[4:3]B[aa];C[note];W[];AB[dc];B[bc];PL[W];W[])").unwrap();
    assert_eq!(result.moves.len(), 4);
    assert_stone_origins(&result, &[(0, 0, 1, NodeId(0)), (1, 2, 3, NodeId(4))]);
}

#[test]
fn stone_origins_on_a_branch_use_its_own_nodes_and_match_the_origin_query() {
    let collection = sgf::parse(b"(;SZ[5];B[aa](;W[bb];B[cc])(;W[];B[dd]))").unwrap();
    let mut document = crate::GameDocument::from_sgf(&collection.games[0]).unwrap();
    let first = document.node(document.root()).unwrap().children()[0];
    let branch = document.node(first).unwrap().children()[1];
    let last = document.node(branch).unwrap().children()[0];
    document.apply(crate::Command::GoTo(last)).unwrap();
    let result = document.position_at(last).unwrap();
    assert_stone_origins(&result, &[(0, 0, 1, first), (3, 3, 3, last)]);
    assert_eq!(
        document.stone_origin(Point { x: 3, y: 3 }).unwrap(),
        Some(crate::StoneOrigin {
            node: last,
            move_number: 3,
            review_parent: Some(branch),
        })
    );
    assert_eq!(document.stone_origin(Point { x: 1, y: 1 }).unwrap(), None);
}

#[test]
fn stone_origins_clear_the_entire_self_captured_group() {
    let result = position("(;SZ[3]RU[tromp-taylor]AW[ba][bb][ac];B[aa];B[ab])").unwrap();
    assert_eq!(result.captured_by.white, 2);
    assert_stone_origins(&result, &[]);
}
