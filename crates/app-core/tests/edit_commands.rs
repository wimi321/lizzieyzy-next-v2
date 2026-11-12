#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{AppCore, AppEvent, AppSubscription, DocumentView, GameInfoEdit, NewGameOptions};
use std::{future::Future, path::Path, time::Duration};

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("safety deadline")
}

async fn document(events: &mut AppSubscription) -> DocumentView {
    within(async {
        loop {
            if let AppEvent::DocumentChanged { view, .. } = events.recv().await.unwrap() {
                return view;
            }
        }
    })
    .await
}

async fn open(directory: &Path, bytes: &[u8]) -> (AppCore, AppSubscription, DocumentView) {
    let (app, mut events) = within(app_core::spawn(directory)).await.unwrap();
    within(app.open_bytes(bytes.to_vec())).await.unwrap();
    let view = document(&mut events).await;
    (app, events, view)
}

fn info() -> GameInfoEdit {
    GameInfoEdit {
        black_name: Some("黑方".into()),
        black_rank: Some("3d".into()),
        white_name: Some("白方".into()),
        white_rank: Some("2d".into()),
        komi: 3.75,
        rules: "japanese".into(),
        result: Some("B+R".into()),
        date: Some("2026-11-11".into()),
        event: Some("测试".into()),
    }
}

