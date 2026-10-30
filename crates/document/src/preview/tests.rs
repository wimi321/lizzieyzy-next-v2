#![allow(clippy::unwrap_used, clippy::expect_used)]

use go_core::{Color, IllegalMove, KoRule, Move, Point};

use crate::{Captures, Command, DocError, GameDocument, NodeId};

fn document(input: &str) -> GameDocument {
    GameDocument::from_sgf(&sgf::parse(input.as_bytes()).unwrap().games[0]).unwrap()
}

fn play(color: Color, x: u8, y: u8) -> Move {
    Move::Play(color, Point { x, y })
}

#[test]
fn preview_captures_existing_stones_and_stops_at_first_illegal_move() {
    let doc = document("(;SZ[5]AB[ab][ba][bc]AW[bb][ca][cc][db])");
    let pv = [
        play(Color::Black, 2, 1),
        play(Color::White, 2, 0), // Already occupied: do not attempt the third move.
        play(Color::Black, 4, 4),
    ];
    let preview = doc.preview_variation(doc.root(), &pv).unwrap();
    assert_eq!(preview.applied_moves, pv[..1]);
    assert_eq!(preview.position.board.stone_at(Point { x: 1, y: 1 }), None);
    assert_eq!(preview.position.board.stone_at(Point { x: 4, y: 4 }), None);
    assert_eq!(
        preview.position.captured_by,
        Captures { black: 1, white: 0 }
    );
    assert_eq!(preview.position.board.to_play(), Color::White);
    assert_eq!(preview.stones.len(), 1);
    assert_eq!(preview.stones[0].point, Point { x: 2, y: 1 });
    assert_eq!(preview.stones[0].color, Color::Black);
    assert_eq!(preview.stones[0].move_number, 1);
    assert!(matches!(
        preview.stop_reason,
        Some(DocError::IllegalMove {
            reason: IllegalMove::Occupied { .. },
            ..
        })
    ));
    assert_eq!(preview.position.moves, pv[..1]);
    assert_eq!(preview.position.analysis_moves, pv[..1]);
}

#[test]
fn captured_preview_stones_lose_numbers_and_replayed_points_get_new_numbers() {
    let doc = document("(;SZ[5]AB[ab][ba][bc]AW[ca][cc][db])");
    // White fills bb, Black captures with cb, ko is passed, White recaptures,
    // then ko is passed again and Black returns to cb with a new label.
    let pv = [
        Move::Pass(Color::Black),
        play(Color::White, 1, 1),
        play(Color::Black, 2, 1),
        Move::Pass(Color::White),
        Move::Pass(Color::Black),
        play(Color::White, 1, 1),
        Move::Pass(Color::Black),
        Move::Pass(Color::White),
        play(Color::Black, 2, 1),
    ];
    let preview = doc.preview_variation(doc.root(), &pv).unwrap();
    assert_eq!(preview.applied_moves, pv);
    assert_eq!(preview.stop_reason, None);
    assert_eq!(preview.stones.len(), 1);
    assert_eq!(preview.stones[0].point, Point { x: 2, y: 1 });
    assert_eq!(preview.stones[0].move_number, 9);
    assert_eq!(
        preview.position.captured_by,
        Captures { black: 2, white: 1 }
    );
}

#[test]
fn preview_enforces_ko_including_history_before_preview() {
    for input in [
        "(;SZ[5]AB[ab][ba][bc]AW[bb][ca][cc][db])",
        "(;SZ[5]AB[ab][ba][bc]AW[bb][ca][cc][db];B[cb])",
    ] {
        let mut doc = document(input);
        let pv = if doc.node(doc.root()).unwrap().children().is_empty() {
            vec![play(Color::Black, 2, 1), play(Color::White, 1, 1)]
        } else {
            doc.apply(Command::Step(1)).unwrap();
            vec![play(Color::White, 1, 1)]
        };
        let preview = doc.preview_variation(doc.current(), &pv).unwrap();
        assert_eq!(preview.applied_moves.len(), pv.len() - 1);
        assert!(matches!(
            preview.stop_reason,
            Some(DocError::IllegalMove {
                reason: IllegalMove::Ko {
                    rule: KoRule::Simple,
                    ..
                },
                ..
            })
        ));
    }
}

