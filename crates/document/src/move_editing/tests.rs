#![allow(clippy::unwrap_used, clippy::expect_used)]

use go_core::{Color, IllegalMove, Move, Point};

use crate::{Command, DocError, GameDocument, NodeId};

fn document(input: &str) -> GameDocument {
    GameDocument::from_sgf(&sgf::parse(input.as_bytes()).unwrap().games[0]).unwrap()
}

fn point(x: u8, y: u8) -> Point {
    Point { x, y }
}

fn play(color: Color, x: u8, y: u8) -> Move {
    Move::Play(color, point(x, y))
}

fn branched() -> GameDocument {
    let mut doc = document("(;SZ[9];B[aa];W[bb](;B[cc];W[dd])(;B[ee];W[ff]))");
    doc.apply(Command::Step(2)).unwrap();
    doc
}

fn assert_tree(doc: &GameDocument, expected: &str) {
    assert_eq!(doc.to_sgf(), document(expected).to_sgf());
    // Every retained branch, not only the focus, must remain replayable.
    for node in doc.variation_tree().unwrap() {
        doc.position_at(node.id).unwrap();
    }
}

fn assert_atomic_undo_redo(doc: &mut GameDocument, command: Command, expected: &str) {
    let before = doc.snapshot();
    let revision = doc.revision();
    doc.apply(command).unwrap();
    assert_tree(doc, expected);
    assert_eq!(doc.revision(), revision + 1);
    let after = doc.snapshot();
    doc.apply(Command::GoTo(doc.root())).unwrap();
    doc.undo().unwrap();
    assert_eq!(doc.snapshot(), before);
    assert!(!doc.can_undo());
    doc.redo().unwrap();
    assert_eq!(doc.snapshot(), after);
}

#[test]
fn colored_play_records_repeated_colors_as_moves_and_reuses_matching_child() {
    let mut doc = GameDocument::new();
    assert!(matches!(
        doc.apply(Command::Play(play(Color::White, 0, 0))),
        Err(DocError::WrongPlayer { .. })
    ));
    doc.apply(Command::PlayColored(play(Color::White, 0, 0)))
        .unwrap();
    let first = doc.current();
    let before = doc.snapshot();
    doc.apply(Command::PlayColored(play(Color::White, 1, 1)))
        .unwrap();
    assert_tree(&doc, "(;;W[aa];W[bb])");
    let after = doc.snapshot();
    doc.undo().unwrap();
    assert_eq!(doc.snapshot(), before);
    doc.redo().unwrap();
    assert_eq!(doc.snapshot(), after);
    let existing = doc.current();
    let count = doc.snapshot().nodes.len();
    doc.apply(Command::GoTo(first)).unwrap();
    doc.apply(Command::PlayColored(play(Color::White, 1, 1)))
        .unwrap();
    assert_eq!(doc.current(), existing);
    assert_eq!(doc.snapshot().nodes.len(), count);
    assert_eq!(doc.position_at(existing).unwrap().moves.len(), 2);
}

#[test]
fn colored_play_obeys_capture_suicide_occupied_and_ko() {
    let mut doc = document("(;SZ[5]PL[W]AB[ab][ba][bc]AW[bb][ca][cc][db])");
    doc.apply(Command::PlayColored(play(Color::Black, 2, 1)))
        .unwrap();
    assert_eq!(
        doc.position_at(doc.current())
            .unwrap()
            .board
            .stone_at(point(1, 1)),
        None
    );
    let before = doc.snapshot();
    for mv in [
        play(Color::Black, 2, 1),
        play(Color::White, 1, 1),
        play(Color::Black, 10, 0),
    ] {
        assert!(matches!(
            doc.apply(Command::PlayColored(mv)),
            Err(DocError::IllegalMove { .. })
        ));
        assert_eq!(doc.snapshot(), before);
    }
    let mut suicide = document("(;SZ[5]PL[W]AW[ab][ba])");
    assert!(matches!(
        suicide.apply(Command::PlayColored(play(Color::Black, 0, 0))),
        Err(DocError::IllegalMove {
            reason: IllegalMove::Suicide { .. },
            ..
        })
    ));
}

