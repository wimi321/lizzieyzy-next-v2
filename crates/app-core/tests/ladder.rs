#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
use app_core::{
    AnalysisToolsError, AppError, AppEvent, AppSubscription, DocumentView, LadderOutcome,
};
use common::within;

const ROOT: &str = ";SZ[9]AB[cb][db][bc][cd]AW[cc]PL[W]";

async fn view(events: &mut AppSubscription, revision: u64) -> DocumentView {
    within(async {
        loop {
            if let AppEvent::DocumentChanged {
                revision: current,
                view,
            } = events.recv().await.unwrap()
                && current == revision
            {
                return view;
            }
        }
    })
    .await
}

#[tokio::test]
async fn ladder_creates_branch_or_reuses_prefix_and_all_new_moves_undo_once() {
    for continuation in [";W[aa]", ";W[dc]C[keep]"] {
        let directory = tempfile::tempdir().unwrap();
        let (app, mut events) = within(app_core::spawn(directory.path())).await.unwrap();
        let revision = within(app.open_bytes(format!("({ROOT}{continuation})").into_bytes()))
            .await
            .unwrap();
        let before = view(&mut events, revision).await;
        let original = within(app.variation_tree()).await.unwrap();
        let result = within(app.continue_ladder()).await.unwrap();
        assert_eq!(result.outcome, LadderOutcome::Captured);
        assert!(result.moves > 5);
        let tree = within(app.variation_tree()).await.unwrap();
        let after = view(&mut events, tree.revision).await;
        assert_eq!(
            after.board.as_ref().unwrap().move_number as usize,
            result.moves
        );
        assert_eq!(
            after.board.as_ref().unwrap().to_play,
            app_core::ColorView::White
        );
        let root = tree.nodes.iter().find(|n| n.id == tree.root).unwrap();
        assert_eq!(
            root.children.len(),
            if continuation.contains("aa") { 2 } else { 1 }
        );
        assert_eq!(root.children[0], original.nodes[0].children[0]);
        if continuation.contains("dc") {
            assert!(
                tree.nodes
                    .iter()
                    .find(|n| n.id == root.children[0])
                    .unwrap()
                    .has_comment
            );
        }
        let saved = directory.path().join("ladder.sgf");
        within(app.save(&saved)).await.unwrap();
        let parsed = sgf::parse(&std::fs::read(saved).unwrap()).unwrap();
        let document = document::GameDocument::from_sgf(&parsed.games[0]).unwrap();
        let final_board = document
            .position_at(document::NodeId(after.tree.current))
            .unwrap()
            .board;
        assert_eq!(
            final_board.liberties_at(go_core::Point { x: 2, y: 2 }),
            Some(1)
        );
        let revision = within(app.undo()).await.unwrap();
        let restored = view(&mut events, revision).await;
        assert_eq!(restored.board, before.board);
        assert_eq!(
            within(app.variation_tree()).await.unwrap().nodes,
            original.nodes
        );
        assert!(!restored.can_undo);
        let revision = within(app.redo()).await.unwrap();
        let redone = view(&mut events, revision).await;
        assert_eq!(redone.board, after.board);
        assert_eq!(
            within(app.variation_tree()).await.unwrap().nodes,
            tree.nodes
        );
        within(app.go_to(0)).await.unwrap();
        let before_reuse = within(app.variation_tree()).await.unwrap().nodes;
        let repeated = within(app.continue_ladder()).await.unwrap();
        assert_eq!(repeated, result);
        assert_eq!(
            within(app.variation_tree()).await.unwrap().nodes,
            before_reuse
        );
        within(app.shutdown()).await.unwrap();
    }
}

#[tokio::test]
async fn not_ladder_is_typed_and_preserves_tree_revision_and_undo_history() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = within(app_core::spawn(directory.path())).await.unwrap();
    let revision = within(app.open_bytes(b"(;SZ[9];B[aa])".to_vec()))
        .await
        .unwrap();
    let before = view(&mut events, revision).await;
    let tree = within(app.variation_tree()).await.unwrap();
    assert!(matches!(
        within(app.continue_ladder()).await,
        Err(AppError::AnalysisTools(AnalysisToolsError::NotLadder))
    ));
    assert_eq!(within(app.variation_tree()).await.unwrap(), tree);
    assert_eq!(within(app.step(0)).await.unwrap(), revision);
    assert!(!before.can_undo);
    assert!(within(app.undo()).await.is_err());
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn ladder_escape_appends_the_countercapture_and_can_be_undone() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = within(app_core::spawn(directory.path())).await.unwrap();
    let revision = within(app.open_bytes(b"(;SZ[5]AB[ba][ab][bc]AW[bb][ac][ad]PL[W])".to_vec()))
        .await
        .unwrap();
    let before = view(&mut events, revision).await;
    let result = within(app.continue_ladder()).await.unwrap();
    assert_eq!(result.outcome, LadderOutcome::Escaped);
    assert_eq!(result.moves, 1);
    let tree = within(app.variation_tree()).await.unwrap();
    let after = view(&mut events, tree.revision).await;
    assert_eq!(
        after.board.as_ref().unwrap().stones[0],
        Some(app_core::ColorView::White)
    );
    assert_eq!(after.board.as_ref().unwrap().stones[5], None);
    let revision = within(app.undo()).await.unwrap();
    assert_eq!(view(&mut events, revision).await.board, before.board);
    within(app.shutdown()).await.unwrap();
}