#[test]
fn preview_passes_have_no_labels_and_wrong_color_truncates_without_mutation() {
    let doc = GameDocument::new();
    let pv = [
        Move::Pass(Color::Black),
        play(Color::White, 1, 1),
        Move::Pass(Color::White),
        play(Color::Black, 2, 2),
    ];
    let preview = doc.preview_variation(doc.root(), &pv).unwrap();
    assert_eq!(preview.applied_moves, pv[..2]);
    assert_eq!(preview.stones.len(), 1);
    assert_eq!(preview.stones[0].move_number, 2);
    assert_eq!(preview.position.moves.len(), 2);
    assert_eq!(preview.position.board.to_play(), Color::Black);
    assert!(matches!(
        preview.stop_reason,
        Some(DocError::WrongPlayer {
            expected: Color::Black,
            actual: Color::White,
            ..
        })
    ));
    let empty = doc.preview_variation(doc.root(), &[]).unwrap();
    assert_eq!(empty.position, doc.position_at(doc.root()).unwrap());
    assert!(empty.applied_moves.is_empty());
    assert!(empty.stones.is_empty());
    assert_eq!(empty.stop_reason, None);
}

#[test]
fn preview_handles_self_capture_without_a_ghost_label() {
    let doc = document("(;SZ[3]RU[tromp-taylor]AB[aa]AW[ba][bb][ac])");
    let preview = doc
        .preview_variation(doc.root(), &[play(Color::Black, 0, 1)])
        .unwrap();
    assert_eq!(preview.applied_moves.len(), 1);
    assert_eq!(preview.stop_reason, None);
    assert!(preview.stones.is_empty());
    assert_eq!(
        preview.position.captured_by,
        Captures { black: 0, white: 2 }
    );
    assert_eq!(preview.position.board.stone_at(Point { x: 0, y: 0 }), None);
    assert_eq!(preview.position.board.stone_at(Point { x: 0, y: 1 }), None);
}

#[test]
fn preview_rejects_invalid_base_and_truncates_out_of_bounds_or_suicide() {
    let doc = document("(;SZ[3]AW[ba][ab];B[aa])");
    assert!(matches!(
        doc.preview_variation(NodeId(1), &[]),
        Err(DocError::IllegalMove { .. })
    ));
    assert_eq!(
        doc.preview_variation(NodeId(999), &[]),
        Err(DocError::MissingNode(NodeId(999)))
    );
    for mv in [play(Color::Black, 0, 0), play(Color::Black, 3, 0)] {
        let preview = doc.preview_variation(doc.root(), &[mv]).unwrap();
        assert_eq!(preview.position, doc.position_at(doc.root()).unwrap());
        assert!(preview.applied_moves.is_empty());
        assert!(preview.stones.is_empty());
        assert!(matches!(
            preview.stop_reason,
            Some(DocError::IllegalMove { .. })
        ));
    }
}

#[test]
fn preview_preserves_focus_revision_and_entire_undo_redo_history() {
    let mut doc = document("(;SZ[9])");
    doc.apply(Command::Play(play(Color::Black, 0, 0))).unwrap();
    doc.apply(Command::SetComment("kept".into())).unwrap();
    doc.apply(Command::Play(play(Color::White, 1, 1))).unwrap();
    doc.undo().unwrap(); // Retain both an undo and a redo history.
    let mut baseline = doc.clone();
    let revision = doc.revision();
    let snapshot = doc.snapshot();
    for (node, moves) in [
        (doc.current(), vec![play(Color::White, 2, 2)]),
        (doc.root(), vec![Move::Pass(Color::Black)]),
        (doc.current(), vec![play(Color::White, 0, 0)]),
    ] {
        doc.preview_variation(node, &moves).unwrap();
        assert_eq!(doc.revision(), revision);
        assert_eq!(doc.snapshot(), snapshot);
    }
    assert!(doc.can_undo());
    assert!(doc.can_redo());
    doc.redo().unwrap();
    baseline.redo().unwrap();
    assert_eq!(doc.snapshot(), baseline.snapshot());
    while baseline.can_undo() {
        assert_eq!(doc.undo().unwrap(), baseline.undo().unwrap());
        assert_eq!(doc.snapshot(), baseline.snapshot());
    }
    assert_eq!(doc.undo(), Err(DocError::NothingToUndo));
    while baseline.can_redo() {
        assert_eq!(doc.redo().unwrap(), baseline.redo().unwrap());
        assert_eq!(doc.snapshot(), baseline.snapshot());
    }
    assert_eq!(doc.redo(), Err(DocError::NothingToRedo));
}