#[test]
fn java_insert_move_reconnects_each_branch_and_skips_illegal_continuation() {
    // LizzieFrame.insertMove + Board.placeLinkedList: inserted cc occupies the
    // old main move, Board.place skips that move and continues with W[dd].
    let mut doc = branched();
    let current = doc.current();
    let removed = doc.node(current).unwrap().children()[0];
    assert_atomic_undo_redo(
        &mut doc,
        Command::InsertMove(play(Color::Black, 2, 2)),
        "(;SZ[9];B[aa];W[bb];B[cc](;W[dd])(;B[ee];W[ff]))",
    );
    assert_eq!(doc.node(doc.current()).unwrap().parent(), Some(current));
    assert!(doc.node(removed).is_err());
}

#[test]
fn java_switch_stone_keeps_later_colors_and_both_variations() {
    // Board.editMove(isSwitch=true) toggles only the selected move, then
    // placeLinkedList restores the recorded colors of the entire subtree.
    let mut doc = branched();
    let focus = doc.current();
    assert_atomic_undo_redo(
        &mut doc,
        Command::SwitchStone(point(0, 0)),
        "(;SZ[9];W[aa];W[bb](;B[cc];W[dd])(;B[ee];W[ff]))",
    );
    assert_eq!(doc.current(), focus);
}

#[test]
fn java_delete_stone_splices_move_and_preserves_all_branches() {
    // Board.editMove(isDelete=true) marks just the chosen move needSkip;
    // placeLinkedList still visits each continuation with its original color.
    let mut doc = branched();
    let focus = doc.current();
    assert_atomic_undo_redo(
        &mut doc,
        Command::DeleteStone(point(0, 0)),
        "(;SZ[9];W[bb](;B[cc];W[dd])(;B[ee];W[ff]))",
    );
    assert_eq!(doc.current(), focus);
}

#[test]
fn java_move_stone_rewrites_coordinate_and_skips_newly_occupied_main_move() {
    // LizzieFrame.DraggedReleased rewrites x/y, then Board.placeLinkedList
    // omits old cc because the moved stone now occupies it; side branch stays.
    let mut doc = branched();
    let focus = doc.current();
    assert_atomic_undo_redo(
        &mut doc,
        Command::MoveStone {
            from: point(0, 0),
            to: point(2, 2),
        },
        "(;SZ[9];B[cc];W[bb](;W[dd])(;B[ee];W[ff]))",
    );
    assert_eq!(doc.current(), focus);
}

#[test]
fn edited_descendants_skip_suicide_and_ko_but_keep_later_moves() {
    let mut suicide = document("(;SZ[5]AB[ab];B[ba](;W[aa];B[dd])(;W[ee]))");
    suicide.apply(Command::Step(1)).unwrap();
    // Moving the source away makes aa legal; moving it back restores the
    // surrounding black stones and skips the now-suicidal white continuation.
    suicide
        .apply(Command::MoveStone {
            from: point(1, 0),
            to: point(1, 1),
        })
        .unwrap();
    suicide
        .apply(Command::MoveStone {
            from: point(1, 1),
            to: point(1, 0),
        })
        .unwrap();
    assert_tree(&suicide, "(;SZ[5]AB[ab];B[ba](;B[dd])(;W[ee]))");

    let mut ko = document("(;SZ[5]AB[ab][ba][bc]AW[bb][ca][cc][db];W[bb];B[ee])");
    ko.apply(Command::InsertMove(play(Color::Black, 2, 1)))
        .unwrap();
    assert_tree(&ko, "(;SZ[5]AB[ab][ba][bc]AW[bb][ca][cc][db];B[cb];B[ee])");
}

#[test]
fn deleting_or_skipping_annotated_move_preserves_properties_as_empty_node() {
    let mut doc = document("(;SZ[9];B[aa]C[keep]XX[raw];W[bb](;B[cc]TR[dd];W[dd])(;B[ee]))");
    doc.apply(Command::Step(2)).unwrap();
    doc.apply(Command::DeleteStone(point(0, 0))).unwrap();
    assert_tree(
        &doc,
        "(;SZ[9];C[keep]XX[raw];W[bb](;B[cc]TR[dd];W[dd])(;B[ee]))",
    );
    doc.apply(Command::InsertMove(play(Color::Black, 2, 2)))
        .unwrap();
    assert_tree(
        &doc,
        "(;SZ[9];C[keep]XX[raw];W[bb];B[cc](;TR[dd];W[dd])(;B[ee]))",
    );
}

