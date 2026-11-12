#![allow(clippy::unwrap_used, clippy::expect_used)]

use go_core::{Color, Point};

use crate::{Command, DocError, GameDocument, GameInfoEdit, NewGameOptions, NodeId};

fn document(text: &str) -> GameDocument {
    GameDocument::from_sgf(&sgf::parse(text.as_bytes()).unwrap().games[0]).unwrap()
}

fn edited_info(komi: f64) -> GameInfoEdit {
    GameInfoEdit {
        black_name: Some("黑方".into()),
        black_rank: Some("9d".into()),
        white_name: Some("白方".into()),
        white_rank: Some("1p".into()),
        komi,
        rules: "chinese".into(),
        result: Some("B+R".into()),
        date: Some("2026-11-11".into()),
        event: Some("测试赛".into()),
    }
}

fn roundtrip(doc: &GameDocument) -> GameDocument {
    let collection = sgf::Collection {
        games: vec![doc.to_sgf()],
        encoding: "UTF-8".into(),
        warnings: Vec::new(),
    };
    document(&sgf::serialize(&collection))
}

#[test]
fn branch_delete_uses_nearest_fork_even_on_main_and_only_current_without_fork() {
    // Java Board.deleteBranch -> undoToChildOfPreviousWithVariation ->
    // BoardHistoryNode.findChildOfPreviousWithVariation: no mainline exception.
    for branch in [0, 1] {
        let mut doc = document("(;SZ[9];B[aa](;W[bb];B[cc];W[dd])(;W[ee];B[ff];W[gg]))");
        let fork = NodeId(1);
        let start = doc.node(fork).unwrap().children()[branch];
        let selected = doc.node(start).unwrap().children()[0];
        let leaf = doc.node(selected).unwrap().children()[0];
        doc.apply(Command::GoTo(leaf)).unwrap();
        let before = doc.snapshot();
        doc.apply(Command::DeleteBranch(selected)).unwrap();
        assert_eq!(doc.current(), fork);
        assert_eq!(doc.node(start), Err(DocError::MissingNode(start)));
        let after = doc.snapshot();
        doc.undo().unwrap();
        assert_eq!(doc.snapshot(), before);
        doc.redo().unwrap();
        assert_eq!(doc.snapshot(), after);
    }
    let mut linear = document("(;SZ[9];B[aa];W[bb];B[cc])");
    linear.apply(Command::GoTo(NodeId(3))).unwrap();
    linear.apply(Command::DeleteBranch(NodeId(2))).unwrap();
    assert_eq!(linear.current(), NodeId(1));
    assert!(linear.node(NodeId(1)).is_ok());
    assert!(linear.node(NodeId(2)).is_err());
}

#[test]
fn branch_delete_off_focus_preserves_focus_and_nearest_nested_fork() {
    let mut doc = document("(;SZ[9](;B[aa])(;B[bb](;W[cc])(;W[dd];B[ee])))");
    let before = doc.snapshot();
    doc.apply(Command::DeleteBranch(NodeId(5))).unwrap();
    assert_eq!(doc.current(), doc.root());
    assert_eq!(doc.node(NodeId(2)).unwrap().children(), &[NodeId(3)]);
    assert!(doc.node(NodeId(4)).is_err());
    doc.undo().unwrap();
    assert_eq!(doc.snapshot(), before);
}

