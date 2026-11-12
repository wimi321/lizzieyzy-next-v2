#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{AppCore, AppEvent, AppSubscription, ColorView, DocumentView, MoveView};
use go_core::{Color, Move, Point};
use std::{future::Future, time::Duration};

const BRANCHES: &[u8] = b"(;SZ[9];B[aa];W[bb](;B[cc];W[dd])(;B[ee];W[ff]))";

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("move editing safety deadline")
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

async fn history(
    app: &AppCore,
    events: &mut AppSubscription,
    before: &DocumentView,
    before_nodes: &[app_core::VariationNodeView],
    after: &DocumentView,
) {
    let after_nodes = within(app.variation_tree()).await.unwrap().nodes;
    let rev = within(app.undo()).await.unwrap();
    let undone = view(events, rev).await;
    assert_eq!(undone.board, before.board);
    assert_eq!(undone.tree.current, before.tree.current);
    assert_eq!(undone.modified, before.modified);
    assert_eq!(
        within(app.variation_tree()).await.unwrap().nodes,
        before_nodes
    );
    assert!(undone.can_redo);
    let rev = within(app.redo()).await.unwrap();
    let redone = view(events, rev).await;
    assert_eq!(redone.board, after.board);
    assert_eq!(redone.tree.current, after.tree.current);
    assert_eq!(redone.modified, after.modified);
    assert_eq!(
        within(app.variation_tree()).await.unwrap().nodes,
        after_nodes
    );
    assert!(redone.can_undo);
}