#[test]
fn splicing_a_fork_preserves_its_index_between_unaffected_siblings() {
    let mut doc = document("(;SZ[9](;B[aa])(;B[bb](;W[cc])(;W[dd]))(;B[ee]))");
    let middle = doc.node(doc.root()).unwrap().children()[1];
    doc.apply(Command::GoTo(middle)).unwrap();
    assert_atomic_undo_redo(
        &mut doc,
        Command::DeleteStone(point(1, 1)),
        "(;SZ[9](;B[aa])(;W[cc])(;W[dd])(;B[ee]))",
    );
    assert_eq!(doc.current(), doc.root());
}

#[test]
fn java_delete_reuses_untouched_sibling_before_appending_replayed_children() {
    let mut doc = document("(;SZ[9](;B[aa];W[bb];B[cc])(;W[bb];B[dd]))");
    let existing = doc.node(doc.root()).unwrap().children()[1];
    doc.apply(Command::Step(3)).unwrap();
    let focus = doc.current();
    assert_atomic_undo_redo(
        &mut doc,
        Command::DeleteStone(point(0, 0)),
        "(;SZ[9];W[bb](;B[dd])(;B[cc]))",
    );
    assert_eq!(doc.node(doc.root()).unwrap().children(), &[existing]);
    assert_eq!(doc.current(), focus);
}

#[test]
fn java_insert_merges_replayed_duplicate_prefix_after_skipping_occupied_move() {
    let mut doc = document("(;SZ[9];B[aa](;W[bb];B[cc])(;B[cc];W[dd]))");
    doc.apply(Command::Step(1)).unwrap();
    assert_atomic_undo_redo(
        &mut doc,
        Command::InsertMove(play(Color::White, 1, 1)),
        "(;SZ[9];B[aa];W[bb];B[cc];W[dd])",
    );
}

#[test]
fn merged_focus_redirects_to_existing_node_and_annotations_prevent_merging() {
    let mut doc = document("(;SZ[9](;B[aa];W[bb])(;W[bb]))");
    let existing = doc.node(doc.root()).unwrap().children()[1];
    doc.apply(Command::Step(2)).unwrap();
    doc.apply(Command::DeleteStone(point(0, 0))).unwrap();
    assert_eq!(doc.current(), existing);
    assert_tree(&doc, "(;SZ[9];W[bb])");

    let mut annotated = document("(;SZ[9](;B[aa];W[bb]C[preserve])(;W[bb]))");
    annotated.apply(Command::Step(2)).unwrap();
    annotated.apply(Command::DeleteStone(point(0, 0))).unwrap();
    assert_tree(&annotated, "(;SZ[9](;W[bb]C[preserve])(;W[bb]))");
}

#[test]
fn editing_a_branch_does_not_merge_duplicate_ancestors_outside_edited_subtree() {
    let mut doc = document("(;SZ[9](;B[aa];W[bb])(;B[aa];W[cc]))");
    doc.apply(Command::Step(1)).unwrap();
    doc.apply(Command::InsertMove(play(Color::White, 3, 3)))
        .unwrap();
    assert_tree(&doc, "(;SZ[9](;B[aa];W[dd];W[bb])(;B[aa];W[cc]))");
}

#[test]
fn replay_merge_never_collapses_unrelated_existing_duplicate_variations() {
    for (input, expected) in [
        (
            "(;SZ[9](;B[aa];W[ab])(;B[cc];W[cd])(;B[cc];W[ce]))",
            "(;SZ[9](;W[ab])(;B[cc];W[cd])(;B[cc];W[ce]))",
        ),
        (
            "(;SZ[9](;B[aa];B[cc];W[ab])(;B[cc];W[cd])(;B[cc];W[ce]))",
            "(;SZ[9](;B[cc](;W[cd])(;W[ab]))(;B[cc];W[ce]))",
        ),
    ] {
        let mut doc = document(input);
        doc.apply(Command::Step(i32::MAX)).unwrap();
        assert_atomic_undo_redo(&mut doc, Command::DeleteStone(point(0, 0)), expected);
    }
}