#[test]
fn promoting_nested_path_swaps_siblings_and_is_one_undoable_edit() {
    // Java LizzieFrame.setAsMain repeatedly calls Board.setAsMainBranch,
    // swapping each chosen branch with child zero (not shifting other children).
    let mut doc = document("(;SZ[9](;B[aa])(;B[bb])(;B[cc](;W[dd])(;W[ee])(;W[ff];B[gg])))");
    let selected = NodeId(7);
    doc.apply(Command::GoTo(selected)).unwrap();
    let before = doc.snapshot();
    let revision = doc.revision();
    doc.apply(Command::PromoteToMain(selected)).unwrap();
    assert_eq!(doc.revision(), revision + 1);
    assert_eq!(
        doc.node(doc.root()).unwrap().children(),
        &[NodeId(3), NodeId(2), NodeId(1)]
    );
    assert_eq!(
        doc.node(NodeId(3)).unwrap().children(),
        &[NodeId(6), NodeId(5), NodeId(4)]
    );
    assert_eq!(doc.current(), selected);
    let after = doc.snapshot();
    let promoted_revision = doc.revision();
    assert_eq!(
        doc.apply(Command::PromoteToMain(selected)).unwrap(),
        promoted_revision
    );
    doc.apply(Command::GoTo(doc.root())).unwrap();
    doc.undo().unwrap();
    assert_eq!(doc.snapshot(), before);
    assert!(!doc.can_undo());
    doc.redo().unwrap();
    assert_eq!(doc.snapshot(), after);
}

#[test]
fn branch_commands_reject_roots_and_missing_nodes_atomically() {
    let mut doc = document("(;SZ[9];B[aa])");
    let before = doc.snapshot();
    for command in [
        Command::DeleteBranch(doc.root()),
        Command::PromoteToMain(doc.root()),
        Command::DeleteBranch(NodeId(999)),
        Command::PromoteToMain(NodeId(999)),
    ] {
        assert!(doc.apply(command).is_err());
        assert_eq!(doc.snapshot(), before);
        assert_eq!(doc.revision(), 0);
        assert!(!doc.can_undo());
    }
}

#[test]
fn empty_comment_removes_all_comments_and_tree_marker_tracks_undo_redo() {
    let mut doc = document("(;SZ[9]C[]XX[keep]C[secondary];B[aa]C[hello])");
    assert!(!doc.variation_tree().unwrap()[0].has_comment);
    doc.apply(Command::Step(1)).unwrap();
    let before = doc.snapshot();
    doc.apply(Command::SetComment(String::new())).unwrap();
    assert!(!doc.variation_tree().unwrap()[1].has_comment);
    assert!(
        doc.node(doc.current())
            .unwrap()
            .properties()
            .iter()
            .all(|p| p.identifier != "C")
    );
    let after = doc.snapshot();
    doc.undo().unwrap();
    assert_eq!(doc.snapshot(), before);
    assert!(doc.variation_tree().unwrap()[1].has_comment);
    doc.redo().unwrap();
    assert_eq!(doc.snapshot(), after);
    doc.apply(Command::GoTo(doc.root())).unwrap();
    doc.apply(Command::SetComment(String::new())).unwrap();
    assert!(
        doc.node(doc.root())
            .unwrap()
            .properties()
            .iter()
            .all(|p| p.identifier != "C")
    );
}

