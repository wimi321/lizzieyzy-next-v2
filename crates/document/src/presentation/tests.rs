#![allow(clippy::unwrap_used, clippy::expect_used)]

use go_core::{Color, Move, Point};

use crate::{Command, GameDocument, NodeId};

fn document(input: &str) -> GameDocument {
    GameDocument::from_sgf(&sgf::parse(input.as_bytes()).unwrap().games[0]).unwrap()
}

#[test]
fn root_game_info_preserves_first_text_and_reuses_tolerant_numeric_metadata() {
    let doc = document(
        "(;SZ[9]PB[ 黑方 ][ignored]PB[second]BR[9d]PW[White]WR[]\
         AP[foxwq]RU[Chinese]KM[375]HA[2]RE[B+R]DT[2026-10-30]EV[Final]\
         ;PB[child]RU[Japanese]KM[0]HA[9])",
    );
    let info = doc.game_info().unwrap();
    assert_eq!(info.black_name.as_deref(), Some(" 黑方 "));
    assert_eq!(info.black_rank.as_deref(), Some("9d"));
    assert_eq!(info.white_name.as_deref(), Some("White"));
    assert_eq!(info.white_rank.as_deref(), Some(""));
    assert_eq!(info.rules.as_deref(), Some("Chinese"));
    assert_eq!(info.komi, 7.5);
    assert_eq!(info.handicap, Some(2));
    assert_eq!(info.result.as_deref(), Some("B+R"));
    assert_eq!(info.date.as_deref(), Some("2026-10-30"));
    assert_eq!(info.event.as_deref(), Some("Final"));
    let invalid = document("(;SZ[9]KM[bad]KO[6.5]HA[bad]RU[Unknown]B[ZZ])");
    assert!(invalid.position_at(invalid.root()).is_err());
    let info = invalid.game_info().unwrap();
    assert_eq!(info.komi, 6.5);
    assert_eq!(info.handicap, Some(0));
    assert_eq!(info.rules.as_deref(), Some("Unknown"));
}

#[test]
fn missing_game_info_is_empty_except_effective_komi() {
    let doc = GameDocument::new();
    let info = doc.game_info().unwrap();
    assert_eq!(
        [
            info.black_name,
            info.black_rank,
            info.white_name,
            info.white_rank,
            info.rules,
            info.result,
            info.date,
            info.event,
        ],
        [None, None, None, None, None, None, None, None]
    );
    assert_eq!(info.handicap, None);
    assert_eq!(info.komi, 7.5);
}

#[test]
fn variation_tree_preserves_nested_fixture_relationships_and_child_order() {
    let mut doc = document(include_str!(
        "../../../../fixtures/sgf/nested-variations.sgf"
    ));
    let nodes = doc.variation_tree().unwrap();
    assert_eq!(nodes.len(), 6);
    assert_eq!(nodes[0].parent, None);
    assert_eq!(nodes[0].children, [NodeId(1)]);
    assert_eq!(nodes[1].children, [NodeId(2), NodeId(5)]);
    assert_eq!(nodes[2].children, [NodeId(3), NodeId(4)]);
    assert_eq!(nodes[4].parent, Some(NodeId(2)));
    assert_eq!(
        nodes
            .iter()
            .map(|node| node.move_number)
            .collect::<Vec<_>>(),
        [0, 1, 2, 3, 3, 2]
    );
    assert_eq!(
        nodes[4].mv,
        Some(Move::Play(Color::Black, Point { x: 5, y: 5 }))
    );
    assert!(nodes.iter().all(|node| node.move_error.is_none()));
    doc.apply(Command::GoTo(NodeId(5))).unwrap();
    doc.apply(Command::Play(Move::Pass(Color::Black))).unwrap();
    let added = doc.current();
    let updated = doc.variation_tree().unwrap();
    assert_eq!(updated.len(), 7);
    assert_eq!(updated[5].children, [added]);
    assert_eq!(updated[6].id, added);
    assert_eq!(updated[6].parent, Some(NodeId(5)));
    assert_eq!(updated[6].move_number, 3);
    assert_eq!(updated[6].mv, Some(Move::Pass(Color::Black)));
    doc.undo().unwrap();
    assert_eq!(doc.variation_tree().unwrap(), nodes);
    doc.redo().unwrap();
    assert_eq!(doc.variation_tree().unwrap(), updated);
}

#[test]
fn tree_keeps_empty_pass_setup_and_malformed_moves_inspectable() {
    let doc = document("(;SZ[9]B[aa];C[empty];W[];AB[bb];B[jj];W[cc]W[dd];B[ee]W[ff])");
    let nodes = doc.variation_tree().unwrap();
    assert_eq!(
        nodes
            .iter()
            .map(|node| node.move_number)
            .collect::<Vec<_>>(),
        [1, 1, 2, 2, 3, 4, 5]
    );
    assert!(nodes[0].mv.is_some());
    assert_eq!(nodes[1].mv, None);
    assert_eq!(nodes[2].mv, Some(Move::Pass(Color::White)));
    assert_eq!(nodes[3].mv, None);
    assert!(nodes[..4].iter().all(|node| node.move_error.is_none()));
    assert!(nodes[4..].iter().all(|node| node.move_error.is_some()));
    assert!(nodes[4..].iter().all(|node| node.mv.is_none()));
}

#[test]
fn promote_delete_and_undo_change_order_without_reusing_ids() {
    let mut doc = document("(;SZ[9](;B[aa])(;B[bb];W[]))");
    let original = doc.variation_tree().unwrap();
    doc.apply(Command::PromoteVariation(NodeId(2))).unwrap();
    let promoted = doc.variation_tree().unwrap();
    assert_eq!(promoted[0].children, [NodeId(2), NodeId(1)]);
    assert_eq!(
        promoted.iter().map(|node| node.id).collect::<Vec<_>>(),
        [NodeId(0), NodeId(2), NodeId(3), NodeId(1)]
    );
    doc.apply(Command::DeleteNode(NodeId(2))).unwrap();
    assert_eq!(doc.variation_tree().unwrap().len(), 2);
    doc.undo().unwrap();
    assert_eq!(doc.variation_tree().unwrap(), promoted);
    doc.undo().unwrap();
    assert_eq!(doc.variation_tree().unwrap(), original);
}
