#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{AppEvent, AppSubscription, BoardView, DocumentView, TreeView};
use document::{Command, GameDocument, NodeId};
use go_core::{Color, Move, Point};
use std::time::Duration;
use ts_rs::TS;

fn document(source: &[u8]) -> GameDocument {
    GameDocument::from_sgf(&sgf::parse(source).unwrap().games[0]).unwrap()
}

fn view_at(document: &mut GameDocument, node: u32) -> DocumentView {
    document.apply(Command::GoTo(NodeId(node))).unwrap();
    DocumentView::from_document(document, 0).unwrap()
}

#[test]
fn display_arrays_preserve_rectangular_order_setup_captures_and_pass_numbers() {
    let mut doc = document(b"(;SZ[3:5]AB[ba]AW[aa];B[ab];W[];B[ce])");
    let view = view_at(&mut doc, 3);
    let board = view.board.unwrap();
    let mut numbers = vec![None; 15];
    let mut nodes = vec![None; 15];
    numbers[3] = Some(1);
    nodes[3] = Some(1);
    numbers[14] = Some(3);
    nodes[14] = Some(3);
    assert_eq!(board.stone_move_numbers, numbers);
    assert_eq!(board.stone_node_ids, nodes);
    assert_eq!(board.stone_move_numbers.len(), board.stones.len());
    assert_eq!(board.captured_black, 1);
    assert_eq!(board.move_number, 3);
    assert_eq!(view.graph.points[view.graph.current_index].move_number, 3);
    assert_eq!(view.tree.branch_start_move_number, None);
    let json = serde_json::to_value(&board).unwrap();
    assert_eq!(json["stone_move_numbers"][14], 3);
    assert_eq!(json["stone_node_ids"][0], serde_json::Value::Null);
    assert_eq!(serde_json::from_value::<BoardView>(json).unwrap(), board);
}

#[test]
fn branch_start_counts_root_moves_passes_and_empty_nodes_and_survives_illegal_records() {
    let mut doc = document(b"(;SZ[9]B[aa];W[](;B[bb])(;C[branch];B[cc](;W[dd])(;W[aa])))");
    let nodes = doc.variation_tree().unwrap();
    let empty = nodes.iter().find(|n| n.has_comment).unwrap().id.0;
    let branch_move = doc.node(NodeId(empty)).unwrap().children()[0].0;
    let nested = doc.node(NodeId(branch_move)).unwrap().children()[1].0;
    for node in [0, 1, 2] {
        assert_eq!(view_at(&mut doc, node).tree.branch_start_move_number, None);
    }
    for node in [empty, branch_move, nested] {
        let view = view_at(&mut doc, node);
        assert_eq!(view.tree.branch_start_move_number, Some(3));
        if node == nested {
            assert!(view.board.is_none());
            assert!(view.position_error.is_some());
        }
    }
    doc.apply(Command::PromoteToMain(NodeId(nested))).unwrap();
    assert_eq!(
        view_at(&mut doc, nested).tree.branch_start_move_number,
        None
    );
    doc.undo().unwrap();
    assert_eq!(
        view_at(&mut doc, nested).tree.branch_start_move_number,
        Some(3)
    );
}

#[test]
fn display_view_wire_fields_are_optional_for_old_clients() {
    let original = DocumentView::from_document(&GameDocument::new(), 0).unwrap();
    let mut json = serde_json::to_value(original).unwrap();
    let board = json["board"].as_object_mut().unwrap();
    board.remove("stone_move_numbers");
    board.remove("stone_node_ids");
    json["tree"]
        .as_object_mut()
        .unwrap()
        .remove("branch_start_move_number");
    let old = serde_json::from_value::<DocumentView>(json).unwrap();
    assert!(old.board.as_ref().unwrap().stone_move_numbers.is_empty());
    assert!(old.board.unwrap().stone_node_ids.is_empty());
    assert_eq!(old.tree.branch_start_move_number, None);
    assert!(BoardView::decl().contains("stone_move_numbers?: Array<number | null>"));
    assert!(BoardView::decl().contains("stone_node_ids?: Array<number | null>"));
    assert!(TreeView::decl().contains("branch_start_move_number?: number | null"));
}

async fn next_document(events: &mut AppSubscription) -> DocumentView {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let AppEvent::DocumentChanged { view, .. } = events.recv().await.unwrap() {
                return view;
            }
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn preview_board_reports_pv_numbers_without_inventing_document_nodes() {
    let dir = tempfile::tempdir().unwrap();
    let (app, mut events) = app_core::spawn(dir.path()).await.unwrap();
    app.open_bytes(b"(;SZ[5];B[aa];W[];B[cc])".to_vec())
        .await
        .unwrap();
    app.step(3).await.unwrap();
    let before = next_document(&mut events).await;
    let moves = vec![
        Move::Pass(Color::White),
        Move::Play(Color::Black, Point { x: 4, y: 4 }),
    ];
    let preview = app
        .preview_variation(before.tree.current, moves.clone())
        .await
        .unwrap();
    assert_eq!(preview.board.move_number, 5);
    assert_eq!(preview.applied_moves.len(), 2);
    assert_eq!(preview.board.stone_move_numbers[0], Some(1));
    assert_eq!(preview.board.stone_node_ids[0], Some(1));
    assert_eq!(preview.board.stone_move_numbers[12], Some(3));
    assert_eq!(preview.board.stone_node_ids[12], Some(3));
    assert_eq!(preview.board.stone_move_numbers[24], Some(2));
    assert_eq!(preview.board.stone_node_ids[24], None);
    assert_eq!(preview.stones[0].move_number, 2);
    assert_eq!(preview.board.stone_move_numbers.len(), 25);
    assert_eq!(preview.board.stone_node_ids.len(), 25);
    // A second read is identical and no preview node was added to the document.
    assert_eq!(
        app.preview_variation(before.tree.current, moves)
            .await
            .unwrap(),
        preview
    );
    assert_eq!(app.variation_tree().await.unwrap().nodes.len(), 4);
    app.step(-1).await.unwrap();
    let navigated = next_document(&mut events).await;
    assert_eq!(navigated.tree.current, 2);
    assert_eq!(navigated.board.unwrap().stone_move_numbers[24], None);
    app.shutdown().await.unwrap();
}