#[test]
fn malformed_untouched_sibling_does_not_block_editing_a_valid_branch() {
    let mut doc = document("(;SZ[9](;B[aa];W[bb])(;B[zz]))");
    doc.apply(Command::Step(2)).unwrap();
    let before = doc.snapshot();
    doc.apply(Command::SwitchStone(point(0, 0))).unwrap();
    assert_eq!(
        doc.to_sgf(),
        document("(;SZ[9](;W[aa];W[bb])(;B[zz]))").to_sgf()
    );
    doc.position_at(doc.current()).unwrap();
    let after = doc.snapshot();
    doc.undo().unwrap();
    assert_eq!(doc.snapshot(), before);
    doc.redo().unwrap();
    assert_eq!(doc.snapshot(), after);
}

#[test]
fn failed_edits_preserve_content_focus_ids_and_redo() {
    let mut doc = branched();
    doc.apply(Command::InsertMove(play(Color::Black, 7, 7)))
        .unwrap();
    let abandoned_id = doc.current();
    doc.undo().unwrap();
    let before = doc.snapshot();
    let revision = doc.revision();
    for command in [
        Command::InsertMove(play(Color::Black, 0, 0)),
        Command::SwitchStone(point(7, 7)),
        Command::DeleteStone(point(7, 7)),
        Command::MoveStone {
            from: point(0, 0),
            to: point(1, 1),
        },
        Command::MoveStone {
            from: point(0, 0),
            to: point(9, 9),
        },
        Command::AddVariation(vec![play(Color::Black, 3, 3), play(Color::White, 3, 3)]),
        Command::AddVariation(vec![play(Color::White, 3, 3)]),
    ] {
        assert!(doc.apply(command).is_err());
        assert_eq!(doc.snapshot(), before);
        assert_eq!(doc.revision(), revision);
        assert!(doc.can_redo());
    }
    doc.apply(Command::InsertMove(play(Color::Black, 7, 7)))
        .unwrap();
    assert_eq!(doc.current().0, abandoned_id.0 + 1);
    assert!(!doc.can_redo());
}

#[test]
fn target_switch_suicide_and_malformed_continuations_fail_atomically() {
    let mut doc = document("(;SZ[5]AB[ba][ab];B[aa])");
    doc.apply(Command::Step(1)).unwrap();
    let before = doc.snapshot();
    assert!(matches!(
        doc.apply(Command::SwitchStone(point(0, 0))),
        Err(DocError::IllegalMove {
            reason: IllegalMove::Suicide { .. },
            ..
        })
    ));
    assert_eq!(doc.snapshot(), before);

    let mut doc = document("(;SZ[9];B[aa];W[broken])");
    doc.apply(Command::Step(1)).unwrap();
    let before = doc.snapshot();
    assert!(
        doc.apply(Command::InsertMove(play(Color::Black, 1, 1)))
            .is_err()
    );
    assert_eq!(doc.snapshot(), before);
}

#[test]
fn add_variation_keeps_focus_and_creates_duplicate_prefix_as_distinct_branch() {
    let mut doc = branched();
    let focus = doc.current();
    assert_atomic_undo_redo(
        &mut doc,
        Command::AddVariation(vec![
            play(Color::Black, 2, 2),
            Move::Pass(Color::White),
            play(Color::Black, 6, 6),
        ]),
        "(;SZ[9];B[aa];W[bb](;B[cc];W[dd])(;B[ee];W[ff])(;B[cc];W[];B[gg]))",
    );
    assert_eq!(doc.current(), focus);
}