#[test]
fn game_info_preserves_other_properties_and_existing_order_with_one_undo() {
    let mut doc = document(
        "(;XX[raw][two]PB[old]KM[7.5]C[comment]PB[duplicate]AP[foxwq]HA[2]AB[aa][ii]KO[375]LZ[payload];W[cc])",
    );
    doc.apply(Command::Step(1)).unwrap();
    let original = doc.snapshot();
    doc.apply(Command::SetGameInfo(edited_info(50.0))).unwrap();
    let properties = doc.node(doc.root()).unwrap().properties();
    let names: Vec<_> = properties.iter().map(|p| p.identifier.as_str()).collect();
    assert_eq!(
        names,
        [
            "XX", "PB", "KM", "C", "PB", "AP", "HA", "AB", "KO", "LZ", "BR", "PW", "WR", "RE",
            "DT", "EV", "LZKOMI"
        ]
    );
    let unedited: Vec<_> = properties
        .iter()
        .filter(|p| {
            !matches!(
                p.identifier.as_str(),
                "PB" | "KM" | "BR" | "PW" | "WR" | "RU" | "RE" | "DT" | "EV" | "LZKOMI"
            )
        })
        .cloned()
        .collect();
    let original_unedited: Vec<_> = original.nodes[0]
        .properties()
        .iter()
        .filter(|p| !matches!(p.identifier.as_str(), "PB" | "KM"))
        .cloned()
        .collect();
    assert_eq!(unedited, original_unedited);
    assert_eq!(doc.game_info().unwrap().komi, 50.0);
    assert_eq!(doc.position_at(doc.current()).unwrap().komi, 50.0);
    assert_eq!(roundtrip(&doc).game_info(), doc.game_info());
    let edited = doc.snapshot();
    let revision = doc.revision();
    assert_eq!(
        doc.apply(Command::SetGameInfo(edited_info(50.0))).unwrap(),
        revision
    );
    doc.undo().unwrap();
    assert_eq!(doc.snapshot(), original);
    doc.redo().unwrap();
    assert_eq!(doc.snapshot(), edited);
    let mut cleared = edited_info(7.5);
    cleared.black_rank = None;
    cleared.white_rank = Some(String::new());
    cleared.result = None;
    doc.apply(Command::SetGameInfo(cleared)).unwrap();
    assert_eq!(doc.game_info().unwrap().black_rank, None);
    assert_eq!(doc.game_info().unwrap().white_rank, Some(String::new()));
    assert!(
        !doc.node(doc.root())
            .unwrap()
            .properties()
            .iter()
            .any(|p| p.identifier == "LZKOMI")
    );
}

#[test]
fn explicit_komi_survives_fox_and_legacy_encodings_and_stale_markers_do_not_override() {
    let fox = include_str!("../../../../fixtures/sgf/fox-recorded-2026-10-30.sgf");
    for value in [6.25, 50.0, 100.0, -50.0] {
        let mut doc = document(fox);
        let before = doc.snapshot();
        doc.apply(Command::SetGameInfo(edited_info(value))).unwrap();
        assert_eq!(doc.game_info().unwrap().komi, value);
        assert_eq!(roundtrip(&doc).game_info().unwrap().komi, value);
        doc.undo().unwrap();
        assert_eq!(doc.snapshot(), before);
    }
    let mut other = document("(;SZ[19]AP[other]KM[7.5])");
    other
        .apply(Command::SetGameInfo(edited_info(6.25)))
        .unwrap();
    assert_eq!(roundtrip(&other).game_info().unwrap().komi, 6.25);
    assert_eq!(
        document("(;SZ[19]AP[foxwq]KM[100]LZKOMI[50])")
            .game_info()
            .unwrap()
            .komi,
        1.0
    );
    assert_eq!(
        document("(;SZ[19]AP[foxwq]KM[100]LZKOMI[NaN])")
            .game_info()
            .unwrap()
            .komi,
        1.0
    );
}

#[test]
fn game_info_invalid_input_is_atomic() {
    let mut doc = document("(;SZ[9]KM[7.5]XX[keep])");
    for komi in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 81.0, -81.0] {
        let before = doc.snapshot();
        assert!(doc.apply(Command::SetGameInfo(edited_info(komi))).is_err());
        assert_eq!(doc.snapshot(), before);
        assert_eq!(doc.revision(), 0);
        assert!(!doc.can_undo());
    }
    let mut bad_rules = edited_info(7.5);
    bad_rules.rules = "unknown".into();
    assert!(matches!(
        doc.apply(Command::SetGameInfo(bad_rules)),
        Err(DocError::UnknownRules(_))
    ));
}

