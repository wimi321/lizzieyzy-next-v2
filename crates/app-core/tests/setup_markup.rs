#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{AppCore, AppEvent, AppSubscription, ColorView, DocumentView, MarkupKindView};
use document::MarkupKind;
use go_core::{Color, Point};
use std::{future::Future, time::Duration};

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("setup/markup safety deadline")
}

async fn view(events: &mut AppSubscription, expected: u64) -> DocumentView {
    within(async {
        loop {
            if let AppEvent::DocumentChanged { revision, view } = events.recv().await.unwrap() {
                assert_eq!(revision, expected);
                return view;
            }
        }
    })
    .await
}

async fn open(bytes: &[u8]) -> (tempfile::TempDir, AppCore, AppSubscription, DocumentView) {
    let dir = tempfile::tempdir().unwrap();
    let (app, mut events) = within(app_core::spawn(dir.path())).await.unwrap();
    let rev = within(app.open_bytes(bytes.to_vec())).await.unwrap();
    let initial = view(&mut events, rev).await;
    (dir, app, events, initial)
}

async fn undo_redo(
    app: &AppCore,
    events: &mut AppSubscription,
    before: &DocumentView,
    after: &DocumentView,
) {
    let rev = within(app.undo()).await.unwrap();
    let undone = view(events, rev).await;
    assert_eq!(undone.board, before.board);
    assert_eq!(undone.markup, before.markup);
    assert_eq!(undone.tree.current, before.tree.current);
    assert_eq!(undone.modified, before.modified);
    assert!(undone.can_redo);
    let rev = within(app.redo()).await.unwrap();
    let redone = view(events, rev).await;
    assert_eq!(redone.board, after.board);
    assert_eq!(redone.markup, after.markup);
    assert_eq!(redone.tree.current, after.tree.current);
    assert_eq!(redone.modified, after.modified);
    assert!(redone.can_undo);
}

