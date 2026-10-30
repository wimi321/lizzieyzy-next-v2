#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{
    AppConfig, AppCore, AppError, AppEvent, AppSubscription, ColorView, DocumentView,
    EngineProfile, EngineStatus, MoveView, PointView, VariationNodeView, VariationTreeView,
};
use document::{Command, DocError, NodeId};
use go_core::{Color, Move, Point};
use std::{future::Future, path::Path, time::Duration};
use tempfile::TempDir;
use tokio::time::timeout;

async fn within<T>(future: impl Future<Output = T>) -> T {
    timeout(Duration::from_secs(5), future)
        .await
        .expect("operation exceeded safety deadline")
}

async fn app(directory: &Path) -> (AppCore, AppSubscription) {
    let mut config = AppConfig::default();
    config.analysis.live.max_visits = 1;
    // No timeout fault is under test, including during the required CPU-load runs.
    config.session.write_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(60);
    config.session.max_restarts = 0;
    within(app_core::spawn_with_config(directory, config))
        .await
        .unwrap()
}

async fn document(output: &mut AppSubscription, revision: u64) -> DocumentView {
    within(async {
        loop {
            match output.recv().await.expect("app event stream closed") {
                AppEvent::DocumentChanged {
                    revision: actual,
                    view,
                } => {
                    assert_eq!(actual, revision);
                    return view;
                }
                AppEvent::AnalysisUpdated(value) => {
                    panic!("analysis arrived before document: {value:?}")
                }
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("engine failed: {reason}")
                }
                _ => {}
            }
        }
    })
    .await
}

async fn first_final(output: &mut AppSubscription, node_id: u32) {
    within(async {
        loop {
            match output.recv().await.unwrap() {
                AppEvent::AnalysisUpdated(value) if !value.is_during_search => {
                    assert_eq!(value.revision, 0);
                    assert_eq!(value.node_id, node_id);
                    assert_eq!(value.visits, 1);
                    return;
                }
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("engine failed: {reason}")
                }
                _ => {}
            }
        }
    })
    .await;
}

fn node(tree: &VariationTreeView, id: u32) -> &VariationNodeView {
    tree.nodes.iter().find(|node| node.id == id).unwrap()
}

fn play(color: Color, x: u8, y: u8) -> Move {
    Move::Play(color, Point { x, y })
}

fn view_play(color: ColorView, x: u8, y: u8) -> MoveView {
    MoveView::Play {
        color,
        point: PointView { x, y },
    }
}