#[test]
fn leaf_variation_does_not_create_dummy_pass_and_empty_list_is_noop() {
    let mut doc = document("(;SZ[9];B[aa])");
    doc.apply(Command::Step(1)).unwrap();
    let focus = doc.current();
    let revision = doc.revision();
    assert_eq!(doc.apply(Command::AddVariation(vec![])).unwrap(), revision);
    assert_eq!(
        doc.apply(Command::MoveStone {
            from: point(0, 0),
            to: point(0, 0)
        })
        .unwrap(),
        revision
    );
    assert!(!doc.can_undo());
    doc.apply(Command::AddVariation(vec![play(Color::White, 1, 1)]))
        .unwrap();
    assert_tree(&doc, "(;SZ[9];B[aa];W[bb])");
    assert_eq!(doc.current(), focus);
}

#[test]
fn java_non_main_leaf_variation_advances_to_end_and_undo_restores_start() {
    let mut doc = document("(;SZ[9];B[aa](;W[bb])(;W[cc];B[dd]))");
    let fork = doc.node(NodeId(1)).unwrap().children()[1];
    let leaf = doc.node(fork).unwrap().children()[0];
    doc.apply(Command::GoTo(leaf)).unwrap();
    assert_atomic_undo_redo(
        &mut doc,
        Command::AddVariation(vec![play(Color::White, 4, 4), play(Color::Black, 5, 5)]),
        "(;SZ[9];B[aa](;W[bb])(;W[cc];B[dd];W[ee];B[ff]))",
    );
    let position = doc.position_at(doc.current()).unwrap();
    assert_eq!(position.moves.last(), Some(&play(Color::Black, 5, 5)));
    assert_eq!(position.moves.len(), 5);
    doc.undo().unwrap();
    assert_eq!(doc.current(), leaf);
}

#[test]
fn origin_is_read_only_branch_aware_and_review_starts_at_parent() {
    let mut doc = branched();
    let fork = doc.current();
    let branch = doc.node(fork).unwrap().children()[1];
    doc.apply(Command::GoTo(branch)).unwrap();
    doc.apply(Command::SetComment("temporary".into())).unwrap();
    doc.undo().unwrap();
    let before = doc.snapshot();
    let revision = doc.revision();
    let origin = doc.stone_origin(point(4, 4)).unwrap().unwrap();
    assert_eq!(origin.node, branch);
    assert_eq!(origin.move_number, 3);
    assert_eq!(origin.review_parent, Some(fork));
    assert_eq!(
        doc.stone_origin(point(0, 0)).unwrap().unwrap().node,
        NodeId(1)
    );
    assert_eq!(doc.stone_origin(point(2, 2)).unwrap(), None);
    assert!(doc.stone_origin(point(9, 0)).is_err());
    assert_eq!(doc.snapshot(), before);
    assert_eq!(doc.revision(), revision);
    assert!(doc.can_redo());
}

#[test]
fn origin_tracks_capture_replay_pass_and_same_color_setup_overwrite() {
    let mut doc = document("(;SZ[5]AB[ab][ba][bc]AW[ca][cc][db];W[bb];B[cb];W[];B[];W[bb])");
    doc.apply(Command::Step(2)).unwrap();
    assert_eq!(doc.stone_origin(point(1, 1)).unwrap(), None);
    assert_eq!(doc.stone_origin(point(0, 1)).unwrap(), None);
    let captured = doc.stone_origin(point(2, 1)).unwrap().unwrap();
    assert_eq!(captured.move_number, 2);
    doc.apply(Command::Step(3)).unwrap();
    let returned = doc.stone_origin(point(1, 1)).unwrap().unwrap();
    assert_eq!(returned.node, doc.current());
    assert_eq!(returned.move_number, 5);
    assert_eq!(doc.stone_origin(point(2, 1)).unwrap(), None);

    let mut setup = document("(;SZ[9];B[aa];AB[aa];W[bb])");
    setup.apply(Command::Step(3)).unwrap();
    assert_eq!(setup.stone_origin(point(0, 0)).unwrap(), None);
    assert!(matches!(
        setup.apply(Command::DeleteStone(point(0, 0))),
        Err(DocError::NoMoveAtPoint(_))
    ));
    let root_move = document("(;SZ[9]B[aa])");
    assert_eq!(
        root_move
            .stone_origin(point(0, 0))
            .unwrap()
            .unwrap()
            .review_parent,
        None
    );
}

