#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{
    AppCore, AppError, AppEvent, AppSubscription, DocumentView, NewGameOptions, TrialError,
    TsumegoOptions,
};
use go_core::{Color, Move, Point};
use std::{future::Future, time::Duration};

const SOURCE: &[u8] =
    b"(;SZ[19]KM[6.5]RU[japanese]PB[original]AB[aa][ab][ba]AW[bb]PL[W];W[bc](;B[ac])(;B[cc]))";

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("trial safety deadline")
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
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = within(app_core::spawn(directory.path())).await.unwrap();
    let revision = within(app.open_bytes(bytes.to_vec())).await.unwrap();
    let initial = view(&mut events, revision).await;
    (directory, app, events, initial)
}

#[tokio::test]
async fn trial_restores_original_tree_focus_dirty_baseline_and_both_history_stacks() {
    for initially_dirty in [false, true] {
        let (dir, app, mut events, _) = open(SOURCE).await;
        let revision = within(app.step(1)).await.unwrap();
        view(&mut events, revision).await;
        let revision = within(app.set_comment("kept".into())).await.unwrap();
        view(&mut events, revision).await;
        let original_file = dir.path().join("original.sgf");
        within(app.save(&original_file)).await.unwrap();
        view(&mut events, revision).await;
        if initially_dirty {
            let revision = within(app.set_comment("dirty kept".into())).await.unwrap();
            view(&mut events, revision).await;
        }
        let revision = within(app.play(Move::Play(Color::Black, Point { x: 3, y: 3 })))
            .await
            .unwrap();
        view(&mut events, revision).await;
        let revision = within(app.undo()).await.unwrap();
        let original = view(&mut events, revision).await;
        assert_eq!(original.modified, initially_dirty);
        assert!(original.can_undo && original.can_redo);
        let original_tree = within(app.variation_tree()).await.unwrap();
        let original_bytes = std::fs::read(&original_file).unwrap();
        let recent = within(app.recent_files()).await.unwrap();

        let revision = within(app.start_trial()).await.unwrap();
        let trial = view(&mut events, revision).await;
        assert!(trial.trial);
        assert!(!trial.can_undo && !trial.can_redo);
        assert!(!trial.modified);
        assert!(trial.file_path.is_none() && trial.file_name.is_none());
        assert_eq!(trial.board, original.board);
        assert_eq!(trial.tree.current, original.tree.current);
        assert_eq!(trial.tree.variation_count, 0);
        assert_eq!(within(app.start_trial()).await.unwrap(), revision);
        assert!(within(app.undo()).await.is_err());
        assert!(within(app.redo()).await.is_err());

        let revision = within(app.step(-1)).await.unwrap();
        assert!(
            view(&mut events, revision).await.board.unwrap().move_number
                < original.board.as_ref().unwrap().move_number
        );
        let revision = within(app.go_to(original.tree.current)).await.unwrap();
        view(&mut events, revision).await;
        let revision = within(app.play(Move::Play(Color::Black, Point { x: 4, y: 4 })))
            .await
            .unwrap();
        let played = view(&mut events, revision).await;
        assert!(played.modified && played.can_undo);
        let revision = within(app.undo()).await.unwrap();
        assert_eq!(view(&mut events, revision).await.board, trial.board);
        let revision = within(app.redo()).await.unwrap();
        assert_eq!(view(&mut events, revision).await.board, played.board);

        let export = dir.path().join("trial.sgf");
        within(app.save(&export)).await.unwrap();
        assert!(std::fs::read_to_string(&export).unwrap().contains("B[ee]"));
        assert!(within(app.save(&export)).await.is_err());
        assert!(within(app.save(&original_file)).await.is_err());
        assert_eq!(std::fs::read(&original_file).unwrap(), original_bytes);
        assert_eq!(within(app.recent_files()).await.unwrap(), recent);

        let revision = within(app.end_trial()).await.unwrap();
        let restored = view(&mut events, revision).await;
        assert!(!restored.trial);
        assert_eq!(restored.board, original.board);
        assert_eq!(restored.tree.current, original.tree.current);
        assert_eq!(restored.comment, original.comment);
        assert_eq!(restored.game_info, original.game_info);
        assert_eq!(restored.file_path, original.file_path);
        assert_eq!(restored.file_name, original.file_name);
        assert_eq!(restored.modified, original.modified);
        assert_eq!((restored.can_undo, restored.can_redo), (true, true));
        assert_eq!(
            within(app.variation_tree()).await.unwrap().nodes,
            original_tree.nodes
        );
        assert_eq!(within(app.end_trial()).await.unwrap(), revision);
        let revision = within(app.redo()).await.unwrap();
        assert_eq!(
            view(&mut events, revision).await.board.unwrap().stones[60],
            Some(app_core::ColorView::Black)
        );
        let revision = within(app.undo()).await.unwrap();
        assert_eq!(view(&mut events, revision).await.board, original.board);
        within(app.shutdown()).await.unwrap();
    }
}

