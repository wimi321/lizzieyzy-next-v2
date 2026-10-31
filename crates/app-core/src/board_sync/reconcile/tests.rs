use super::*;
use readboard::{Cell, RemoteContext};

fn document(text: &str) -> GameDocument {
    let collection = sgf::parse(text.as_bytes()).unwrap();
    let mut document = GameDocument::from_sgf(&collection.games[0]).unwrap();
    document.apply(Command::Step(i32::MAX)).unwrap();
    document
}

fn snapshot(document: &GameDocument, marked: Option<(Color, Point)>) -> Snapshot {
    let position = document.position_at(document.current()).unwrap();
    let mut cells: Vec<_> = position
        .board
        .stones()
        .iter()
        .map(|stone| match stone {
            None => Cell::Empty,
            Some(Color::Black) => Cell::Black,
            Some(Color::White) => Cell::White,
        })
        .collect();
    if let Some((color, point)) = marked {
        let index =
            usize::from(point.y) * usize::from(position.board.size().width) + usize::from(point.x);
        cells[index] = if color == Color::Black {
            Cell::LastBlack
        } else {
            Cell::LastWhite
        };
    }
    Snapshot {
        size: position.board.size(),
        cells,
        fox_move_number: None,
        force_rebuild: false,
        context: RemoteContext {
            last_move_source: Some("redBlueMarker".into()),
            ..Default::default()
        },
    }
}

fn play(document: &mut GameDocument, color: Color, x: u8, y: u8) -> Snapshot {
    let point = Point { x, y };
    document
        .apply(Command::Play(Move::Play(color, point)))
        .unwrap();
    snapshot(document, Some((color, point)))
}

#[test]
fn marked_moves_append_real_nodes_and_duplicate_frame_is_idempotent() {
    let mut local = document("(;SZ[5])");
    let mut remote = local.clone();
    let mut reconciler = Reconciler::default();
    let first = play(&mut remote, Color::Black, 0, 0);
    let one = reconciler.apply(&mut local, &first, true, false).unwrap();
    assert!(one.changed && !one.rebuilt && !one.held);
    let second = play(&mut remote, Color::White, 1, 1);
    reconciler.apply(&mut local, &second, true, false).unwrap();
    assert_eq!(local.position_at(local.current()).unwrap().moves.len(), 2);
    let revision = local.revision();
    let repeat = reconciler.apply(&mut local, &second, true, false).unwrap();
    assert!(!repeat.changed && !repeat.rebuilt);
    assert_eq!(local.revision(), revision);
    assert_eq!(local.snapshot().nodes.len(), 3);
}

#[test]
fn single_move_capture_is_verified_by_rules_and_preserves_history() {
    let mut local = document("(;SZ[5]AB[ba][ab][cb]AW[bb]PL[B])");
    let mut remote = local.clone();
    let mut reconciler = Reconciler::default();
    let frame = play(&mut remote, Color::Black, 1, 2);
    let result = reconciler.apply(&mut local, &frame, true, false).unwrap();
    assert!(!result.rebuilt && !result.held);
    let position = local.position_at(local.current()).unwrap();
    assert_eq!(
        position.moves,
        vec![Move::Play(Color::Black, Point { x: 1, y: 2 })]
    );
    assert_eq!(position.captured_by.black, 1);
    assert_eq!(position.board.stones(), frame.stones());
}

#[test]
fn changed_position_requires_two_identical_conflicts_before_rebuild() {
    let mut local = document("(;SZ[5]PB[Alice]PW[Bob]KM[6.5]RU[japanese];B[aa];W[bb])");
    let remote = document("(;SZ[5]AB[dd]AW[ee]PL[B])");
    let frame = snapshot(&remote, Some((Color::White, Point { x: 4, y: 4 })));
    let mut reconciler = Reconciler::default();
    let original = local.snapshot();
    let first = reconciler.apply(&mut local, &frame, true, false).unwrap();
    assert!(first.held && !first.changed);
    assert_eq!(local.snapshot(), original);
    let second = reconciler.apply(&mut local, &frame, true, false).unwrap();
    assert!(second.rebuilt && second.changed && !second.held);
    let position = local.position_at(local.root()).unwrap();
    assert!(position.moves.is_empty());
    assert_eq!(position.initial_stones.len(), 2);
    assert_eq!(position.board.stones(), frame.stones());
    assert_eq!(position.komi, 6.5);
    assert_eq!(position.rules, go_core::RulePreset::Japanese);
    assert_eq!(
        local.game_info().unwrap().black_name.as_deref(),
        Some("Alice")
    );
    assert_eq!(local.snapshot().nodes.len(), 1);
    assert!(
        !reconciler
            .apply(&mut local, &frame, true, false)
            .unwrap()
            .rebuilt
    );
}