#[test]
fn new_game_roundtrips_information_and_rectangular_even_games() {
    let doc = GameDocument::new_game(NewGameOptions {
        width: 9,
        height: 13,
        komi: 6.25,
        handicap: 0,
        rules: "japanese".into(),
        black_name: Some("黑".into()),
        black_rank: Some("3d".into()),
        white_name: Some("白".into()),
        white_rank: Some("2d".into()),
    })
    .unwrap();
    let position = doc.position_at(doc.root()).unwrap();
    assert_eq!(
        (position.board.size().width, position.board.size().height),
        (9, 13)
    );
    assert_eq!(position.board.to_play(), Color::Black);
    assert_eq!(position.komi, 6.25);
    assert_eq!(doc.game_info(), roundtrip(&doc).game_info());
    assert_eq!(position, roundtrip(&doc).position_at(doc.root()).unwrap());
    assert_eq!(doc.revision(), 0);
    assert!(!doc.can_undo());
}

#[test]
fn new_game_rejects_invalid_sizes_handicap_rules_and_komi() {
    for (width, height, handicap) in [
        (1, 19, 0),
        (26, 19, 0),
        (19, 0, 0),
        (19, 19, 1),
        (19, 19, 10),
        (9, 13, 2),
        (7, 7, 2),
    ] {
        assert!(
            GameDocument::new_game(NewGameOptions {
                width,
                height,
                handicap,
                ..NewGameOptions::default()
            })
            .is_err()
        );
    }
    assert!(
        GameDocument::new_game(NewGameOptions {
            rules: "bad".into(),
            ..NewGameOptions::default()
        })
        .is_err()
    );
    for komi in [f64::NAN, f64::INFINITY, 361.0, -361.0] {
        assert!(
            GameDocument::new_game(NewGameOptions {
                komi,
                ..NewGameOptions::default()
            })
            .is_err()
        );
    }
}

#[test]
fn fixed_handicap_matches_java_19_topology_and_explicit_9_13_star_point_mapping() {
    // 19x19: Board.fixedHandicapPoints exact coordinate order. Java rejects
    // other sizes locally; 9/13 cases explicitly test the conservative mapped
    // star points rather than claiming these are Java execution recordings.
    let cases = [
        (
            9,
            [
                "cg,gc",
                "cc,gc,cg",
                "cc,cg,gc,gg",
                "cc,cg,gc,gg,ee",
                "cc,cg,gc,gg,ce,ge",
                "cc,cg,gc,gg,ge,ce,ee",
                "cc,cg,gc,gg,ec,eg,ce,ge",
                "cc,cg,gc,gg,ec,eg,ce,ge,ee",
            ],
        ),
        (
            13,
            [
                "dj,jd",
                "dd,jd,dj",
                "dd,dj,jd,jj",
                "dd,dj,jd,jj,gg",
                "dd,dj,jd,jj,dg,jg",
                "dd,dj,jd,jj,jg,dg,gg",
                "dd,dj,jd,jj,gd,gj,dg,jg",
                "dd,dj,jd,jj,gd,gj,dg,jg,gg",
            ],
        ),
        (
            19,
            [
                "dp,pd",
                "dd,pd,dp",
                "dd,dp,pd,pp",
                "dd,dp,pd,pp,jj",
                "dd,dp,pd,pp,dj,pj",
                "dd,dp,pd,pp,pj,dj,jj",
                "dd,dp,pd,pp,jd,jp,dj,pj",
                "dd,dp,pd,pp,jd,jp,dj,pj,jj",
            ],
        ),
    ];
    for (size, counts) in cases {
        for (index, expected) in counts.into_iter().enumerate() {
            let handicap = index as u8 + 2;
            let doc = GameDocument::new_game(NewGameOptions {
                width: size,
                height: size,
                handicap,
                ..NewGameOptions::default()
            })
            .unwrap();
            let root = doc.node(doc.root()).unwrap();
            let points = &root
                .properties()
                .iter()
                .find(|p| p.identifier == "AB")
                .unwrap()
                .values;
            assert_eq!(
                points.join(","),
                expected,
                "size={size}, handicap={handicap}"
            );
            let position = doc.position_at(doc.root()).unwrap();
            assert_eq!(position.board.to_play(), Color::White);
            assert_eq!(position.handicap, u32::from(handicap));
            assert!(position.moves.is_empty());
            assert!(position.analysis_moves.is_empty());
            assert_eq!(position.initial_stones.len(), usize::from(handicap));
            for point in expected.split(',') {
                assert_eq!(
                    position
                        .board
                        .stone_at(Point::from_sgf(point, position.board.size()).unwrap()),
                    Some(Color::Black)
                );
            }
            let imported = roundtrip(&doc);
            assert_eq!(imported.game_info(), doc.game_info());
            assert_eq!(imported.position_at(imported.root()).unwrap(), position);
        }
    }
}