#[test]
fn trial_copy_preserves_ko_and_ancestors_but_removes_continuations_and_history() {
    let mut original = document("(;SZ[5]AB[ab][ba][bc]AW[bb][ca][cc][db];B[cb];W[ee])");
    original.apply(Command::Step(1)).unwrap();
    original.apply(Command::SetComment("saved".into())).unwrap();
    original.apply(Command::SetComment("redo".into())).unwrap();
    original.undo().unwrap();
    let before = original.snapshot();
    let mut trial = original.trial_copy().unwrap();
    assert_eq!(trial.current(), original.current());
    assert_eq!(
        trial.position_at(trial.current()).unwrap(),
        original.position_at(original.current()).unwrap()
    );
    assert!(trial.node(trial.current()).unwrap().children().is_empty());
    assert!(!trial.can_undo());
    assert!(!trial.can_redo());
    assert!(matches!(
        trial.apply(Command::PlayColored(play(Color::White, 1, 1))),
        Err(DocError::IllegalMove {
            reason: IllegalMove::Ko { .. },
            ..
        })
    ));
    trial.apply(Command::Step(-1)).unwrap();
    assert_eq!(trial.current(), trial.root());
    trial
        .apply(Command::PlayColored(play(Color::White, 4, 4)))
        .unwrap();
    assert_eq!(original.snapshot(), before);
    assert!(original.can_undo());
    assert!(original.can_redo());
}

#[test]
fn invalid_trial_base_leaves_original_unchanged() {
    let mut original = document("(;SZ[9];B[aa];W[aa])");
    original.apply(Command::Step(2)).unwrap();
    let before = original.snapshot();
    assert!(original.trial_copy().is_err());
    assert_eq!(original.snapshot(), before);
}

#[test]
fn root_recorded_move_can_be_switched_deleted_or_moved_without_removing_metadata() {
    for (command, expected) in [
        (
            Command::SwitchStone(point(0, 0)),
            "(;SZ[9]RU[japanese]KM[6.5]AB[hh]PL[B]W[aa]C[root]XX[raw](;W[bb])(;W[cc]))",
        ),
        (
            Command::DeleteStone(point(0, 0)),
            "(;SZ[9]RU[japanese]KM[6.5]AB[hh]PL[B]C[root]XX[raw](;W[bb])(;W[cc]))",
        ),
        (
            Command::MoveStone {
                from: point(0, 0),
                to: point(3, 3),
            },
            "(;SZ[9]RU[japanese]KM[6.5]AB[hh]PL[B]B[dd]C[root]XX[raw](;W[bb])(;W[cc]))",
        ),
    ] {
        let mut doc =
            document("(;SZ[9]RU[japanese]KM[6.5]AB[hh]PL[B]B[aa]C[root]XX[raw](;W[bb])(;W[cc]))");
        doc.apply(Command::Step(1)).unwrap();
        let root = doc.root();
        let focus = doc.current();
        assert_atomic_undo_redo(&mut doc, command, expected);
        assert_eq!(doc.root(), root);
        assert_eq!(doc.current(), focus);
    }
    let mut minimal = document("(;B[aa])");
    assert_atomic_undo_redo(&mut minimal, Command::DeleteStone(point(0, 0)), "(;)");
    assert_eq!(minimal.current(), minimal.root());
}

#[test]
fn root_stone_edit_validates_original_setup_and_is_atomic_on_illegal_replacement() {
    let mut doc = document("(;SZ[5]AB[ba][ab]B[aa]C[keep])");
    let before = doc.snapshot();
    assert!(matches!(
        doc.apply(Command::SwitchStone(point(0, 0))),
        Err(DocError::IllegalMove {
            reason: IllegalMove::Suicide { .. },
            ..
        })
    ));
    assert_eq!(doc.snapshot(), before);
    assert!(!doc.can_undo());
    assert_eq!(doc.revision(), 0);
    assert!(
        doc.apply(Command::MoveStone {
            from: point(0, 0),
            to: point(1, 0)
        })
        .is_err()
    );
    assert_eq!(doc.snapshot(), before);
}

