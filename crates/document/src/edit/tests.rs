#![allow(clippy::unwrap_used, clippy::expect_used)]

use go_core::{Color, IllegalMove, KoRule, Move, Point};
use proptest::prelude::*;

use crate::{Command, DocError, GameDocument, NodeId};

fn document(input: &[u8]) -> GameDocument {
    GameDocument::from_sgf(&sgf::parse(input).unwrap().games[0]).unwrap()
}

fn play(color: Color, x: u8, y: u8) -> Command {
    Command::Play(Move::Play(color, Point { x, y }))
}

#[test]
fn play_reuses_existing_children_and_creates_ordered_variations() {
    let mut doc = document(b"(;SZ[9];B[aa]C[existing](;W[bb])(;W[]))");
    let root = doc.root();
    let first = doc.node(root).unwrap().children()[0];
    let children = doc.node(first).unwrap().children().to_vec();
    let count = doc.snapshot().nodes.len();
    assert_eq!(doc.apply(play(Color::Black, 0, 0)), Ok(1));
    assert_eq!(doc.current(), first);
    assert_eq!(doc.node(first).unwrap().comment(), Some("existing"));
    doc.apply(Command::Play(Move::Pass(Color::White))).unwrap();
    assert_eq!(doc.current(), children[1]);
    assert_eq!(doc.snapshot().nodes.len(), count);
    doc.apply(Command::GoTo(first)).unwrap();
    doc.apply(play(Color::White, 2, 2)).unwrap();
    let created = doc.current();
    assert_eq!(
        doc.node(first).unwrap().children(),
        &[children[0], children[1], created]
    );
    assert_eq!(doc.snapshot().nodes.len(), count + 1);
    doc.apply(Command::PromoteVariation(created)).unwrap();
    doc.apply(Command::GoTo(first)).unwrap();
    doc.apply(Command::Step(1)).unwrap();
    assert_eq!(doc.current(), created);
    assert_eq!(doc.position_at(created).unwrap().moves.len(), 2);
}

#[test]
fn deletion_restores_subtree_ids_order_and_current_on_undo() {
    let mut doc = document(b"(;SZ[9](;B[aa];W[bb])(;B[cc];W[dd]))");
    let root = doc.root();
    let removed = doc.node(root).unwrap().children()[0];
    let removed_leaf = doc.node(removed).unwrap().children()[0];
    let sibling = doc.node(root).unwrap().children()[1];
    doc.apply(Command::GoTo(removed_leaf)).unwrap();
    let before = doc.snapshot();
    doc.apply(Command::DeleteNode(removed)).unwrap();
    assert_eq!(doc.current(), root);
    assert_eq!(doc.node(root).unwrap().children(), &[sibling]);
    assert_eq!(
        doc.node(removed_leaf),
        Err(DocError::MissingNode(removed_leaf))
    );
    let after = doc.snapshot();
    doc.apply(Command::GoTo(sibling)).unwrap();
    doc.undo().unwrap();
    assert_eq!(doc.snapshot(), before);
    doc.apply(Command::GoTo(sibling)).unwrap();
    doc.redo().unwrap();
    assert_eq!(doc.snapshot(), after);
    doc.undo().unwrap();
    doc.apply(Command::GoTo(sibling)).unwrap();
    doc.apply(Command::DeleteNode(removed)).unwrap();
    assert_eq!(doc.current(), sibling);
}

#[test]
fn discarded_redo_and_deleted_nodes_never_recycle_ids() {
    let mut doc = GameDocument::new();
    let root = doc.root();
    doc.apply(play(Color::Black, 0, 0)).unwrap();
    let abandoned = doc.current();
    doc.undo().unwrap();
    // A failed operation and a no-op must preserve the redo stack.
    assert!(doc.apply(Command::DeleteNode(root)).is_err());
    assert_eq!(doc.apply(Command::GoTo(root)), Ok(2));
    assert!(doc.can_redo());
    doc.apply(play(Color::Black, 1, 1)).unwrap();
    let second = doc.current();
    assert!(second > abandoned);
    assert!(!doc.can_redo());
    assert!(doc.node(abandoned).is_err());
    doc.apply(Command::DeleteNode(second)).unwrap();
    doc.apply(play(Color::Black, 0, 0)).unwrap();
    assert!(doc.current() > second);
    assert_eq!(doc.redo(), Err(DocError::NothingToRedo));
}