#[test]
fn unchanged_default_game_info_is_noop_without_a_komi_marker() {
    let mut doc = document("(;SZ[19]KM[7.5]RU[chinese]XX[keep])");
    let before = doc.snapshot();
    let unchanged = GameInfoEdit {
        black_name: None,
        black_rank: None,
        white_name: None,
        white_rank: None,
        komi: 7.5,
        rules: "chinese".into(),
        result: None,
        date: None,
        event: None,
    };
    assert_eq!(doc.apply(Command::SetGameInfo(unchanged)).unwrap(), 0);
    assert_eq!(doc.snapshot(), before);
    assert!(!doc.can_undo());
    assert!(
        !doc.node(doc.root())
            .unwrap()
            .properties()
            .iter()
            .any(|p| p.identifier == "LZKOMI")
    );
}

fn current_edit(doc: &GameDocument) -> GameInfoEdit {
    let current = doc.game_info().unwrap();
    GameInfoEdit {
        black_name: current.black_name,
        black_rank: current.black_rank,
        white_name: current.white_name,
        white_rank: current.white_rank,
        komi: current.komi,
        rules: current.rules_preset,
        result: current.result,
        date: current.date,
        event: current.event,
    }
}

#[test]
fn fox_current_game_info_is_noop_and_preserves_original_attributes_and_redo() {
    let mut doc = document(
        "(;SZ[19]AP[foxwq]KM[375][ignored]KM[100]RU[Chinese][ignored]RU[Japanese]\
        PB[first][second]PB[duplicate]BR[9d][ignored]PW[white][ignored]WR[]\
        RE[B+R][ignored]DT[2026-11-11][ignored]EV[event][ignored]KO[unused]LZKOMI[stale])",
    );
    let original = doc.snapshot();
    assert_eq!(doc.game_info().unwrap().komi, 7.5);
    assert_eq!(doc.game_info().unwrap().rules_preset, "chinese");
    assert_eq!(
        doc.apply(Command::SetGameInfo(current_edit(&doc))).unwrap(),
        0
    );
    assert_eq!(doc.snapshot(), original);
    assert!(!doc.can_undo());
    assert!(!doc.can_redo());
    assert_eq!(doc.content_revision(), 0);

    doc.apply(Command::SetComment("temporary".into())).unwrap();
    doc.undo().unwrap();
    let revision = doc.revision();
    let content_revision = doc.content_revision();
    assert_eq!(
        doc.apply(Command::SetGameInfo(current_edit(&doc))).unwrap(),
        revision
    );
    assert_eq!(doc.content_revision(), content_revision);
    assert_eq!(doc.snapshot(), original);
    assert!(doc.can_redo(), "unchanged submission must not discard redo");
    doc.redo().unwrap();
    assert_eq!(doc.node(doc.root()).unwrap().comment(), Some("temporary"));
}