#[tokio::test]
async fn colored_moves_allow_consecutive_color_but_keep_legality_and_single_edit_history() {
    let (dir, app, mut events, mut before) = open(b"(;SZ[9])").await;
    for mv in [
        Move::Play(Color::White, Point { x: 0, y: 0 }),
        Move::Play(Color::White, Point { x: 1, y: 0 }),
        Move::Pass(Color::White),
    ] {
        let nodes = within(app.variation_tree()).await.unwrap().nodes;
        let rev = within(app.play_colored(mv)).await.unwrap();
        let after = view(&mut events, rev).await;
        assert_eq!(after.board.as_ref().unwrap().to_play, ColorView::Black);
        history(&app, &mut events, &before, &nodes, &after).await;
        before = after;
    }
    let tree = within(app.variation_tree()).await.unwrap();
    assert!(
        within(app.play_colored(Move::Play(Color::Black, Point { x: 0, y: 0 })))
            .await
            .is_err()
    );
    assert!(
        within(app.play_colored(Move::Play(Color::White, Point { x: 9, y: 0 })))
            .await
            .is_err()
    );
    assert_eq!(within(app.variation_tree()).await.unwrap(), tree);
    let path = dir.path().join("colored.sgf");
    within(app.save(&path)).await.unwrap();
    let bytes = std::fs::read_to_string(path).unwrap();
    assert!(bytes.contains(";W[aa];W[ba];W[]"));
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn java_insert_retains_both_continuations_and_focuses_the_inserted_move() {
    let (_dir, app, mut events, _) = open(BRANCHES).await;
    let rev = within(app.step(2)).await.unwrap();
    let before = view(&mut events, rev).await;
    let nodes = within(app.variation_tree()).await.unwrap().nodes;
    let old_children = nodes
        .iter()
        .find(|node| node.id == before.tree.current)
        .unwrap()
        .children
        .clone();
    let rev = within(app.insert_move(Move::Play(Color::Black, Point { x: 7, y: 7 })))
        .await
        .unwrap();
    let after = view(&mut events, rev).await;
    assert_eq!(
        after.board.as_ref().unwrap().stones[70],
        Some(ColorView::Black)
    );
    let tree = within(app.variation_tree()).await.unwrap();
    let inserted = tree
        .nodes
        .iter()
        .find(|node| node.id == after.tree.current)
        .unwrap();
    assert_eq!(inserted.children, old_children);
    assert_eq!(inserted.parent, Some(before.tree.current));
    for (child, expected) in old_children
        .iter()
        .zip([Point { x: 2, y: 2 }, Point { x: 4, y: 4 }])
    {
        assert_eq!(
            tree.nodes.iter().find(|node| node.id == *child).unwrap().mv,
            Some(MoveView::from(Move::Play(Color::Black, expected)))
        );
    }
    history(&app, &mut events, &before, &nodes, &after).await;
    assert!(
        within(app.insert_move(Move::Play(Color::White, Point { x: 7, y: 7 })))
            .await
            .is_err()
    );
    assert!(
        within(app.insert_move(Move::Play(Color::White, Point { x: 9, y: 0 })))
            .await
            .is_err()
    );
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn java_switch_delete_and_move_reconnect_branched_record_and_undo_in_one_step() {
    for operation in ["switch", "delete", "move"] {
        let (_dir, app, mut events, _) = open(BRANCHES).await;
        let rev = within(app.step(2)).await.unwrap();
        let before = view(&mut events, rev).await;
        let nodes = within(app.variation_tree()).await.unwrap().nodes;
        let origin = within(app.stone_origin(Point { x: 0, y: 0 }))
            .await
            .unwrap()
            .unwrap();
        let source = Point { x: 0, y: 0 };
        let target = Point { x: 7, y: 7 };
        let rev = match operation {
            "switch" => within(app.switch_stone(source)).await,
            "delete" => within(app.delete_stone(source)).await,
            _ => within(app.move_stone(source, target)).await,
        }
        .unwrap();
        let after = view(&mut events, rev).await;
        assert_eq!(after.tree.current, before.tree.current);
        let stones = &after.board.as_ref().unwrap().stones;
        assert_eq!(
            stones[0],
            if operation == "switch" {
                Some(ColorView::White)
            } else {
                None
            }
        );
        assert_eq!(
            stones[70],
            if operation == "move" {
                Some(ColorView::Black)
            } else {
                None
            }
        );
        let tree = within(app.variation_tree()).await.unwrap();
        // Board.editMove / Board.moveToNew rebuild after the edited move;
        // both later cc->dd and ee->ff branches remain in their original order.
        for old in nodes
            .iter()
            .filter(|n| n.id != origin.node_id && n.id != before.tree.root)
        {
            let changed = tree.nodes.iter().find(|n| n.id == old.id).unwrap();
            assert_eq!(changed.children, old.children);
            assert_eq!(changed.mv, old.mv);
        }
        if operation == "delete" {
            assert!(!tree.nodes.iter().any(|n| n.id == origin.node_id));
            assert_eq!(
                tree.nodes
                    .iter()
                    .find(|n| n.id == before.tree.current)
                    .unwrap()
                    .parent,
                Some(before.tree.root)
            );
        }
        history(&app, &mut events, &before, &nodes, &after).await;
        let tree = within(app.variation_tree()).await.unwrap();
        let missing = Point { x: 8, y: 8 };
        assert!(within(app.switch_stone(missing)).await.is_err());
        assert!(within(app.delete_stone(missing)).await.is_err());
        assert!(within(app.move_stone(missing, source)).await.is_err());
        assert!(
            within(app.move_stone(Point { x: 1, y: 1 }, Point { x: 9, y: 0 }))
                .await
                .is_err()
        );
        assert_eq!(within(app.variation_tree()).await.unwrap(), tree);
        within(app.shutdown()).await.unwrap();
    }
}

#[tokio::test]
async fn add_variation_keeps_focus_and_creates_distinct_same_prefix_branch_atomically() {
    let (_dir, app, mut events, _) = open(BRANCHES).await;
    let rev = within(app.step(2)).await.unwrap();
    let before = view(&mut events, rev).await;
    let nodes = within(app.variation_tree()).await.unwrap().nodes;
    let moves = vec![
        Move::Play(Color::Black, Point { x: 2, y: 2 }),
        Move::Play(Color::White, Point { x: 6, y: 6 }),
    ];
    let rev = within(app.add_variation(moves)).await.unwrap();
    let after = view(&mut events, rev).await;
    assert_eq!(after.board, before.board);
    assert_eq!(after.tree.current, before.tree.current);
    assert_eq!(after.tree.variation_count, 3);
    let tree = within(app.variation_tree()).await.unwrap();
    let parent = tree
        .nodes
        .iter()
        .find(|n| n.id == before.tree.current)
        .unwrap();
    let first = tree
        .nodes
        .iter()
        .find(|n| n.id == parent.children[0])
        .unwrap();
    let last = tree
        .nodes
        .iter()
        .find(|n| n.id == parent.children[2])
        .unwrap();
    assert_eq!(first.mv, last.mv);
    assert_ne!(first.id, last.id);
    history(&app, &mut events, &before, &nodes, &after).await;
    let before_error = within(app.variation_tree()).await.unwrap();
    assert!(
        within(app.add_variation(vec![
            Move::Play(Color::Black, Point { x: 8, y: 8 }),
            Move::Play(Color::White, Point { x: 8, y: 8 })
        ]))
        .await
        .is_err()
    );
    assert_eq!(within(app.variation_tree()).await.unwrap(), before_error);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn java_add_variation_extends_side_branch_leaf_and_focuses_its_end_only() {
    // LizzieFrame.addSuggestionAsBranch delegates a non-main-trunk leaf to
    // playCurrentVariation. A main-trunk leaf keeps its original focus.
    for (leaf_point, advances) in [(Point { x: 5, y: 5 }, true), (Point { x: 3, y: 3 }, false)] {
        let (_dir, app, mut events, _) = open(BRANCHES).await;
        let tree = within(app.variation_tree()).await.unwrap();
        let leaf = tree
            .nodes
            .iter()
            .find(|node| node.mv == Some(MoveView::from(Move::Play(Color::White, leaf_point))))
            .unwrap();
        let rev = within(app.go_to(leaf.id)).await.unwrap();
        let before = view(&mut events, rev).await;
        let nodes = within(app.variation_tree()).await.unwrap().nodes;
        let moves = vec![
            Move::Play(Color::Black, Point { x: 7, y: 7 }),
            Move::Play(Color::White, Point { x: 8, y: 8 }),
        ];
        let rev = within(app.add_variation(moves)).await.unwrap();
        let after = view(&mut events, rev).await;
        let tree = within(app.variation_tree()).await.unwrap();
        let source = tree.nodes.iter().find(|node| node.id == leaf.id).unwrap();
        assert_eq!(source.children.len(), 1);
        let first = tree
            .nodes
            .iter()
            .find(|node| node.id == source.children[0])
            .unwrap();
        assert_eq!(first.children.len(), 1);
        let end = first.children[0];
        assert_eq!(
            after.tree.current,
            if advances { end } else { before.tree.current }
        );
        if advances {
            let board = after.board.as_ref().unwrap();
            assert_eq!(board.stones[70], Some(ColorView::Black));
            assert_eq!(board.stones[80], Some(ColorView::White));
            assert_eq!(
                board.move_number,
                before.board.as_ref().unwrap().move_number + 2
            );
        } else {
            assert_eq!(after.board, before.board);
        }
        history(&app, &mut events, &before, &nodes, &after).await;
        within(app.shutdown()).await.unwrap();
    }
}

#[tokio::test]
async fn origin_query_reports_jump_and_review_targets_without_changing_revision_or_history() {
    let (_dir, app, mut events, _) = open(b"(;SZ[9]AB[hh];B[aa];W[bb])").await;
    let rev = within(app.step(2)).await.unwrap();
    let before = view(&mut events, rev).await;
    let tree = within(app.variation_tree()).await.unwrap();
    let origin = within(app.stone_origin(Point { x: 0, y: 0 }))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(origin.revision, rev);
    assert_eq!(origin.move_number, 1);
    assert_eq!(origin.review_parent, Some(before.tree.root));
    assert_eq!(origin.node_id, tree.nodes[1].id);
    assert_eq!(
        within(app.stone_origin(Point { x: 7, y: 7 }))
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        within(app.stone_origin(Point { x: 8, y: 8 }))
            .await
            .unwrap(),
        None
    );
    assert!(
        within(app.stone_origin(Point { x: 9, y: 0 }))
            .await
            .is_err()
    );
    assert_eq!(within(app.step(0)).await.unwrap(), rev);
    assert_eq!(within(app.variation_tree()).await.unwrap(), tree);
    assert!(within(app.undo()).await.is_err());
    within(app.clone().shutdown()).await.unwrap();
    let mv = Move::Play(Color::Black, Point { x: 0, y: 0 });
    assert!(within(app.play_colored(mv)).await.is_err());
    assert!(within(app.insert_move(mv)).await.is_err());
    assert!(
        within(app.switch_stone(Point { x: 0, y: 0 }))
            .await
            .is_err()
    );
    assert!(
        within(app.delete_stone(Point { x: 0, y: 0 }))
            .await
            .is_err()
    );
    assert!(
        within(app.move_stone(Point { x: 0, y: 0 }, Point { x: 1, y: 1 }))
            .await
            .is_err()
    );
    assert!(within(app.add_variation(vec![mv])).await.is_err());
    assert!(
        within(app.stone_origin(Point { x: 0, y: 0 }))
            .await
            .is_err()
    );
}