#[test]
fn navigation_clamps_at_boundaries_and_noops_do_not_advance_revision() {
    let mut doc = document(b"(;SZ[9];B[aa];W[bb])");
    let root = doc.root();
    assert_eq!(doc.apply(Command::Step(i32::MIN)), Ok(0));
    assert_eq!(doc.apply(Command::Step(0)), Ok(0));
    assert_eq!(doc.undo(), Err(DocError::NothingToUndo));
    assert_eq!(doc.apply(Command::Step(i32::MAX)), Ok(1));
    let end = doc.current();
    assert_eq!(doc.apply(Command::Step(1)), Ok(1));
    assert_eq!(doc.apply(Command::PromoteVariation(end)), Ok(1));
    assert_eq!(doc.apply(Command::Step(i32::MIN)), Ok(2));
    assert_eq!(doc.current(), root);
    assert_eq!(doc.undo(), Err(DocError::NothingToUndo));
    assert_eq!(doc.redo(), Err(DocError::NothingToRedo));
    assert_eq!(doc.revision(), 2);
    assert_eq!(doc.current(), root);
}

#[test]
fn navigation_preserves_history_and_undo_targets_last_content_change() {
    let mut doc = document(b"(;SZ[9];B[aa];W[bb])");
    let root = doc.root();
    let parent = doc.node(root).unwrap().children()[0];
    doc.apply(Command::GoTo(parent)).unwrap();
    doc.apply(play(Color::White, 2, 2)).unwrap();
    let created = doc.current();
    let before_navigation = doc.revision();
    for _ in 0..1_000 {
        doc.apply(Command::GoTo(root)).unwrap();
        doc.apply(play(Color::Black, 0, 0)).unwrap();
        doc.apply(Command::Step(1)).unwrap();
        doc.apply(Command::GoTo(created)).unwrap();
        assert_eq!((doc.undo.len(), doc.redo.len()), (1, 0));
    }
    assert_eq!(doc.revision(), before_navigation + 4_000);
    doc.undo().unwrap();
    assert_eq!(doc.current(), parent);
    assert_eq!(doc.node(created), Err(DocError::MissingNode(created)));
    assert_eq!((doc.undo.len(), doc.redo.len()), (0, 1));

    let before_navigation = doc.revision();
    for _ in 0..1_000 {
        doc.apply(Command::GoTo(root)).unwrap();
        doc.apply(play(Color::Black, 0, 0)).unwrap();
        doc.apply(Command::Step(1)).unwrap();
        doc.apply(Command::Step(-1)).unwrap();
        assert_eq!((doc.undo.len(), doc.redo.len()), (0, 1));
    }
    assert_eq!(doc.revision(), before_navigation + 4_000);
    doc.redo().unwrap();
    assert_eq!(doc.current(), created);
    assert_eq!(doc.node(created).unwrap().parent(), Some(parent));
    assert_eq!((doc.undo.len(), doc.redo.len()), (1, 0));
}

#[test]
fn comment_and_promotion_history_restore_edit_focus_after_navigation() {
    let mut doc = document(b"(;SZ[9](;B[aa]C[before])(;B[bb]))");
    let root = doc.root();
    let first = doc.node(root).unwrap().children()[0];
    let variation = doc.node(root).unwrap().children()[1];
    doc.apply(Command::GoTo(first)).unwrap();
    doc.apply(Command::SetComment("after".into())).unwrap();
    doc.apply(Command::GoTo(variation)).unwrap();
    doc.undo().unwrap();
    assert_eq!(doc.current(), first);
    assert_eq!(doc.node(first).unwrap().comment(), Some("before"));
    doc.apply(Command::GoTo(root)).unwrap();
    doc.redo().unwrap();
    assert_eq!(doc.current(), first);
    assert_eq!(doc.node(first).unwrap().comment(), Some("after"));

    doc.apply(Command::GoTo(variation)).unwrap();
    doc.apply(Command::PromoteVariation(variation)).unwrap();
    doc.apply(Command::GoTo(first)).unwrap();
    doc.undo().unwrap();
    assert_eq!(doc.current(), variation);
    assert_eq!(doc.node(root).unwrap().children(), &[first, variation]);
    doc.apply(Command::GoTo(root)).unwrap();
    doc.redo().unwrap();
    assert_eq!(doc.current(), variation);
    assert_eq!(doc.node(root).unwrap().children(), &[variation, first]);
}