#[test]
fn play_sequence_continues_as_fresh_branch_preserving_annotations_and_original_line() {
    let mut doc =
        document("(;SZ[9]C[root];B[aa]C[start](;W[bb]C[old]XX[keep];B[cc])(;W[dd]C[side]))");
    doc.apply(Command::Step(1)).unwrap();
    let start = doc.current();
    let original_children = doc.node(start).unwrap().children().to_vec();
    let original_nodes = doc.snapshot().nodes;
    assert_atomic_undo_redo(
        &mut doc,
        Command::PlaySequence(vec![play(Color::White, 1, 1), play(Color::Black, 4, 4)]),
        "(;SZ[9]C[root];B[aa]C[start](;W[bb]C[old]XX[keep];B[cc])(;W[dd]C[side])(;W[bb];B[ee]))",
    );
    let children = doc.node(start).unwrap().children();
    assert_eq!(&children[..2], original_children);
    let fresh = children[2];
    assert_ne!(fresh, original_children[0]);
    assert_eq!(doc.node(doc.current()).unwrap().parent(), Some(fresh));
    assert_eq!(doc.node(fresh).unwrap().comment(), None);
    for original in original_nodes.into_iter().filter(|node| node.id() != start) {
        assert_eq!(doc.node(original.id()).unwrap(), &original);
    }
}

#[test]
fn play_sequence_best_pv_is_one_edit_and_advances_even_at_main_leaf() {
    let mut doc = document("(;SZ[9];B[aa])");
    doc.apply(Command::Step(1)).unwrap();
    let start = doc.current();
    assert_atomic_undo_redo(
        &mut doc,
        Command::PlaySequence(vec![
            play(Color::White, 1, 1),
            Move::Pass(Color::Black),
            play(Color::White, 2, 2),
        ]),
        "(;SZ[9];B[aa];W[bb];B[];W[cc])",
    );
    let final_node = doc.current();
    assert_eq!(doc.position_at(final_node).unwrap().moves.len(), 4);
    assert_ne!(final_node, start);
    doc.undo().unwrap();
    assert_eq!(doc.current(), start);
    assert!(doc.node(start).unwrap().children().is_empty());
    assert!(doc.node(final_node).is_err());
    assert!(!doc.can_undo());
    doc.redo().unwrap();
    assert_eq!(doc.current(), final_node);
}

#[test]
fn play_sequence_invalid_middle_move_is_atomic_and_does_not_consume_ids_or_redo() {
    let mut doc = GameDocument::new();
    doc.apply(Command::PlaySequence(vec![play(Color::Black, 0, 0)]))
        .unwrap();
    let first = doc.current();
    doc.undo().unwrap();
    let before = doc.snapshot();
    let revision = doc.revision();
    for moves in [
        vec![play(Color::Black, 1, 1), play(Color::White, 1, 1)],
        vec![play(Color::Black, 1, 1), play(Color::Black, 2, 2)],
        vec![play(Color::Black, 1, 1), play(Color::White, 19, 1)],
    ] {
        assert!(doc.apply(Command::PlaySequence(moves)).is_err());
        assert_eq!(doc.snapshot(), before);
        assert_eq!(doc.revision(), revision);
        assert!(doc.can_redo());
        assert!(!doc.can_undo());
    }
    assert_eq!(doc.apply(Command::PlaySequence(vec![])).unwrap(), revision);
    assert_eq!(doc.snapshot(), before);
    assert!(doc.can_redo());
    doc.apply(Command::PlaySequence(vec![play(Color::Black, 1, 1)]))
        .unwrap();
    assert_eq!(doc.current().0, first.0 + 1);
    assert!(!doc.can_redo());
}

#[test]
fn play_sequence_cannot_reuse_an_imported_continuation_to_bypass_ko() {
    let mut doc = document("(;SZ[5]AB[ab][ba][bc]AW[bb][ca][cc][db];B[cb];W[bb])");
    doc.apply(Command::Step(1)).unwrap();
    let before = doc.snapshot();
    assert!(matches!(
        doc.apply(Command::PlaySequence(vec![play(Color::White, 1, 1)])),
        Err(DocError::IllegalMove {
            reason: IllegalMove::Ko { .. },
            ..
        })
    ));
    assert_eq!(doc.snapshot(), before);
    assert!(!doc.can_undo());
}