#[test]
fn changing_only_name_keeps_fox_komi_rules_and_all_other_attributes_verbatim() {
    let mut doc = document(
        "(;SZ[19]AP[foxwq]KM[375][ignored]KM[100]RU[Chinese][ignored]RU[Japanese]\
        PB[first][second]PB[duplicate]BR[9d][ignored]PW[white]WR[1d]\
        RE[B+R][ignored]DT[2026-11-11]EV[event]KO[unused]LZKOMI[stale])",
    );
    let original = doc.snapshot();
    let original_other: Vec<_> = doc
        .node(doc.root())
        .unwrap()
        .properties()
        .iter()
        .filter(|property| property.identifier != "PB")
        .cloned()
        .collect();
    let mut changed = current_edit(&doc);
    changed.black_name = Some("new name".into());
    doc.apply(Command::SetGameInfo(changed)).unwrap();
    let properties = doc.node(doc.root()).unwrap().properties();
    let other: Vec<_> = properties
        .iter()
        .filter(|property| property.identifier != "PB")
        .cloned()
        .collect();
    assert_eq!(other, original_other);
    assert!(
        properties
            .iter()
            .filter(|property| property.identifier == "PB")
            .all(|property| property.values == ["new name"])
    );
    assert_eq!(doc.game_info().unwrap().komi, 7.5);
    assert_eq!(doc.game_info().unwrap().rules.as_deref(), Some("Chinese"));
    assert_eq!(roundtrip(&doc).game_info(), doc.game_info());
    doc.undo().unwrap();
    assert_eq!(doc.snapshot(), original);
}

#[test]
fn unchanged_missing_unknown_and_fallback_game_info_does_not_normalize_source() {
    for source in [
        "(;SZ[2])",
        "(;SZ[19]RU[Unknown][ignored]RU[Chinese]KM[bad][ignored]KO[6.5][ignored])",
        "(;SZ[19]RU[]KM[bad]KO[also bad]PB[]PB[ignored])",
    ] {
        let mut doc = document(source);
        let original = doc.snapshot();
        assert_eq!(doc.game_info().unwrap().rules_preset, "chinese");
        assert_eq!(
            doc.apply(Command::SetGameInfo(current_edit(&doc))).unwrap(),
            0
        );
        assert_eq!(doc.snapshot(), original);
        assert!(!doc.can_undo());
    }
}

#[test]
fn changing_rules_alone_preserves_komi_source_and_uses_new_rule_interpretation() {
    let mut doc = document("(;SZ[19]AP[foxwq]KM[375][ignored]RU[Chinese]KO[0]LZKOMI[stale])");
    let original = doc.snapshot();
    let mut changed = current_edit(&doc);
    assert_eq!(changed.komi, 7.5);
    changed.rules = "japanese".into();
    doc.apply(Command::SetGameInfo(changed)).unwrap();
    let without_rules = |doc: &GameDocument| {
        doc.node(doc.root())
            .unwrap()
            .properties()
            .iter()
            .filter(|property| property.identifier != "RU")
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(
        without_rules(&doc),
        original.nodes[0]
            .properties()
            .iter()
            .filter(|property| property.identifier != "RU")
            .cloned()
            .collect::<Vec<_>>()
    );
    // The explicit rule edit changes the interpretation of imported Fox KM;
    // unchanged KM/KO/LZKOMI must not silently be rewritten to counteract it.
    assert_eq!(doc.game_info().unwrap().komi, 3.75);
    assert_eq!(doc.game_info().unwrap().rules_preset, "japanese");
    assert_eq!(roundtrip(&doc).game_info(), doc.game_info());
    doc.undo().unwrap();
    assert_eq!(doc.snapshot(), original);
    assert_eq!(doc.game_info().unwrap().komi, 7.5);
}

#[test]
fn new_game_still_writes_explicit_default_komi_and_rules() {
    let doc = GameDocument::new_game(NewGameOptions::default()).unwrap();
    let properties = doc.node(doc.root()).unwrap().properties();
    for (name, value) in [("KM", "7.5"), ("RU", "chinese")] {
        assert_eq!(
            properties
                .iter()
                .find(|property| property.identifier == name)
                .unwrap()
                .values,
            [value]
        );
    }
    assert_eq!(doc.content_revision(), 0);
}