#[test]
fn comments_preserve_unknowns_and_restore_duplicate_comments_exactly() {
    let mut doc = document(b"(;SZ[9]C[a]XX[x]C[b]LZ[payload])");
    let original = doc.snapshot();
    doc.apply(Command::SetComment("new ] \\ 中文".into()))
        .unwrap();
    let modified = doc.snapshot();
    assert_eq!(
        doc.node(doc.root()).unwrap().comment(),
        Some("new ] \\ 中文")
    );
    let revision = doc.revision();
    assert_eq!(
        doc.apply(Command::SetComment("new ] \\ 中文".into())),
        Ok(revision)
    );
    doc.undo().unwrap();
    assert_eq!(doc.snapshot(), original);
    doc.redo().unwrap();
    assert_eq!(doc.snapshot(), modified);
    doc.apply(Command::SetComment(String::new())).unwrap();
    assert_eq!(doc.node(doc.root()).unwrap().comment(), Some(""));
}

#[test]
fn errors_leave_content_revision_allocation_and_history_unchanged() {
    let mut doc = document(b"(;SZ[9];B[aa])");
    doc.apply(Command::Step(1)).unwrap();
    let before = doc.snapshot();
    let revision = doc.revision();
    let allocated = doc.arena.len();
    let commands = [
        play(Color::White, 0, 0),
        play(Color::White, 99, 0),
        Command::GoTo(NodeId(u32::MAX)),
        Command::DeleteNode(doc.root()),
        Command::PromoteVariation(doc.root()),
        Command::DeleteNode(NodeId(u32::MAX)),
        Command::PromoteVariation(NodeId(u32::MAX)),
    ];
    for command in commands {
        assert!(doc.apply(command).is_err());
        assert_eq!(doc.snapshot(), before);
        assert_eq!(doc.revision(), revision);
        assert_eq!(doc.arena.len(), allocated);
        assert_eq!(doc.undo.len(), 0);
        assert!(!doc.can_redo());
    }
    doc.revision = u64::MAX;
    assert_eq!(
        doc.apply(Command::SetComment("x".into())),
        Err(DocError::RevisionLimit)
    );
    assert_eq!(doc.undo(), Err(DocError::RevisionLimit));
    assert_eq!(doc.snapshot(), before);
}

#[test]
fn user_moves_check_ko_even_when_matching_a_recorded_replay_child() {
    let mut doc = document(b"(;SZ[5]AB[ba][ab][bc]AW[ca][bb][cc][db];B[cb];W[bb])");
    doc.apply(Command::Step(1)).unwrap();
    let capture = doc.current();
    let recapture = doc.node(capture).unwrap().children()[0];
    assert!(doc.position_at(recapture).is_ok());
    assert!(matches!(
        doc.apply(play(Color::White, 1, 1)),
        Err(DocError::IllegalMove {
            reason: IllegalMove::Ko {
                rule: KoRule::Simple,
                ..
            },
            ..
        })
    ));
    assert_eq!(doc.current(), capture);
    doc.apply(Command::Play(Move::Pass(Color::White))).unwrap();
    doc.apply(Command::Play(Move::Pass(Color::Black))).unwrap();
    doc.apply(play(Color::White, 1, 1)).unwrap();
    assert_eq!(doc.position_at(doc.current()).unwrap().captured_by.white, 1);
}

#[test]
fn user_moves_obey_turn_while_import_preserves_consecutive_colors() {
    let mut doc = document(b"(;SZ[9];B[aa];B[bb])");
    doc.apply(Command::Step(1)).unwrap();
    let before = doc.snapshot();
    assert!(matches!(
        doc.apply(play(Color::Black, 1, 1)),
        Err(DocError::WrongPlayer {
            expected: Color::White,
            actual: Color::Black,
            ..
        })
    ));
    assert_eq!(doc.snapshot(), before);
    doc.apply(Command::Step(1)).unwrap();
    assert_eq!(doc.position_at(doc.current()).unwrap().moves.len(), 2);
}