#[tokio::test]
async fn corpus_game_info_and_all_variations_are_exposed_and_new_branch_links_are_correct() {
    let directory = TempDir::new().unwrap();
    let (handle, mut output) = app(directory.path()).await;
    // Retain the checked-in variation corpus and add the information fields
    // missing from that fixture. This tests the real SGF import and IPC views.
    let mut corpus = sgf::parse(include_bytes!(
        "../../../fixtures/sgf/nested-variations.sgf"
    ))
    .unwrap();
    for (identifier, value) in [
        ("PB", "黑棋"),
        ("BR", "九段"),
        ("PW", "White"),
        ("WR", "8d"),
        ("RU", "Japanese"),
        ("HA", "0"),
        ("RE", "B+R"),
        ("DT", "2026-10-30"),
        ("EV", "测试赛事"),
    ] {
        corpus.games[0].sequence[0].properties.push(sgf::Property {
            identifier: identifier.into(),
            values: vec![value.into()],
        });
    }
    let revision = within(handle.open_bytes(sgf::serialize(&corpus).into_bytes()))
        .await
        .unwrap();
    let initial = document(&mut output, revision).await;
    let info = &initial.game_info;
    assert_eq!(info.black_name.as_deref(), Some("黑棋"));
    assert_eq!(info.black_rank.as_deref(), Some("九段"));
    assert_eq!(info.white_name.as_deref(), Some("White"));
    assert_eq!(info.white_rank.as_deref(), Some("8d"));
    assert_eq!(info.komi, 7.5);
    assert_eq!(info.rules.as_deref(), Some("Japanese"));
    assert_eq!(info.handicap, Some(0));
    assert_eq!(info.result.as_deref(), Some("B+R"));
    assert_eq!(info.date.as_deref(), Some("2026-10-30"));
    assert_eq!(info.event.as_deref(), Some("测试赛事"));

    let tree = within(handle.variation_tree()).await.unwrap();
    assert_eq!(tree.revision, initial.tree.structure_revision);
    assert_eq!(tree.root, initial.tree.root);
    assert_eq!(tree.nodes.len(), 6);
    let root = node(&tree, tree.root);
    assert_eq!((root.parent, root.mv, root.move_number), (None, None, 0));
    let first = node(&tree, root.children[0]);
    assert_eq!(first.mv, Some(view_play(ColorView::Black, 4, 4)));
    assert_eq!(first.parent, Some(root.id));
    let main = node(&tree, first.children[0]);
    let alternate = node(&tree, first.children[1]);
    assert_eq!(main.mv, Some(view_play(ColorView::White, 2, 2)));
    assert_eq!(alternate.mv, Some(view_play(ColorView::White, 6, 6)));
    assert_eq!(main.children.len(), 2);
    for (id, coordinate) in main.children.iter().zip([(3, 3), (5, 5)]) {
        let variation = node(&tree, *id);
        assert_eq!(variation.parent, Some(main.id));
        assert_eq!(variation.move_number, 3);
        assert_eq!(
            variation.mv,
            Some(view_play(ColorView::Black, coordinate.0, coordinate.1))
        );
    }
    assert!(tree.nodes.iter().all(|node| node.move_error.is_none()));
    assert_eq!(
        initial.tree.main_line,
        vec![root.id, first.id, main.id, main.children[0]]
    );

    let revision = within(handle.go_to(alternate.id)).await.unwrap();
    let navigated = document(&mut output, revision).await;
    assert_eq!(navigated.game_info, initial.game_info);
    assert_eq!(navigated.tree.structure_revision, tree.revision);
    let revision = within(handle.play(play(Color::Black, 7, 7))).await.unwrap();
    let added = document(&mut output, revision).await;
    let changed = within(handle.variation_tree()).await.unwrap();
    assert_eq!(changed.nodes.len(), tree.nodes.len() + 1);
    assert!(changed.revision > tree.revision);
    assert_eq!(changed.revision, added.tree.structure_revision);
    let child = node(&changed, added.tree.current);
    assert_eq!(child.parent, Some(alternate.id));
    assert_eq!(child.mv, Some(view_play(ColorView::Black, 7, 7)));
    assert_eq!(child.move_number, 3);
    assert_eq!(node(&changed, alternate.id).children, vec![child.id]);
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn metadata_keeps_missing_fields_empty_and_reuses_imported_komi_normalization() {
    let directory = TempDir::new().unwrap();
    let (handle, mut output) = app(directory.path()).await;
    let revision = within(
        handle.open_bytes(include_bytes!("../../../fixtures/sgf/java-saved-lz.sgf").to_vec()),
    )
    .await
    .unwrap();
    let java = document(&mut output, revision).await;
    assert_eq!(java.game_info.black_name.as_deref(), Some("Black"));
    assert_eq!(java.game_info.white_name.as_deref(), Some("White"));
    assert_eq!(java.game_info.date.as_deref(), Some("2000-01-01"));
    assert_eq!(java.game_info.result.as_deref(), Some(""));
    assert_eq!(java.game_info.komi, 6.5);
    assert_eq!(java.game_info.handicap, None);

    let revision = within(handle.open_bytes(
        include_bytes!("../../../fixtures/sgf/fox-export-shape-synthetic.sgf").to_vec(),
    ))
    .await
    .unwrap();
    let fox = document(&mut output, revision).await;
    assert_eq!(fox.game_info.komi, 7.5);
    assert_eq!(fox.game_info.handicap, Some(0));

    let revision = within(handle.open_bytes(b"(;SZ[9];PB[child]BR[9d]RU[Japanese])".to_vec()))
        .await
        .unwrap();
    let missing = document(&mut output, revision).await;
    let info = &missing.game_info;
    assert_eq!(info.komi, 7.5);
    assert_eq!(info.handicap, None);
    for field in [
        &info.black_name,
        &info.black_rank,
        &info.white_name,
        &info.white_rank,
        &info.rules,
        &info.result,
        &info.date,
        &info.event,
    ] {
        assert!(field.is_none());
    }
    let revision = within(handle.step(1)).await.unwrap();
    assert_eq!(document(&mut output, revision).await.game_info, *info);
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn tree_version_changes_only_for_structure_or_successful_document_replacement() {
    let directory = TempDir::new().unwrap();
    let (handle, mut output) = app(directory.path()).await;
    let source = include_bytes!("../../../fixtures/sgf/root-variations.sgf").to_vec();
    let revision = within(handle.open_bytes(source.clone())).await.unwrap();
    let initial = document(&mut output, revision).await;
    let original = within(handle.variation_tree()).await.unwrap();
    let children = node(&original, original.root).children.clone();

    let revision = within(handle.go_to(children[1])).await.unwrap();
    assert_eq!(
        document(&mut output, revision)
            .await
            .tree
            .structure_revision,
        original.revision
    );
    let revision = within(handle.apply(Command::SetComment("new comment".into())))
        .await
        .unwrap();
    assert_eq!(
        document(&mut output, revision)
            .await
            .tree
            .structure_revision,
        original.revision
    );
    for revision in [
        within(handle.undo()).await.unwrap(),
        within(handle.redo()).await.unwrap(),
    ] {
        // The latest-state mailbox can coalesce these edits. Check the read-only
        // tree reply instead of depending on intermediate event delivery.
        assert!(revision > initial.tree.structure_revision);
        assert_eq!(within(handle.variation_tree()).await.unwrap(), original);
    }
    let revision = within(handle.go_to(original.root)).await.unwrap();
    document(&mut output, revision).await;
    let revision = within(handle.play(play(Color::Black, 2, 2))).await.unwrap();
    let existing = document(&mut output, revision).await;
    assert_eq!(existing.tree.current, children[1]);
    assert_eq!(existing.tree.structure_revision, original.revision);

    let revision = within(handle.apply(Command::PromoteVariation(NodeId(children[1]))))
        .await
        .unwrap();
    let promoted = document(&mut output, revision).await;
    let promoted_tree = within(handle.variation_tree()).await.unwrap();
    assert!(promoted_tree.revision > original.revision);
    assert_eq!(promoted.tree.structure_revision, promoted_tree.revision);
    assert_eq!(
        node(&promoted_tree, original.root).children,
        vec![children[1], children[0], children[2]]
    );

    let revision = within(handle.undo()).await.unwrap();
    let undone = document(&mut output, revision).await;
    assert!(undone.tree.structure_revision > promoted_tree.revision);
    let undone_tree = within(handle.variation_tree()).await.unwrap();
    assert_eq!(undone_tree.nodes, original.nodes);
    let revision = within(handle.redo()).await.unwrap();
    let redone = document(&mut output, revision).await;
    assert!(redone.tree.structure_revision > undone.tree.structure_revision);
    assert_eq!(
        within(handle.variation_tree()).await.unwrap().nodes,
        promoted_tree.nodes
    );

    let revision = within(handle.apply(Command::DeleteNode(NodeId(children[1]))))
        .await
        .unwrap();
    let deleted = document(&mut output, revision).await;
    let deleted_tree = within(handle.variation_tree()).await.unwrap();
    assert!(deleted_tree.revision > redone.tree.structure_revision);
    assert_eq!(deleted.tree.current, original.root);
    assert!(!deleted_tree.nodes.iter().any(|node| node.id == children[1]));
    let revision = within(handle.undo()).await.unwrap();
    document(&mut output, revision).await;
    let restored = within(handle.variation_tree()).await.unwrap();
    assert!(restored.revision > deleted_tree.revision);
    assert_eq!(restored.nodes, promoted_tree.nodes);

    assert!(
        within(handle.open_bytes(b"not SGF".to_vec()))
            .await
            .is_err()
    );
    assert_eq!(within(handle.variation_tree()).await.unwrap(), restored);
    let revision = within(handle.open_bytes(source)).await.unwrap();
    let reopened = document(&mut output, revision).await;
    let reopened_tree = within(handle.variation_tree()).await.unwrap();
    assert!(reopened.tree.structure_revision > restored.revision);
    assert_eq!(reopened_tree.nodes, original.nodes);
    assert_eq!(reopened_tree.revision, reopened.tree.structure_revision);
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn tree_includes_root_moves_empty_setup_pass_and_malformed_nodes() {
    let directory = TempDir::new().unwrap();
    let (handle, mut output) = app(directory.path()).await;
    let revision = within(
        handle
            .open_bytes(b"(;SZ[9]B[aa];C[empty];AW[bb]PL[W];W[];B[zz];C[after invalid])".to_vec()),
    )
    .await
    .unwrap();
    let initial = document(&mut output, revision).await;
    let tree = within(handle.variation_tree()).await.unwrap();
    assert_eq!(tree.nodes.len(), 6);
    let expected_moves = [
        Some(view_play(ColorView::Black, 0, 0)),
        None,
        None,
        Some(MoveView::Pass {
            color: ColorView::White,
        }),
        None,
        None,
    ];
    let mut previous = None;
    for (index, id) in initial.tree.main_line.iter().enumerate() {
        let item = node(&tree, *id);
        assert_eq!(item.parent, previous);
        assert_eq!(item.mv, expected_moves[index]);
        assert_eq!(item.move_number, [1, 1, 1, 2, 3, 3][index]);
        assert_eq!(item.move_error.is_some(), index == 4);
        previous = Some(*id);
    }
    let malformed = initial.tree.main_line[4];
    let revision = within(handle.go_to(malformed)).await.unwrap();
    let invalid = document(&mut output, revision).await;
    assert!(invalid.board.is_none());
    assert!(invalid.position_error.is_some());
    assert_eq!(within(handle.variation_tree()).await.unwrap(), tree);
    assert!(
        within(handle.preview_variation(malformed, vec![]))
            .await
            .is_err()
    );
    assert!(matches!(
        within(handle.preview_variation(u32::MAX, vec![])).await,
        Err(AppError::Document(DocError::MissingNode(NodeId(u32::MAX))))
    ));
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn preview_captures_and_stops_before_the_first_illegal_move() {
    let directory = TempDir::new().unwrap();
    let (handle, mut output) = app(directory.path()).await;
    let revision = within(
        handle.open_bytes(include_bytes!("../../../fixtures/sgf/capture-black.sgf").to_vec()),
    )
    .await
    .unwrap();
    let initial = document(&mut output, revision).await;
    let before_tree = within(handle.variation_tree()).await.unwrap();
    let capture = play(Color::Black, 1, 2);
    let pv = vec![capture, play(Color::White, 1, 2), play(Color::Black, 8, 8)];
    let preview = within(handle.preview_variation(initial.tree.current, pv))
        .await
        .unwrap();
    assert_eq!(
        (preview.revision, preview.node_id),
        (revision, initial.tree.current)
    );
    assert_eq!(preview.applied_moves, vec![MoveView::from(capture)]);
    assert!(preview.stop_reason.is_some());
    assert_eq!(preview.board.stones[10], None);
    assert_eq!(preview.board.stones[19], Some(ColorView::Black));
    assert_eq!(preview.board.stones[80], None);
    assert_eq!(preview.board.captured_black, 1);
    assert_eq!(preview.board.captured_white, 0);
    assert_eq!(preview.board.move_number, 1);
    assert_eq!(preview.board.to_play, ColorView::White);
    assert_eq!(preview.stones.len(), 1);
    assert_eq!(preview.stones[0].point, PointView { x: 1, y: 2 });
    assert_eq!(preview.stones[0].color, ColorView::Black);
    assert_eq!(preview.stones[0].move_number, 1);
    let after = within(handle.preview_variation(initial.tree.current, vec![]))
        .await
        .unwrap();
    assert_eq!(after.revision, revision);
    assert_eq!(after.board, initial.board.unwrap());
    assert_eq!(within(handle.variation_tree()).await.unwrap(), before_tree);
    assert!(matches!(
        within(handle.undo()).await,
        Err(AppError::Document(DocError::NothingToUndo))
    ));
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn preview_numbers_skip_passes_and_remove_preview_stones_that_get_captured() {
    let directory = TempDir::new().unwrap();
    let (handle, mut output) = app(directory.path()).await;
    let revision = within(handle.open_bytes(b"(;SZ[5]AW[ba][ab]PL[B])".to_vec()))
        .await
        .unwrap();
    let initial = document(&mut output, revision).await;
    // Black at bb has two liberties; White surrounds the final liberty after
    // a Black pass, so the first numbered preview stone must disappear.
    let moves = vec![
        play(Color::Black, 1, 1),
        play(Color::White, 2, 1),
        Move::Pass(Color::Black),
        play(Color::White, 1, 2),
        play(Color::Black, 4, 4),
    ];
    let preview = within(handle.preview_variation(initial.tree.root, moves.clone()))
        .await
        .unwrap();
    assert!(preview.stop_reason.is_none());
    assert_eq!(
        preview.applied_moves,
        moves.into_iter().map(MoveView::from).collect::<Vec<_>>()
    );
    assert_eq!(preview.board.stones[6], None);
    assert_eq!(preview.board.captured_white, 1);
    assert_eq!(preview.board.move_number, 5);
    assert_eq!(
        preview
            .stones
            .iter()
            .map(|stone| stone.move_number)
            .collect::<Vec<_>>(),
        vec![2, 4, 5]
    );
    assert_eq!(preview.stones[0].point, PointView { x: 2, y: 1 });
    assert_eq!(preview.stones[1].point, PointView { x: 1, y: 2 });
    assert_eq!(preview.stones[2].point, PointView { x: 4, y: 4 });
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn preview_preserves_document_revision_undo_and_redo_history() {
    let directory = TempDir::new().unwrap();
    let (handle, mut output) = app(directory.path()).await;
    let initial = document(&mut output, 0).await;
    let revision = within(handle.play(play(Color::Black, 0, 0))).await.unwrap();
    let played = document(&mut output, revision).await;
    let played_tree = within(handle.variation_tree()).await.unwrap();
    let preview =
        within(handle.preview_variation(played.tree.current, vec![play(Color::White, 1, 1)]))
            .await
            .unwrap();
    assert_eq!(preview.revision, revision);
    assert_eq!(within(handle.variation_tree()).await.unwrap(), played_tree);
    let revision = within(handle.undo()).await.unwrap();
    assert_eq!(revision, preview.revision + 1);
    let undone = document(&mut output, revision).await;
    assert_eq!(undone.board, initial.board);
    assert!(matches!(
        within(handle.undo()).await,
        Err(AppError::Document(DocError::NothingToUndo))
    ));
    let preview =
        within(handle.preview_variation(undone.tree.current, vec![play(Color::Black, 2, 2)]))
            .await
            .unwrap();
    assert_eq!(preview.revision, revision);
    let revision = within(handle.redo()).await.unwrap();
    assert_eq!(revision, preview.revision + 1);
    let redone = document(&mut output, revision).await;
    assert_eq!(redone.tree.current, played.tree.current);
    assert_eq!(redone.board, played.board);
    assert_eq!(
        within(handle.variation_tree()).await.unwrap().nodes,
        played_tree.nodes
    );
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn previews_do_not_submit_engine_queries_or_publish_document_events() {
    let directory = TempDir::new().unwrap();
    let marker = directory.path().join("unexpected-second-query");
    let model = directory.path().join("model.bin");
    let config = directory.path().join("analysis.cfg");
    std::fs::write(&model, "fake model").unwrap();
    std::fs::write(&config, "numAnalysisThreads = 1\n").unwrap();
    let engine = EngineProfile {
        program: fake_katago::binary_path().unwrap(),
        model,
        config,
        extra_args: vec![
            "--visits-per-sec".into(),
            "100000".into(),
            "--crash-after-queries".into(),
            "2".into(),
            "--fault-once-file".into(),
            marker.to_str().unwrap().into(),
        ],
    };
    let (handle, mut output) = app(directory.path()).await;
    let initial = document(&mut output, 0).await;
    within(handle.configure_engine(Some(engine))).await.unwrap();
    first_final(&mut output, initial.tree.root).await;
    for moves in [
        vec![],
        vec![play(Color::Black, 2, 2)],
        vec![Move::Pass(Color::Black)],
    ] {
        let preview = within(handle.preview_variation(initial.tree.current, moves))
            .await
            .unwrap();
        assert_eq!(preview.revision, 0);
    }
    // Shutdown drains the fake engine's stdin before it exits. A second query
    // would create the marker even if a later transition cleared its events.
    within(handle.shutdown()).await.unwrap();
    assert!(!marker.exists(), "preview submitted an analysis query");
    while let Some(event) = within(output.recv()).await {
        assert!(matches!(
            event,
            AppEvent::EngineStatusChanged(EngineStatus::Stopped)
        ));
    }

    // Positive control: the persisted profile starts another fake process.
    // A real document edit must submit query two and create the same marker,
    // proving that absence above did not come from a misconfigured fault flag.
    let (control, mut output) = app(directory.path()).await;
    let initial = document(&mut output, 0).await;
    first_final(&mut output, initial.tree.root).await;
    let revision = within(control.play(play(Color::Black, 1, 1)))
        .await
        .unwrap();
    document(&mut output, revision).await;
    within(async {
        loop {
            if let AppEvent::EngineStatusChanged(EngineStatus::Failed { .. }) =
                output.recv().await.unwrap()
            {
                break;
            }
        }
    })
    .await;
    assert!(marker.is_file(), "real edit did not submit query two");
    within(control.shutdown()).await.unwrap();
}