#[tokio::test]
async fn trial_and_tsumego_reject_each_other_without_losing_either_restore_point() {
    let (_dir, app, mut events, _) = open(SOURCE).await;
    let revision = within(app.step(1)).await.unwrap();
    let original = view(&mut events, revision).await;
    let revision = within(app.start_trial()).await.unwrap();
    let trial = view(&mut events, revision).await;
    assert!(matches!(
        within(app.generate_tsumego(TsumegoOptions::default())).await,
        Err(AppError::Trial(TrialError::TsumegoConflict))
    ));
    assert!(!within(app.tsumego_status()).await.unwrap().active);
    assert_eq!(within(app.step(0)).await.unwrap(), revision);
    assert_eq!(within(app.exit_tsumego()).await.unwrap(), revision);
    let revision = within(app.end_trial()).await.unwrap();
    assert_eq!(view(&mut events, revision).await.board, original.board);
    let revision = within(app.generate_tsumego(TsumegoOptions::default()))
        .await
        .unwrap();
    let puzzle = view(&mut events, revision).await;
    assert!(!puzzle.trial);
    let status = within(app.tsumego_status()).await.unwrap();
    assert!(matches!(
        within(app.start_trial()).await,
        Err(AppError::Trial(TrialError::TsumegoConflict))
    ));
    assert_eq!(within(app.tsumego_status()).await.unwrap(), status);
    assert_eq!(within(app.end_trial()).await.unwrap(), revision);
    let revision = within(app.exit_tsumego()).await.unwrap();
    let restored = view(&mut events, revision).await;
    assert_eq!(restored.board, original.board);
    assert_eq!(restored.tree.current, original.tree.current);
    assert_eq!(restored.can_undo, original.can_undo);
    assert_eq!(trial.board, original.board);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn trial_keeps_ko_history_and_errors_do_not_replace_or_damage_saved_document() {
    let (dir, app, mut events, _) =
        open(b"(;SZ[5]AB[ab][ba][cb]AW[bb][ac][cc][bd]PL[B];B[bc])").await;
    let revision = within(app.step(1)).await.unwrap();
    let original = view(&mut events, revision).await;
    let recapture = Move::Play(Color::White, Point { x: 1, y: 1 });
    assert!(within(app.play(recapture)).await.is_err());
    let revision = within(app.start_trial()).await.unwrap();
    view(&mut events, revision).await;
    assert!(within(app.play(recapture)).await.is_err());
    assert!(within(app.play_colored(recapture)).await.is_err());
    assert!(within(app.open_bytes(b"(;SZ[9])".to_vec())).await.is_err());
    assert!(
        within(app.open_path(dir.path().join("other.sgf")))
            .await
            .is_err()
    );
    assert!(
        within(app.open_dropped_paths(vec![dir.path().join("other.sgf")]))
            .await
            .is_err()
    );
    assert!(
        within(app.new_game(NewGameOptions::default()))
            .await
            .is_err()
    );
    assert!(within(app.convert_to_start_position()).await.is_err());
    let sync_before = within(app.board_sync_status()).await.unwrap();
    assert!(matches!(
        within(app.start_board_sync(
            dir.path().join("nonexistent-readboard"),
            app_core::BoardSyncOptions::default(),
        ))
        .await,
        Err(AppError::Trial(TrialError::DocumentReplacement))
    ));
    assert_eq!(within(app.board_sync_status()).await.unwrap(), sync_before);
    assert_eq!(within(app.step(0)).await.unwrap(), revision);
    let revision = within(app.end_trial()).await.unwrap();
    assert_eq!(view(&mut events, revision).await.board, original.board);
    let revision = within(app.open_bytes(b"(;SZ[5];B[aa];W[aa])".to_vec()))
        .await
        .unwrap();
    view(&mut events, revision).await;
    let revision = within(app.step(2)).await.unwrap();
    let invalid = view(&mut events, revision).await;
    assert!(invalid.board.is_none());
    assert!(within(app.start_trial()).await.is_err());
    assert_eq!(within(app.end_trial()).await.unwrap(), revision);
    within(app.clone().shutdown()).await.unwrap();
    assert!(within(app.start_trial()).await.is_err());
    assert!(within(app.end_trial()).await.is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn trial_export_rejects_symlink_and_hard_link_aliases_of_original() {
    let (dir, app, mut events, _) = open(SOURCE).await;
    let original = dir.path().join("source.sgf");
    within(app.save(&original)).await.unwrap();
    let bytes = std::fs::read(&original).unwrap();
    let symbolic = dir.path().join("symbolic.sgf");
    let hard = dir.path().join("hard.sgf");
    std::os::unix::fs::symlink(&original, &symbolic).unwrap();
    std::fs::hard_link(&original, &hard).unwrap();
    let revision = within(app.start_trial()).await.unwrap();
    view(&mut events, revision).await;
    for path in [&symbolic, &hard] {
        assert!(within(app.save(path)).await.is_err());
    }
    assert_eq!(std::fs::read(&original).unwrap(), bytes);
    let revision = within(app.end_trial()).await.unwrap();
    view(&mut events, revision).await;
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn old_document_wire_defaults_trial_to_false() {
    let (_dir, app, _events, initial) = open(b"(;SZ[9])").await;
    let mut wire = serde_json::to_value(initial).unwrap();
    wire.as_object_mut().unwrap().remove("trial");
    assert!(!serde_json::from_value::<DocumentView>(wire).unwrap().trial);
    within(app.shutdown()).await.unwrap();
}

async fn final_analysis(
    events: &mut AppSubscription,
    revision: u64,
    node: u32,
) -> app_core::AnalysisView {
    within(async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::AnalysisUpdated(result) => {
                    assert_eq!((result.revision, result.node_id), (revision, node));
                    // The same position may now be restored from SQLite before
                    // its new document-scoped query completes. Verify the live
                    // query too; every intervening cache still needs fresh IDs.
                    if !result.is_during_search && result.source == app_core::AnalysisSource::Live {
                        return result;
                    }
                }
                AppEvent::EngineStatusChanged(app_core::EngineStatus::Failed {
                    reason, ..
                }) => panic!("{reason}"),
                _ => {}
            }
        }
    })
    .await
}

#[tokio::test]
async fn trial_analysis_and_restoration_use_distinct_namespaces_for_identical_node_ids() {
    let dir = tempfile::tempdir().unwrap();
    let model = dir.path().join("model.bin");
    let engine_config = dir.path().join("analysis.cfg");
    let log = dir.path().join("requests.jsonl");
    std::fs::write(&model, "fake").unwrap();
    std::fs::write(&engine_config, "fake").unwrap();
    let profile = app_core::EngineProfile {
        human_model: None,
        program: fake_katago::binary_path().unwrap(),
        model,
        config: engine_config,
        extra_args: vec![
            "--visits-per-sec".into(),
            "100000".into(),
            "--request-log".into(),
            log.to_str().unwrap().into(),
        ],
        canonical_model_name: None,
    };
    let mut config = app_core::AppConfig::default();
    config.analysis.live.max_visits = 10;
    config.analysis.live.report_interval = Duration::from_millis(10);
    let (app, mut events) = within(app_core::spawn_with_config(dir.path(), config))
        .await
        .unwrap();
    within(app.set_auto_quick_analyze_on_load(false))
        .await
        .unwrap();
    within(app.open_bytes(BRANCHED_ANALYSIS.to_vec()))
        .await
        .unwrap();
    let revision = within(app.step(2)).await.unwrap();
    let original = view(&mut events, revision).await;
    within(app.configure_engine(Some(profile))).await.unwrap();
    final_analysis(&mut events, revision, original.tree.current).await;
    let revision = within(app.start_trial()).await.unwrap();
    let trial = view(&mut events, revision).await;
    assert_eq!(trial.tree.current, original.tree.current);
    assert!(
        trial
            .graph
            .points
            .iter()
            .all(|point| point.summary.is_none())
    );
    final_analysis(&mut events, revision, trial.tree.current).await;
    let revision = within(app.play(Move::Play(Color::Black, Point { x: 7, y: 7 })))
        .await
        .unwrap();
    let played = view(&mut events, revision).await;
    final_analysis(&mut events, revision, played.tree.current).await;
    let revision = within(app.end_trial()).await.unwrap();
    let restored = view(&mut events, revision).await;
    assert_eq!(restored.board, original.board);
    assert!(
        restored
            .graph
            .points
            .iter()
            .all(|point| point.summary.is_none())
    );
    final_analysis(&mut events, revision, restored.tree.current).await;
    within(app.shutdown()).await.unwrap();
    let queries: Vec<katago_protocol::AnalysisQuery> = std::fs::read_to_string(log)
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|value| value.get("action").is_none())
        .map(|value| serde_json::from_value(value).unwrap())
        .collect();
    assert_eq!(queries.len(), 4);
    assert_eq!(queries[0].moves, queries[1].moves);
    assert_eq!(queries[0].moves, queries[3].moves);
    assert_eq!(queries[2].moves.len(), queries[0].moves.len() + 1);
    assert_eq!(queries[0].initial_stones, queries[3].initial_stones);
    let ids: std::collections::HashSet<_> = queries.iter().map(|query| &query.id).collect();
    assert_eq!(ids.len(), 4);
}

const BRANCHED_ANALYSIS: &[u8] = b"(;SZ[9];B[aa];W[bb](;B[cc])(;B[dd]))";