#[tokio::test]
async fn setup_commands_replace_erase_clear_and_select_first_player_with_history() {
    let (_dir, app, mut events, mut before) = open(b"(;SZ[9]PB[Black]C[root]PL[W])").await;
    for color in [Some(Color::Black), Some(Color::White), None] {
        let rev = within(app.set_setup_stone(Point { x: 2, y: 3 }, color))
            .await
            .unwrap();
        let after = view(&mut events, rev).await;
        let board = after.board.as_ref().unwrap();
        assert_eq!(board.stones[29], color.map(ColorView::from));
        assert_eq!(board.move_number, 0);
        assert_eq!(board.to_play, ColorView::White);
        assert_eq!(after.game_info, before.game_info);
        undo_redo(&app, &mut events, &before, &after).await;
        before = after;
    }
    let rev = within(app.set_first_player(Color::Black)).await.unwrap();
    let after = view(&mut events, rev).await;
    assert_eq!(after.board.as_ref().unwrap().to_play, ColorView::Black);
    undo_redo(&app, &mut events, &before, &after).await;
    assert_eq!(
        within(app.set_first_player(Color::Black)).await.unwrap(),
        rev + 2
    );
    let rev = within(app.set_setup_stone(Point { x: 0, y: 0 }, Some(Color::Black)))
        .await
        .unwrap();
    view(&mut events, rev).await;
    let rev = within(app.set_setup_stone(Point { x: 1, y: 1 }, Some(Color::White)))
        .await
        .unwrap();
    let before = view(&mut events, rev).await;
    let rev = within(app.clear_setup()).await.unwrap();
    let after = view(&mut events, rev).await;
    assert!(
        after
            .board
            .as_ref()
            .unwrap()
            .stones
            .iter()
            .all(Option::is_none)
    );
    assert_eq!(after.board.as_ref().unwrap().to_play, ColorView::Black);
    assert_eq!(after.comment.as_deref(), Some("root"));
    undo_redo(&app, &mut events, &before, &after).await;
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn setup_errors_preserve_tree_and_zero_liberty_setup_can_be_repaired() {
    let (_dir, app, mut events, before) = open(b"(;SZ[3];B[aa])").await;
    let original = within(app.variation_tree()).await.unwrap();
    assert!(
        within(app.set_setup_stone(Point { x: 1, y: 1 }, Some(Color::Black)))
            .await
            .is_err()
    );
    assert!(within(app.clear_setup()).await.is_err());
    assert!(within(app.set_first_player(Color::White)).await.is_err());
    assert!(within(app.go_to(u32::MAX)).await.is_err());
    assert_eq!(within(app.variation_tree()).await.unwrap(), original);
    let rev = within(app.convert_to_start_position()).await.unwrap();
    let converted = view(&mut events, rev).await;
    assert_eq!(converted.board, before.board);
    for point in [Point { x: 3, y: 0 }, Point { x: 0, y: 3 }] {
        assert!(
            within(app.set_setup_stone(point, Some(Color::Black)))
                .await
                .is_err()
        );
        assert!(within(app.set_setup_stone(point, None)).await.is_err());
        assert!(
            within(app.set_markup(point, MarkupKind::Triangle))
                .await
                .is_err()
        );
    }
    let rev = within(app.open_bytes(b"(;SZ[3]AB[ba][ab][cb][bc])".to_vec()))
        .await
        .unwrap();
    let before = view(&mut events, rev).await;
    // Java setupPlacementIgnoresCaptureAndSuicide: retain setup stones rather
    // than capture. The existing replay validator keeps this board unanalyzable.
    let rev = within(app.set_setup_stone(Point { x: 1, y: 1 }, Some(Color::White)))
        .await
        .unwrap();
    let after = view(&mut events, rev).await;
    assert!(after.board.is_none());
    assert!(after.position_error.is_some());
    undo_redo(&app, &mut events, &before, &after).await;
    assert!(within(app.convert_to_start_position()).await.is_err());
    assert!(within(app.toggle_to_play()).await.is_err());
    let rev = within(app.set_setup_stone(Point { x: 1, y: 1 }, None))
        .await
        .unwrap();
    assert_eq!(view(&mut events, rev).await.board, before.board);
    let rev = within(app.clear_setup()).await.unwrap();
    assert!(
        view(&mut events, rev)
            .await
            .board
            .unwrap()
            .stones
            .iter()
            .all(Option::is_none)
    );
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn every_markup_kind_toggles_with_undo_and_clear_preserves_non_markup_properties() {
    let (_dir, app, mut events, mut before) = open(b"(;SZ[9]C[keep]XX[opaque])").await;
    let cases = [
        (MarkupKind::Triangle, MarkupKindView::Triangle, None),
        (MarkupKind::Square, MarkupKindView::Square, None),
        (MarkupKind::Circle, MarkupKindView::Circle, None),
        (MarkupKind::Cross, MarkupKindView::Cross, None),
        (MarkupKind::Letter, MarkupKindView::Letter, Some("A")),
        (MarkupKind::Number, MarkupKindView::Number, Some("1")),
    ];
    for (x, (kind, expected, text)) in cases.into_iter().enumerate() {
        let point = Point { x: x as u8, y: 0 };
        let rev = within(app.set_markup(point, kind)).await.unwrap();
        let after = view(&mut events, rev).await;
        let mark = after.markup.iter().find(|m| m.point.x == point.x).unwrap();
        assert_eq!(mark.kind, expected);
        assert_eq!(mark.text.as_deref(), text);
        assert_eq!(after.board, before.board);
        assert_eq!(
            after.tree.structure_revision,
            before.tree.structure_revision
        );
        undo_redo(&app, &mut events, &before, &after).await;
        let rev = within(app.set_markup(point, kind)).await.unwrap();
        let removed = view(&mut events, rev).await;
        assert_eq!(removed.markup, before.markup);
        undo_redo(&app, &mut events, &after, &removed).await;
        let rev = within(app.set_markup(point, kind)).await.unwrap();
        before = view(&mut events, rev).await;
    }
    let rev = within(app.set_markup(Point { x: 0, y: 0 }, MarkupKind::Erase))
        .await
        .unwrap();
    let erased = view(&mut events, rev).await;
    assert_eq!(erased.markup.len(), 5);
    undo_redo(&app, &mut events, &before, &erased).await;
    let rev = within(app.clear_markup()).await.unwrap();
    let cleared = view(&mut events, rev).await;
    assert!(cleared.markup.is_empty());
    assert_eq!(cleared.comment.as_deref(), Some("keep"));
    assert!(!cleared.modified);
    undo_redo(&app, &mut events, &erased, &cleared).await;
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn imported_marks_and_setup_roundtrip_untouched_and_edited_properties() {
    let bytes = b"(;SZ[9]AB[aa:ab]AW[cc]PL[W]TR[dd:de]SQ[ff]CR[gg]MA[hh]LB[aa:A][bb:3][cc:hello]TR[ii]XX[raw];W[bb]LB[ee:child])";
    let (dir, app, mut events, initial) = open(bytes).await;
    assert_eq!(initial.markup.len(), 9);
    assert!(
        initial
            .markup
            .iter()
            .any(|m| m.text.as_deref() == Some("hello"))
    );
    let target = dir.path().join("saved.sgf");
    within(app.save(&target)).await.unwrap();
    let revision = within(app.step(0)).await.unwrap();
    view(&mut events, revision).await;
    let serialized = std::fs::read(&target).unwrap();
    let original = sgf::parse(bytes).unwrap();
    let normalized = sgf::parse(sgf::serialize(&original).as_bytes()).unwrap();
    assert_eq!(sgf::parse(&serialized).unwrap().games, normalized.games);
    let rev = within(app.open_path(&target)).await.unwrap();
    let reopened = view(&mut events, rev).await;
    assert_eq!(reopened.markup, initial.markup);
    assert_eq!(reopened.board, initial.board);
    let rev = within(app.step(1)).await.unwrap();
    let child = view(&mut events, rev).await;
    assert_eq!(child.markup.len(), 1);
    assert_eq!(child.markup[0].text.as_deref(), Some("child"));
    let rev = within(app.set_markup(Point { x: 0, y: 1 }, MarkupKind::Number))
        .await
        .unwrap();
    let edited = view(&mut events, rev).await;
    within(app.save(&target)).await.unwrap();
    view(&mut events, rev).await;
    let rev = within(app.open_path(&target)).await.unwrap();
    view(&mut events, rev).await;
    let rev = within(app.step(1)).await.unwrap();
    assert_eq!(view(&mut events, rev).await.markup, edited.markup);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn conversion_replaces_tree_and_file_identity_and_keeps_root_metadata() {
    let (dir, app, mut events, _) = open(
        b"(;SZ[9]KM[6.5]RU[japanese]PB[B]PW[W]C[root]XX[keep]TR[ee];B[aa];W[bb](;B[cc])(;B[dd]))",
    )
    .await;
    let rev = within(app.step(2)).await.unwrap();
    view(&mut events, rev).await;
    let rev = within(app.set_comment("discard current comment".into()))
        .await
        .unwrap();
    let before = view(&mut events, rev).await;
    within(app.save(dir.path().join("original.sgf")))
        .await
        .unwrap();
    view(&mut events, rev).await;
    let rev = within(app.convert_to_start_position()).await.unwrap();
    let after = view(&mut events, rev).await;
    assert!(after.file_path.is_none());
    assert!(after.file_name.is_none());
    assert!(!after.modified);
    assert!(!after.can_undo && !after.can_redo);
    assert!(within(app.undo()).await.is_err());
    assert!(within(app.redo()).await.is_err());
    assert_eq!(after.tree.main_line, [0]);
    assert_eq!(within(app.variation_tree()).await.unwrap().nodes.len(), 1);
    assert_eq!(after.comment.as_deref(), Some("root"));
    assert_eq!(after.markup.len(), 1);
    assert_eq!(after.game_info, before.game_info);
    let board = after.board.as_ref().unwrap();
    assert_eq!(board.stones, before.board.as_ref().unwrap().stones);
    assert_eq!(board.to_play, before.board.as_ref().unwrap().to_play);
    assert_eq!(
        (
            board.move_number,
            board.captured_black,
            board.captured_white
        ),
        (0, 0, 0)
    );
    assert!(board.last_move.is_none());
    let path = dir.path().join("converted.sgf");
    within(app.save(&path)).await.unwrap();
    view(&mut events, rev).await;
    assert!(std::fs::read_to_string(&path).unwrap().contains("XX[keep]"));
    let rev = within(app.open_path(&path)).await.unwrap();
    assert_eq!(view(&mut events, rev).await.board, after.board);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn toggle_turn_persists_on_root_and_after_move_without_fabricating_a_pass() {
    for bytes in [
        b"(;SZ[9])".as_slice(),
        b"(;SZ[9];B[aa](;W[bb])(;W[cc]))".as_slice(),
    ] {
        let (dir, app, mut events, mut before) = open(bytes).await;
        let rev = within(app.step(1)).await.unwrap();
        if before.tree.main_line.len() > 1 {
            before = view(&mut events, rev).await;
        }
        let rev = within(app.toggle_to_play()).await.unwrap();
        let after = view(&mut events, rev).await;
        let b = before.board.as_ref().unwrap();
        let a = after.board.as_ref().unwrap();
        assert_ne!(a.to_play, b.to_play);
        assert_eq!(a.stones, b.stones);
        assert_eq!(a.move_number, b.move_number);
        undo_redo(&app, &mut events, &before, &after).await;
        let node = after.tree.current;
        let path = dir.path().join("toggle.sgf");
        within(app.save(&path)).await.unwrap();
        view(&mut events, rev + 2).await;
        let parsed = sgf::parse(&std::fs::read(&path).unwrap()).unwrap();
        let document = document::GameDocument::from_sgf(&parsed.games[0]).unwrap();
        // IDs may change when variations serialize, so locate the PL snapshot.
        let position = document
            .variation_tree()
            .unwrap()
            .into_iter()
            .filter(|n| {
                document
                    .node(n.id)
                    .unwrap()
                    .properties()
                    .iter()
                    .any(|p| p.identifier == "PL")
            })
            .map(|n| document.position_at(n.id).unwrap())
            .next()
            .unwrap();
        assert_eq!(ColorView::from(position.board.to_play()), a.to_play);
        assert_eq!(position.moves.len(), a.move_number);
        assert_eq!(
            within(app.variation_tree())
                .await
                .unwrap()
                .nodes
                .iter()
                .find(|n| n.id == node)
                .unwrap()
                .mv,
            None
        );
        within(app.shutdown()).await.unwrap();
    }
}

#[tokio::test]
async fn all_new_commands_report_closed_core_and_old_wire_defaults_markup() {
    let (_dir, app, _events, before) = open(b"(;SZ[9])").await;
    let mut wire = serde_json::to_value(before).unwrap();
    wire.as_object_mut().unwrap().remove("markup");
    assert!(
        serde_json::from_value::<DocumentView>(wire)
            .unwrap()
            .markup
            .is_empty()
    );
    within(app.clone().shutdown()).await.unwrap();
    assert!(
        within(app.set_setup_stone(Point { x: 0, y: 0 }, None))
            .await
            .is_err()
    );
    assert!(within(app.clear_setup()).await.is_err());
    assert!(within(app.set_first_player(Color::White)).await.is_err());
    assert!(within(app.convert_to_start_position()).await.is_err());
    assert!(
        within(app.set_markup(Point { x: 0, y: 0 }, MarkupKind::Erase))
            .await
            .is_err()
    );
    assert!(within(app.clear_markup()).await.is_err());
    assert!(within(app.toggle_to_play()).await.is_err());
}