#[test]
fn new_moves_enforce_suicide_and_superko_presets() {
    let mut suicide = document(b"(;SZ[3]AW[ba][ab])");
    assert!(matches!(
        suicide.apply(play(Color::Black, 0, 0)),
        Err(DocError::IllegalMove {
            reason: IllegalMove::Suicide { .. },
            ..
        })
    ));
    let mut doc =
        document(b"(;SZ[5]RU[tromp-taylor]AB[ba][ab][bc]AW[ca][bb][cc][db];B[cb];W[];B[])");
    doc.apply(Command::Step(i32::MAX)).unwrap();
    assert!(matches!(
        doc.apply(play(Color::White, 1, 1)),
        Err(DocError::IllegalMove {
            reason: IllegalMove::Ko {
                rule: KoRule::PositionalSuperko,
                ..
            },
            ..
        })
    ));
}

#[test]
fn long_sequences_use_flat_arena_and_iterative_sgf_conversion() {
    let mut input = String::from("(;SZ[9]");
    for _ in 0..10_000 {
        input.push_str(";B[]");
    }
    input.push(')');
    let doc = document(input.as_bytes());
    assert_eq!(doc.snapshot().nodes.len(), 10_001);
    let exported = doc.to_sgf();
    assert_eq!(exported.sequence.len(), 10_001);
    assert!(exported.children.is_empty());
    let position = doc.position_at(NodeId(10_000)).unwrap();
    assert_eq!(position.moves.len(), 10_000);
    assert_eq!(position.board.to_play(), Color::White);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn arbitrary_commands_undo_and_redo_content_with_recorded_focus(
        operations in prop::collection::vec((0_u8..9, any::<u8>(), any::<u8>(), any::<bool>()), 0..100)
    ) {
        let mut doc = document(b"(;SZ[5]C[root]XX[keep](;B[aa];W[bb])(;B[cc]C[a]C[b];W[dd]))");
        let original = doc.snapshot();
        let mut states = vec![];
        for (kind, x, y, black) in operations {
            let before = doc.snapshot();
            let before_revision = doc.revision();
            let ids: Vec<_> = before.nodes.iter().map(|node| node.id()).collect();
            let id = ids[usize::from(x) % ids.len()];
            let color = if black { Color::Black } else { Color::White };
            let command = match kind {
                0 => play(color, x % 6, y % 6),
                1 => Command::Play(Move::Pass(color)),
                2 => Command::GoTo(id),
                3 => Command::Step(i32::from(x % 13) - 6),
                4 => Command::SetComment(format!("{x}]\\{y}中文")),
                5 => Command::DeleteNode(id),
                6 => Command::PromoteVariation(id),
                7 => Command::GoTo(NodeId(u32::MAX)),
                _ => Command::SetComment(String::new()),
            };
            match doc.apply(command) {
                Ok(revision) if revision != before_revision => {
                    prop_assert_eq!(revision, before_revision + 1);
                    let after = doc.snapshot();
                    // Classify observable content independently of the history
                    // implementation. Navigation may change only current.
                    if before.nodes != after.nodes {
                        states.push((before, after));
                    } else {
                        prop_assert_eq!(before.root, after.root);
                    }
                }
                _ => {
                    prop_assert_eq!(doc.snapshot(), before);
                    prop_assert_eq!(doc.revision(), before_revision);
                }
            }
            prop_assert_eq!(doc.undo.len(), states.len());
            prop_assert!(!doc.can_redo());
        }
        let final_state = doc.snapshot();
        let final_positions: Vec<_> = final_state.nodes.iter().map(|node| doc.position_at(node.id())).collect();
        for (before, _) in states.iter().rev() {
            let revision = doc.revision();
            prop_assert_eq!(doc.undo().unwrap(), revision + 1);
            prop_assert_eq!(&doc.snapshot(), before);
        }
        // Navigation is not undone, so compare restored content separately
        // from the first edit's saved focus (checked above).
        prop_assert_eq!(doc.snapshot().nodes, original.nodes);
        prop_assert_eq!(doc.root(), original.root);
        prop_assert!(!doc.can_undo());
        for (_, after) in &states {
            let revision = doc.revision();
            prop_assert_eq!(doc.redo().unwrap(), revision + 1);
            prop_assert_eq!(&doc.snapshot(), after);
        }
        prop_assert_eq!(doc.snapshot().nodes, final_state.nodes.clone());
        prop_assert_eq!(doc.root(), final_state.root);
        let redone_positions: Vec<_> = final_state.nodes.iter().map(|node| doc.position_at(node.id())).collect();
        prop_assert_eq!(redone_positions, final_positions);
        prop_assert!(!doc.can_redo());
    }
}