#[test]
fn a_different_conflict_restarts_confirmation() {
    let mut local = document("(;SZ[5];B[aa];W[bb])");
    let first = snapshot(&document("(;SZ[5]AB[dd])"), None);
    let second = snapshot(&document("(;SZ[5]AB[ee])"), None);
    let mut reconciler = Reconciler::default();
    assert!(
        reconciler
            .apply(&mut local, &first, true, false)
            .unwrap()
            .held
    );
    assert!(
        reconciler
            .apply(&mut local, &second, true, false)
            .unwrap()
            .held
    );
    assert!(
        reconciler
            .apply(&mut local, &second, true, false)
            .unwrap()
            .rebuilt
    );
}

#[test]
fn consistency_option_does_not_disable_correction_or_override_local_browsing() {
    for always_keep in [false, true] {
        let mut local = document("(;SZ[5];B[aa])");
        let mut remote = local.clone();
        let mut reconciler = Reconciler::default();
        let initial = snapshot(&remote, Some((Color::Black, Point { x: 0, y: 0 })));
        reconciler
            .apply(&mut local, &initial, always_keep, false)
            .unwrap();
        local.apply(Command::GoTo(local.root())).unwrap();
        let viewed = local.current();
        let frame = play(&mut remote, Color::White, 1, 1);
        let result = reconciler
            .apply(&mut local, &frame, always_keep, false)
            .unwrap();
        assert_eq!(local.current(), viewed);
        assert_ne!(result.resolved_node, viewed);
        assert_eq!(
            local.position_at(result.resolved_node).unwrap().moves.len(),
            2
        );
        assert!(
            !reconciler
                .apply(&mut local, &frame, always_keep, false)
                .unwrap()
                .changed
        );
        assert_eq!(local.current(), viewed);
        let conflict = snapshot(&document("(;SZ[5]AB[ee])"), None);
        assert!(
            reconciler
                .apply(&mut local, &conflict, always_keep, false)
                .unwrap()
                .held
        );
        assert!(
            reconciler
                .apply(&mut local, &conflict, always_keep, false)
                .unwrap()
                .rebuilt
        );
    }
}

#[test]
fn always_goto_last_follows_new_moves_but_not_duplicate_frames() {
    let mut local = document("(;SZ[5];B[aa])");
    let mut remote = local.clone();
    let mut reconciler = Reconciler::default();
    local.apply(Command::GoTo(local.root())).unwrap();
    let frame = play(&mut remote, Color::White, 1, 1);
    let result = reconciler.apply(&mut local, &frame, false, true).unwrap();
    assert_eq!(local.current(), result.resolved_node);
    local.apply(Command::GoTo(local.root())).unwrap();
    assert!(
        !reconciler
            .apply(&mut local, &frame, false, true)
            .unwrap()
            .changed
    );
    assert_eq!(local.current(), local.root());
}

#[test]
fn fox_history_matching_reuses_nodes_and_tolerates_only_marker_color_jitter() {
    let mut local = document("(;SZ[5];B[aa];W[bb];B[cc])");
    let mut reconciler = Reconciler::default();
    let initial = snapshot(&local, Some((Color::Black, Point { x: 2, y: 2 })));
    reconciler.apply(&mut local, &initial, true, false).unwrap();
    let remote = document("(;SZ[5];B[aa])");
    let mut frame = snapshot(&remote, Some((Color::White, Point { x: 0, y: 0 })));
    frame.fox_move_number = Some(1);
    frame.context.platform = Some("fox".into());
    let result = reconciler.apply(&mut local, &frame, true, false).unwrap();
    assert!(!result.rebuilt && !result.held);
    assert_eq!(local.position_at(local.current()).unwrap().moves.len(), 1);
    assert_eq!(local.snapshot().nodes.len(), 4);
    assert_eq!(
        local
            .position_at(local.current())
            .unwrap()
            .board
            .stone_at(Point { x: 0, y: 0 }),
        Some(Color::Black)
    );
}

#[test]
fn markerless_fox_single_step_and_remote_number_after_setup_append() {
    let mut local = document("(;SZ[5])");
    let mut reconciler = Reconciler::default();
    let mut remote = document("(;SZ[5]AB[aa][bb]AW[cc]PL[W])");
    let mut first = snapshot(&remote, Some((Color::Black, Point { x: 1, y: 1 })));
    first.fox_move_number = Some(99);
    first.context.platform = Some("fox".into());
    assert!(
        reconciler
            .apply(&mut local, &first, true, false)
            .unwrap()
            .rebuilt
    );
    let mut second = play(&mut remote, Color::White, 3, 3);
    second.fox_move_number = Some(100);
    second.context.platform = Some("fox".into());
    second.cells[3 * 5 + 3] = Cell::White;
    let result = reconciler.apply(&mut local, &second, true, false).unwrap();
    assert!(!result.rebuilt && !result.held);
    assert_eq!(local.position_at(local.current()).unwrap().moves.len(), 1);
    assert!(
        !reconciler
            .apply(&mut local, &second, true, false)
            .unwrap()
            .changed
    );
}