#[tokio::test]
async fn comments_undo_redo_and_markers_do_not_change_structure() {
    let dir = tempfile::tempdir().unwrap();
    let (app, mut events, before) = open(dir.path(), b"(;SZ[9];B[aa])").await;
    let revision = within(app.set_comment("hello".into())).await.unwrap();
    let changed = document(&mut events).await;
    assert!(changed.modified);
    assert_eq!(changed.comment.as_deref(), Some("hello"));
    assert_eq!(
        changed.tree.structure_revision,
        before.tree.structure_revision
    );
    assert_eq!(changed.tree.comment_revision, revision);
    let tree = within(app.variation_tree()).await.unwrap();
    assert_eq!(tree.comment_revision, revision);
    assert!(tree.nodes[0].has_comment);
    assert!(!tree.nodes[1].has_comment);

    within(app.undo()).await.unwrap();
    let undone = document(&mut events).await;
    assert!(!undone.modified);
    assert!(undone.comment.is_none());
    assert_eq!(
        undone.tree.structure_revision,
        before.tree.structure_revision
    );
    assert!(!within(app.variation_tree()).await.unwrap().nodes[0].has_comment);
    within(app.redo()).await.unwrap();
    let redone = document(&mut events).await;
    assert!(redone.modified);
    assert_eq!(redone.comment, changed.comment);

    let marker_revision = redone.tree.comment_revision;
    within(app.set_comment("different text".into()))
        .await
        .unwrap();
    assert_eq!(
        document(&mut events).await.tree.comment_revision,
        marker_revision
    );
    within(app.set_comment(String::new())).await.unwrap();
    let removed = document(&mut events).await;
    assert!(!removed.modified);
    assert!(removed.comment.is_none());
    assert!(!within(app.variation_tree()).await.unwrap().nodes[0].has_comment);
    within(app.undo()).await.unwrap();
    assert_eq!(
        document(&mut events).await.comment.as_deref(),
        Some("different text")
    );
    within(app.redo()).await.unwrap();
    assert!(!document(&mut events).await.modified);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn deletion_and_path_promotion_each_restore_content_in_one_undo() {
    // The selected leaf has two non-main ancestors; promotion must reorder both.
    let bytes = b"(;SZ[9](;B[aa])(;B[bb](;W[cc])(;W[dd];B[ee])))";
    for operation in ["delete_node", "delete_branch", "promote_to_main"] {
        let dir = tempfile::tempdir().unwrap();
        let (app, mut events, _) = open(dir.path(), bytes).await;
        let original_tree = within(app.variation_tree()).await.unwrap();
        let leaf = original_tree.nodes.last().unwrap().id;
        within(app.go_to(leaf)).await.unwrap();
        assert!(!document(&mut events).await.modified);
        let revision = match operation {
            "delete_node" => within(app.delete_node(leaf)).await,
            "delete_branch" => within(app.delete_branch(leaf)).await,
            _ => within(app.promote_to_main(leaf)).await,
        }
        .unwrap();
        let changed = document(&mut events).await;
        assert!(changed.modified);
        assert_eq!(changed.tree.structure_revision, revision);
        let changed_tree = within(app.variation_tree()).await.unwrap();
        if operation == "promote_to_main" {
            assert_eq!(changed.tree.main_line.last(), Some(&leaf));
            assert_eq!(changed.tree.current, leaf);
        } else {
            assert!(changed_tree.nodes.iter().all(|node| node.id != leaf));
            assert_ne!(changed.tree.current, leaf);
        }
        within(app.undo()).await.unwrap();
        let undone = document(&mut events).await;
        assert!(!undone.modified);
        assert_eq!(undone.tree.current, leaf);
        assert_eq!(
            within(app.variation_tree()).await.unwrap().nodes,
            original_tree.nodes
        );
        within(app.redo()).await.unwrap();
        let redone = document(&mut events).await;
        assert!(redone.modified);
        assert_eq!(redone.tree.current, changed.tree.current);
        assert_eq!(
            within(app.variation_tree()).await.unwrap().nodes,
            changed_tree.nodes
        );
        within(app.shutdown()).await.unwrap();
    }
}

#[tokio::test]
async fn invalid_nodes_root_deletions_and_new_game_values_leave_state_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let (app, mut events, before) = open(dir.path(), b"(;SZ[9]C[keep];B[aa])").await;
    let tree = within(app.variation_tree()).await.unwrap();
    for id in [0, u32::MAX] {
        assert!(within(app.delete_node(id)).await.is_err());
        assert!(within(app.delete_branch(id)).await.is_err());
        assert!(within(app.promote_to_main(id)).await.is_err());
    }
    for options in [
        NewGameOptions {
            width: 1,
            ..Default::default()
        },
        NewGameOptions {
            height: 26,
            ..Default::default()
        },
        NewGameOptions {
            handicap: 1,
            ..Default::default()
        },
        NewGameOptions {
            handicap: 10,
            ..Default::default()
        },
        NewGameOptions {
            width: 9,
            height: 13,
            handicap: 2,
            ..Default::default()
        },
        NewGameOptions {
            rules: "unknown".into(),
            ..Default::default()
        },
        NewGameOptions {
            komi: f64::NAN,
            ..Default::default()
        },
    ] {
        assert!(within(app.new_game(options)).await.is_err());
    }
    let mut invalid_info = info();
    invalid_info.komi = f64::INFINITY;
    assert!(within(app.set_game_info(invalid_info)).await.is_err());
    assert_eq!(within(app.variation_tree()).await.unwrap(), tree);
    // A successful navigation exposes the unchanged content and save baseline.
    within(app.step(1)).await.unwrap();
    let after = document(&mut events).await;
    assert!(!after.modified);
    assert_eq!(after.game_info, before.game_info);
    assert_eq!(
        after.tree.structure_revision,
        before.tree.structure_revision
    );
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn edited_game_info_roundtrips_fox_komi_and_undo_restores_saved_content() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = include_bytes!("../../../fixtures/sgf/fox-recorded-2026-10-30.sgf");
    let (app, mut events, original) = open(dir.path(), bytes).await;
    let updated = info();
    within(app.set_game_info(updated.clone())).await.unwrap();
    let changed = document(&mut events).await;
    assert!(changed.modified);
    assert_eq!(changed.game_info.komi, updated.komi);
    assert_eq!(changed.game_info.black_name, updated.black_name);
    assert_eq!(changed.game_info.event, updated.event);
    assert_eq!(
        changed.tree.structure_revision,
        original.tree.structure_revision
    );
    within(app.undo()).await.unwrap();
    let undone = document(&mut events).await;
    assert!(!undone.modified);
    assert_eq!(undone.game_info, original.game_info);
    within(app.redo()).await.unwrap();
    let redone = document(&mut events).await;
    assert!(redone.modified);
    assert_eq!(redone.game_info, changed.game_info);
    let path = dir.path().join("edited.sgf");
    within(app.save(&path)).await.unwrap();
    assert!(!document(&mut events).await.modified);
    let parsed = sgf::parse(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        parsed.games[0].sequence[0]
            .properties
            .iter()
            .find(|p| p.identifier == "KM")
            .unwrap()
            .values,
        ["3.75"]
    );
    within(app.open_path(&path)).await.unwrap();
    let reopened = document(&mut events).await;
    assert_eq!(reopened.game_info, changed.game_info);
    assert!(!reopened.modified);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn new_game_handicap_matches_java_star_order_and_roundtrips_all_metadata() {
    for size in [9u8, 13, 19] {
        for handicap in 2u8..=9 {
            let dir = tempfile::tempdir().unwrap();
            let (app, mut events, _) = open(dir.path(), b"(;C[old];B[aa])").await;
            let options = NewGameOptions {
                width: size,
                height: size,
                handicap,
                komi: 0.5,
                rules: "chinese".into(),
                black_name: Some("B".into()),
                black_rank: Some("1d".into()),
                white_name: Some("W".into()),
                white_rank: Some("2d".into()),
            };
            within(app.new_game(options)).await.unwrap();
            let view = document(&mut events).await;
            assert!(view.file_path.is_none());
            assert!(view.file_name.is_none());
            assert!(!view.modified);
            assert_eq!(view.tree.main_line, [0]);
            let board = view.board.as_ref().unwrap();
            assert_eq!(board.width, size);
            assert_eq!(board.to_play, app_core::ColorView::White);
            let low = if size == 9 { 2 } else { 3 };
            let high = size - 1 - low;
            let middle = size / 2;
            let mut expected = vec![(high, low), (low, high)];
            if handicap >= 3 {
                expected.push((low, low));
            }
            if handicap >= 4 {
                expected.push((high, high));
            }
            if matches!(handicap, 5 | 7 | 9) {
                expected.push((middle, middle));
            }
            if handicap >= 6 {
                expected.extend([(low, middle), (high, middle)]);
            }
            if handicap >= 8 {
                expected.extend([(middle, low), (middle, high)]);
            }
            for y in 0..size {
                for x in 0..size {
                    let expected_stone = expected
                        .contains(&(x, y))
                        .then_some(app_core::ColorView::Black);
                    assert_eq!(
                        board.stones[usize::from(y) * usize::from(size) + usize::from(x)],
                        expected_stone,
                        "size {size}, handicap {handicap}, point {x},{y}"
                    );
                }
            }
            let target = dir.path().join("new.sgf");
            within(app.save(&target)).await.unwrap();
            document(&mut events).await;
            within(app.open_path(&target)).await.unwrap();
            let reopened = document(&mut events).await;
            assert_eq!(reopened.game_info, view.game_info);
            assert_eq!(reopened.board, view.board);
            within(app.shutdown()).await.unwrap();
        }
    }
}

#[tokio::test]
async fn saved_baseline_ignores_navigation_and_tracks_undo_redo_after_save_as() {
    let dir = tempfile::tempdir().unwrap();
    let (app, mut events, _) = open(dir.path(), b"(;SZ[9];B[aa];W[bb])").await;
    let first = dir.path().join("first.sgf");
    within(app.save(&first)).await.unwrap();
    let saved = document(&mut events).await;
    assert_eq!(saved.file_path.as_deref(), first.to_str());
    within(app.step(1)).await.unwrap();
    assert!(!document(&mut events).await.modified);
    within(app.set_comment("saved edit".into())).await.unwrap();
    assert!(document(&mut events).await.modified);
    let second = dir.path().join("second.sgf");
    within(app.save(&second)).await.unwrap();
    let saved = document(&mut events).await;
    assert_eq!(saved.file_path.as_deref(), second.to_str());
    assert_eq!(saved.file_name.as_deref(), Some("second.sgf"));
    assert!(!saved.modified);
    within(app.undo()).await.unwrap();
    assert!(document(&mut events).await.modified);
    within(app.redo()).await.unwrap();
    assert!(!document(&mut events).await.modified);
    within(app.go_to(0)).await.unwrap();
    assert!(!document(&mut events).await.modified);
    assert!(
        within(app.save(dir.path().join("missing/fail.sgf")))
            .await
            .is_err()
    );
    within(app.step(1)).await.unwrap();
    let unchanged = document(&mut events).await;
    assert_eq!(unchanged.file_path.as_deref(), second.to_str());
    assert!(!unchanged.modified);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn puzzle_exit_restores_original_file_path_and_unsaved_baseline() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.sgf");
    std::fs::write(&source, "(;SZ[19]KM[6.5]AB[aa][ab][ba]AW[bb]PL[W];W[bc])").unwrap();
    let (app, mut events) = within(app_core::spawn(dir.path())).await.unwrap();
    within(app.open_path(&source)).await.unwrap();
    document(&mut events).await;
    within(app.set_comment("unsaved original".into()))
        .await
        .unwrap();
    let original = document(&mut events).await;
    assert!(original.modified);
    for _ in 0..2 {
        within(app.generate_tsumego(app_core::TsumegoOptions::default()))
            .await
            .unwrap();
        let generated = document(&mut events).await;
        assert!(generated.file_path.is_none());
        assert!(!generated.modified);
        within(app.save(dir.path().join("puzzle.sgf")))
            .await
            .unwrap();
        document(&mut events).await;
    }
    within(app.exit_tsumego()).await.unwrap();
    let restored = document(&mut events).await;
    assert_eq!(restored.file_path, original.file_path);
    assert_eq!(restored.file_name, original.file_name);
    assert!(restored.modified);
    assert_eq!(restored.comment, original.comment);
    within(app.undo()).await.unwrap();
    assert!(!document(&mut events).await.modified);
    within(app.shutdown()).await.unwrap();
}
