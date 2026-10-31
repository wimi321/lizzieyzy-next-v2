#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{AppEvent, AppSubscription, ColorView, DocumentView, EngineProfile, Settings};
use document::Command;
use go_core::{Color, Move, Point};
use std::{future::Future, path::Path, time::Duration};

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("operation exceeded its safety deadline")
}

async fn document_event(output: &mut AppSubscription, revision: u64) -> DocumentView {
    within(async {
        loop {
            match output.recv().await.expect("event stream closed") {
                AppEvent::DocumentChanged {
                    revision: actual,
                    view,
                } => {
                    assert_eq!(actual, revision);
                    return view;
                }
                AppEvent::EngineStatusChanged(_) => {}
                AppEvent::BoardSyncStatusChanged(_) => {}
                AppEvent::WholeGameProgress(_) => {
                    panic!("unexpected review without starting a task")
                }
                AppEvent::AnalysisUpdated(_) => panic!("unexpected analysis without an engine"),
            }
        }
    })
    .await
}

fn read_games(path: &Path) -> Vec<sgf::GameTree> {
    sgf::parse(&std::fs::read(path).unwrap()).unwrap().games
}

#[tokio::test]
async fn saved_nested_variations_and_java_properties_reopen_with_identical_trees() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = within(app_core::spawn(directory.path())).await.unwrap();
    for bytes in [
        include_bytes!("../../../fixtures/sgf/nested-variations.sgf").as_slice(),
        include_bytes!("../../../fixtures/sgf/java-saved-lz.sgf").as_slice(),
    ] {
        let revision = within(app.open_bytes(bytes.to_vec())).await.unwrap();
        let opened = document_event(&mut events, revision).await;
        let original_tree = within(app.variation_tree()).await.unwrap();
        assert_eq!(opened.tree.current, opened.tree.root);
        let first = directory.path().join("first.sgf");
        let second = directory.path().join("second.sgf");
        within(app.save(&first)).await.unwrap();
        let source = sgf::parse(bytes).unwrap();
        // The SGF serializer intentionally normalizes only root CA to UTF-8.
        let normalized = sgf::parse(sgf::serialize(&source).as_bytes()).unwrap();
        assert_eq!(read_games(&first), normalized.games);
        let revision = within(app.open_path(&first)).await.unwrap();
        let mut reopened = document_event(&mut events, revision).await;
        let reopened_tree = within(app.variation_tree()).await.unwrap();
        assert_eq!(reopened_tree.nodes, original_tree.nodes);
        assert_eq!(reopened_tree.root, original_tree.root);
        // Reopening creates a fresh document identity even for identical SGF.
        assert!(reopened.tree.structure_revision > opened.tree.structure_revision);
        assert_eq!(reopened_tree.revision, reopened.tree.structure_revision);
        reopened.tree.structure_revision = opened.tree.structure_revision;
        assert_eq!(reopened, opened);
        within(app.save(&second)).await.unwrap();
        assert_eq!(read_games(&second), read_games(&first));
    }
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn collections_open_and_save_only_the_first_game() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = within(app_core::spawn(directory.path())).await.unwrap();
    let bytes = include_bytes!("../../../fixtures/sgf/collection-two.sgf");
    let revision = within(app.open_bytes(bytes.to_vec())).await.unwrap();
    document_event(&mut events, revision).await;
    let target = directory.path().join("selected.sgf");
    within(app.save(&target)).await.unwrap();
    let source = sgf::parse(bytes).unwrap();
    assert_eq!(source.games.len(), 2);
    let normalized = sgf::parse(sgf::serialize(&source).as_bytes()).unwrap();
    assert_eq!(read_games(&target), vec![normalized.games[0].clone()]);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn failed_opens_and_saves_leave_the_existing_document_and_file_intact() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = within(app_core::spawn(directory.path())).await.unwrap();
    let revision = within(app.open_bytes(b"(;SZ[9]C[keep](;B[aa])(;B[bb]))".to_vec()))
        .await
        .unwrap();
    let before = document_event(&mut events, revision).await;
    let target = directory.path().join("original.sgf");
    within(app.save(&target)).await.unwrap();
    let original_bytes = std::fs::read(&target).unwrap();

    assert!(
        within(app.open_bytes(b"(;C[unterminated".to_vec()))
            .await
            .is_err()
    );
    assert!(
        within(app.open_path(directory.path().join("missing.sgf")))
            .await
            .is_err()
    );
    assert!(
        within(app.save(directory.path().join("missing-parent/output.sgf")))
            .await
            .is_err()
    );
    assert_eq!(std::fs::read(&target).unwrap(), original_bytes);
    let directory_target = directory.path().join("existing-directory");
    std::fs::create_dir(&directory_target).unwrap();
    let retained = directory_target.join("retained.txt");
    std::fs::write(&retained, b"retain this file").unwrap();
    assert!(within(app.save(&directory_target)).await.is_err());
    assert_eq!(std::fs::read(&retained).unwrap(), b"retain this file");

    let after = directory.path().join("after-errors.sgf");
    within(app.save(&after)).await.unwrap();
    assert_eq!(std::fs::read(&after).unwrap(), original_bytes);
    let next = within(app.step(1)).await.unwrap();
    assert_eq!(next, revision + 1);
    let view = document_event(&mut events, next).await;
    assert_eq!(view.tree.current, before.tree.main_line[1]);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn play_comment_navigation_undo_and_redo_share_the_document_history() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = within(app_core::spawn(directory.path())).await.unwrap();
    let first = within(app.play(Move::Play(Color::Black, Point { x: 3, y: 4 })))
        .await
        .unwrap();
    let placed = document_event(&mut events, first).await;
    let node = placed.tree.current;
    let board = placed.board.unwrap();
    assert_eq!(board.stones[4 * 19 + 3], Some(ColorView::Black));
    assert_eq!(board.to_play, ColorView::White);

    let commented = within(app.apply(Command::SetComment("a comment".into())))
        .await
        .unwrap();
    assert_eq!(commented, first + 1);
    assert_eq!(
        document_event(&mut events, commented)
            .await
            .comment
            .as_deref(),
        Some("a comment")
    );
    let navigated = within(app.go_to(0)).await.unwrap();
    assert_eq!(document_event(&mut events, navigated).await.tree.current, 0);
    let undone = within(app.undo()).await.unwrap();
    let restored = document_event(&mut events, undone).await;
    assert_eq!(restored.tree.current, node);
    assert_eq!(restored.comment, None);
    let redone = within(app.redo()).await.unwrap();
    let restored = document_event(&mut events, redone).await;
    assert_eq!(restored.tree.current, node);
    assert_eq!(restored.comment.as_deref(), Some("a comment"));
    assert!(within(app.play(Move::Pass(Color::Black))).await.is_err());
    assert!(within(app.go_to(u32::MAX)).await.is_err());
    let final_revision = within(app.play(Move::Pass(Color::White))).await.unwrap();
    assert_eq!(final_revision, redone + 1);
    assert_eq!(
        document_event(&mut events, final_revision)
            .await
            .board
            .unwrap()
            .move_number,
        2
    );
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn illegal_recorded_nodes_remain_navigable_with_warnings() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = within(app_core::spawn(directory.path())).await.unwrap();
    let revision =
        within(app.open_bytes(b"(;SZ[9]RU[unknown](;B[aa];W[aa]C[bad])(;B[bb]))".to_vec()))
            .await
            .unwrap();
    let root = document_event(&mut events, revision).await;
    assert_eq!(root.tree.variation_count, 2);
    assert_eq!(root.warnings.len(), 1);
    let revision = within(app.step(2)).await.unwrap();
    let bad = document_event(&mut events, revision).await;
    assert!(bad.board.is_none());
    assert!(bad.position_error.is_some());
    assert_eq!(bad.comment.as_deref(), Some("bad"));
    assert_eq!(bad.warnings, root.warnings);
    let revision = within(app.go_to(3)).await.unwrap();
    let good = document_event(&mut events, revision).await;
    assert!(good.position_error.is_none());
    assert_eq!(good.board.unwrap().stones[10], Some(ColorView::Black));
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn configured_profile_and_pause_setting_survive_application_restart() {
    let directory = tempfile::tempdir().unwrap();
    let (app, _events) = within(app_core::spawn(directory.path())).await.unwrap();
    assert_eq!(within(app.settings()).await.unwrap(), Settings::default());
    // Missing files are intentionally editable settings, so no process starts.
    let profile = EngineProfile {
        canonical_model_name: None,
        program: directory.path().join("missing-engine"),
        model: directory.path().join("missing-model.bin.gz"),
        config: directory.path().join("analysis.cfg"),
        extra_args: vec!["--threads".into(), "2".into()],
    };
    within(app.configure_engine(Some(profile.clone())))
        .await
        .unwrap();
    within(app.set_pondering(false)).await.unwrap();
    let expected = within(app.settings()).await.unwrap();
    assert_eq!(expected.engine, Some(profile));
    assert!(!expected.pondering);
    within(app.shutdown()).await.unwrap();
    let (reopened, _events) = within(app_core::spawn(directory.path())).await.unwrap();
    assert_eq!(within(reopened.settings()).await.unwrap(), expected);
    within(reopened.configure_engine(None)).await.unwrap();
    within(reopened.set_pondering(true)).await.unwrap();
    within(reopened.shutdown()).await.unwrap();
    assert_eq!(
        within(Settings::load(directory.path())).await.unwrap(),
        Settings::default()
    );
}