#[test]
fn generic_markerless_change_is_not_misrepresented_as_a_known_move() {
    let mut local = document("(;SZ[5])");
    let frame = snapshot(&document("(;SZ[5]AB[cc])"), None);
    let result = Reconciler::default()
        .apply(&mut local, &frame, true, false)
        .unwrap();
    assert!(result.rebuilt);
    assert!(local.position_at(local.current()).unwrap().moves.is_empty());
}

#[test]
fn force_rebuild_and_room_change_do_not_wait_for_conflict_confirmation() {
    for force in [false, true] {
        let mut local = document("(;SZ[5];B[aa])");
        let mut reconciler = Reconciler::default();
        let mut first = snapshot(&local, Some((Color::Black, Point { x: 0, y: 0 })));
        first.context.room_token = Some("room-a".into());
        reconciler.apply(&mut local, &first, true, false).unwrap();
        let mut second = first;
        second.force_rebuild = force;
        if !force {
            second.context.room_token = Some("room-b".into());
        }
        assert!(
            reconciler
                .apply(&mut local, &second, true, false)
                .unwrap()
                .rebuilt
        );
    }
}

#[test]
fn invalid_recognition_keeps_document_and_undo_history() {
    let mut local = document("(;SZ[5];B[aa])");
    local.apply(Command::SetComment("keep".into())).unwrap();
    let before = local.snapshot();
    let mut frame = snapshot(&local, None);
    frame.size = go_core::BoardSize::new(2, 2).unwrap();
    frame.cells = vec![Cell::Black; 4];
    frame.force_rebuild = true;
    assert!(
        Reconciler::default()
            .apply(&mut local, &frame, true, false)
            .is_err()
    );
    assert_eq!(local.snapshot(), before);
    assert!(local.can_undo());
}

#[test]
fn rectangular_snapshots_keep_row_major_orientation_and_handicap_turn() {
    let mut local = document("(;SZ[19])");
    let mut frame = snapshot(&document("(;SZ[5:7]AB[ae][dg]PL[W])"), None);
    frame.fox_move_number = Some(0);
    frame.context.platform = Some("fox".into());
    assert!(
        Reconciler::default()
            .apply(&mut local, &frame, true, false)
            .unwrap()
            .rebuilt
    );
    let board = local.position_at(local.root()).unwrap().board;
    assert_eq!(board.size(), go_core::BoardSize::new(5, 7).unwrap());
    assert_eq!(board.stone_at(Point { x: 0, y: 4 }), Some(Color::Black));
    assert_eq!(board.stone_at(Point { x: 3, y: 6 }), Some(Color::Black));
    assert_eq!(board.to_play(), Color::White);
}

#[test]
fn fox_title_disagreement_is_not_trusted_for_history_recovery() {
    let mut local = document("(;SZ[5];B[aa];W[bb])");
    let mut frame = snapshot(
        &document("(;SZ[5];B[aa])"),
        Some((Color::Black, Point { x: 0, y: 0 })),
    );
    frame.context.platform = Some("fox".into());
    frame.fox_move_number = Some(1);
    frame.context.live_title_move = Some(2);
    let before = local.snapshot();
    assert!(
        Reconciler::default()
            .apply(&mut local, &frame, true, false)
            .unwrap()
            .held
    );
    assert_eq!(local.snapshot(), before);
}

#[test]
fn lagging_fox_title_does_not_destroy_a_just_accepted_move() {
    let mut local = document("(;SZ[5])");
    let mut remote = local.clone();
    let mut reconciler = Reconciler::default();
    let mut frame = play(&mut remote, Color::Black, 0, 0);
    frame.context.platform = Some("fox".into());
    frame.context.room_token = Some("room".into());
    frame.fox_move_number = Some(1);
    frame.context.live_title_move = Some(1);
    reconciler.apply(&mut local, &frame, true, false).unwrap();
    let before = local.snapshot();
    frame.fox_move_number = Some(0);
    frame.context.live_title_move = Some(0);
    let result = reconciler.apply(&mut local, &frame, true, false).unwrap();
    assert!(!result.changed && !result.rebuilt && !result.held);
    assert_eq!(local.snapshot(), before);
}

#[test]
fn markerless_fox_capture_is_rebuilt_instead_of_claiming_a_verified_move() {
    let mut local = document("(;SZ[5]AB[ba][ab][cb]AW[bb]PL[B])");
    let mut remote = local.clone();
    let mut reconciler = Reconciler::default();
    let mut initial = snapshot(&remote, None);
    initial.context.platform = Some("fox".into());
    initial.fox_move_number = Some(0);
    reconciler.apply(&mut local, &initial, true, false).unwrap();
    let mut captured = play(&mut remote, Color::Black, 1, 2);
    captured.cells[2 * 5 + 1] = Cell::Black;
    captured.context.platform = Some("fox".into());
    captured.fox_move_number = Some(1);
    assert!(
        reconciler
            .apply(&mut local, &captured, true, false)
            .unwrap()
            .rebuilt
    );
    assert!(local.position_at(local.current()).unwrap().moves.is_empty());
}
