#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{
    AppConfig, AppCore, AppError, AppEvent, AppSubscription, BatchAnalysisStatus, BatchClock,
    BatchItemState, BatchMove, BatchOptions, BatchOutput, BatchPhase, DocumentView, EngineProfile,
    NewGameOptions, Settings, WholeGameOptions, WholeGameScope, WholeGameStatus,
};
use std::{future::Future, path::Path, path::PathBuf, sync::Arc, time::Duration};

const TIMESTAMP: &str = "20261113091425";
const SMALL: &[u8] = b"(;SZ[9]PB[first];B[aa];W[])";
const GIB: &[u8] = include_bytes!("../../../../../fixtures/gib/utf8-even.gib");
const ENCODED: &[u8] = include_bytes!("../../../../../fixtures/sgf/gb18030-no-ca.sgf");
const JAVA: &[u8] = include_bytes!("../../../../../fixtures/lz/java-analysis.sgf");

#[derive(Debug)]
struct FixedClock;

impl BatchClock for FixedClock {
    fn local_timestamp(&self) -> String {
        TIMESTAMP.into()
    }
}

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("operation exceeded safety deadline")
}

fn options(files: Vec<PathBuf>) -> BatchOptions {
    BatchOptions {
        files,
        visits: 1,
        scope: WholeGameScope::Line,
        force: true,
        output: BatchOutput {
            suffix: "已分析".into(),
            directory: None,
        },
    }
}

async fn source(directory: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = directory.join(name);
    tokio::fs::write(&path, bytes).await.unwrap();
    path
}

async fn core(directory: &Path, args: Vec<String>, wait_ready: bool) -> (AppCore, AppSubscription) {
    core_with_clock(directory, args, wait_ready, Arc::new(FixedClock)).await
}

async fn core_with_clock(
    directory: &Path,
    args: Vec<String>,
    wait_ready: bool,
    clock: Arc<dyn BatchClock>,
) -> (AppCore, AppSubscription) {
    // Building the fake process and preparing the filesystem are not subject
    // to a timing assertion. All waits below observe acknowledged state/events.
    let program = fake_katago::binary_path().unwrap();
    let settings = Settings {
        engine: Some(EngineProfile {
            human_model: None,
            model: program.clone(),
            config: program.clone(),
            program,
            extra_args: args,
            canonical_model_name: None,
        }),
        pondering: false,
        cache_enabled: false,
        auto_quick_analyze_on_load: false,
        recent_files: vec!["previous.sgf".into()],
        ..Default::default()
    };
    settings.save(directory).await.unwrap();
    let mut config = AppConfig {
        batch_clock: clock,
        ..Default::default()
    };
    config.session.max_restarts = 0;
    config.session.startup_timeout = Duration::from_secs(60);
    config.session.response_timeout = Duration::from_secs(60);
    config.session.write_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(60);
    let (core, mut output) = crate::spawn_with_config(directory, config).await.unwrap();
    if wait_ready {
        within(async {
            loop {
                match output.recv().await.expect("stream closed before ready") {
                    AppEvent::EngineStatusChanged(crate::EngineStatus::Ready { .. }) => break,
                    AppEvent::EngineStatusChanged(crate::EngineStatus::Failed {
                        reason, ..
                    }) => {
                        panic!("fake engine failed: {reason}");
                    }
                    _ => {}
                }
            }
        })
        .await;
    }
    (core, output)
}

async fn completed(output: &mut AppSubscription) -> (BatchAnalysisStatus, Option<DocumentView>) {
    wait_batch(output, |status| status.phase == BatchPhase::Completed).await
}

async fn wait_running(output: &mut AppSubscription) -> (BatchAnalysisStatus, Option<DocumentView>) {
    wait_batch(output, |status| {
        status.phase == BatchPhase::Running
            && status
                .items
                .iter()
                .any(|item| matches!(item.state, BatchItemState::Running { progress: Some(_) }))
    })
    .await
}

async fn wait_batch(
    output: &mut AppSubscription,
    predicate: impl Fn(&BatchAnalysisStatus) -> bool,
) -> (BatchAnalysisStatus, Option<DocumentView>) {
    within(async {
        let mut last_document = None;
        loop {
            match output.recv().await.expect("stream closed during batch") {
                AppEvent::DocumentChanged { view, .. } => last_document = Some(view),
                AppEvent::BatchAnalysisProgress(status) if predicate(&status) => {
                    return (status, last_document);
                }
                _ => {}
            }
        }
    })
    .await
}

fn output_path(item: &crate::BatchItem) -> &Path {
    match &item.state {
        BatchItemState::Completed { output } => output,
        state => panic!("item did not complete: {state:?}"),
    }
}

fn assert_waiting(status: &BatchAnalysisStatus, count: usize) {
    assert_eq!(status.items.len(), count);
    assert!(
        status
            .items
            .iter()
            .all(|item| item.state == BatchItemState::Waiting)
    );
    assert_eq!(status.current_index, None);
}

async fn shutdown(core: AppCore) {
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn three_games_save_in_order_with_lz_original_bytes_and_recent_files_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let files = vec![
        source(temp.path(), "first.sgf", ENCODED).await,
        source(temp.path(), "second.GIB", GIB).await,
        source(temp.path(), "third.sgf", JAVA).await,
    ];
    let (core, mut output) = core(temp.path(), vec![], true).await;
    let before_recent = core.recent_files().await.unwrap();
    core.start_batch_analysis(options(files.clone()))
        .await
        .unwrap();
    let (status, document) = completed(&mut output).await;
    assert_eq!(status.items.len(), 3);
    for (index, (item, original)) in status.items.iter().zip([ENCODED, GIB, JAVA]).enumerate() {
        assert_eq!(item.path, files[index]);
        let stem = files[index].file_stem().unwrap().to_str().unwrap();
        assert_eq!(
            output_path(item),
            temp.path().join(format!("{stem}_已分析_{TIMESTAMP}.sgf"))
        );
        assert_eq!(tokio::fs::read(&files[index]).await.unwrap(), original);
        let bytes = tokio::fs::read(output_path(item)).await.unwrap();
        let collection = sgf::parse(&bytes).unwrap();
        let saved = document::GameDocument::from_sgf(&collection.games[0]).unwrap();
        for node in saved.variation_tree().unwrap() {
            let analysis = saved.saved_analysis(node.id).unwrap().unwrap();
            assert_eq!(analysis.visits, 1, "force must replace saved deeper LZ");
            assert!(!analysis.candidates.is_empty());
        }
    }
    assert_eq!(core.recent_files().await.unwrap(), before_recent);
    assert_eq!(core.batch_analysis_status().await.unwrap(), status);
    assert_eq!(document.unwrap().board.unwrap().width, 5);
    assert_eq!(core.variation_tree().await.unwrap().nodes.len(), 3);
    shutdown(core).await;
}

#[tokio::test]
async fn output_collision_keeps_existing_files_and_uses_incrementing_suffixes() {
    let temp = tempfile::tempdir().unwrap();
    let file = source(temp.path(), "game.sgf", SMALL).await;
    let output_directory = temp.path().join("outputs");
    tokio::fs::create_dir(&output_directory).await.unwrap();
    let existing = source(
        &output_directory,
        &format!("game_已分析_{TIMESTAMP}.sgf"),
        b"must not overwrite",
    )
    .await;
    let (core, mut output) = core(temp.path(), vec![], true).await;
    let mut requested = options(vec![file.clone(), file.clone()]);
    requested.output.directory = Some(output_directory.clone());
    core.start_batch_analysis(requested).await.unwrap();
    let (status, _) = completed(&mut output).await;
    assert_eq!(
        tokio::fs::read(existing).await.unwrap(),
        b"must not overwrite"
    );
    assert_eq!(tokio::fs::read(file).await.unwrap(), SMALL);
    for (index, item) in status.items.iter().enumerate() {
        assert_eq!(
            output_path(item),
            output_directory.join(format!("game_已分析_{TIMESTAMP}-{}.sgf", index + 2))
        );
        assert!(sgf::parse(&tokio::fs::read(output_path(item)).await.unwrap()).is_ok());
    }
    shutdown(core).await;
}

#[tokio::test]
async fn unreadable_middle_game_fails_and_later_game_still_completes() {
    let temp = tempfile::tempdir().unwrap();
    let files = vec![
        source(temp.path(), "first.sgf", SMALL).await,
        temp.path().join("missing.sgf"),
        source(temp.path(), "last.sgf", SMALL).await,
    ];
    let (core, mut output) = core(temp.path(), vec![], true).await;
    core.start_batch_analysis(options(files)).await.unwrap();
    let (status, _) = completed(&mut output).await;
    assert!(output_path(&status.items[0]).is_file());
    assert!(
        matches!(&status.items[1].state, BatchItemState::Failed { reason } if !reason.is_empty())
    );
    assert!(output_path(&status.items[2]).is_file());
    assert_eq!(core.recent_files().await.unwrap(), ["previous.sgf"]);
    shutdown(core).await;
}

#[tokio::test]
async fn middle_output_name_failure_is_reported_without_skipping_the_next_game() {
    let temp = tempfile::tempdir().unwrap();
    // Source components fit the platform's 255-character filename limit, while
    // the generated second output exceeds it. No permission assumptions or
    // races against an asynchronous save are needed to inject the failure.
    let long_stem = "x".repeat(190);
    let files = vec![
        source(temp.path(), "first.sgf", SMALL).await,
        source(temp.path(), &format!("{long_stem}.sgf"), SMALL).await,
        source(temp.path(), "last.sgf", SMALL).await,
    ];
    let (core, mut output) = core(temp.path(), vec![], true).await;
    let mut requested = options(files.clone());
    requested.output.suffix = "a".repeat(64);
    core.start_batch_analysis(requested).await.unwrap();
    let (status, _) = completed(&mut output).await;
    assert!(output_path(&status.items[0]).is_file());
    assert!(
        matches!(&status.items[1].state, BatchItemState::Failed { reason } if !reason.is_empty())
    );
    assert!(output_path(&status.items[2]).is_file());
    for path in files {
        assert_eq!(tokio::fs::read(path).await.unwrap(), SMALL);
    }
    shutdown(core).await;
}

#[tokio::test]
async fn range_invalid_for_one_game_fails_only_that_item() {
    let temp = tempfile::tempdir().unwrap();
    let files = vec![
        source(temp.path(), "short.sgf", b"(;SZ[9];B[])").await,
        source(temp.path(), "long.sgf", SMALL).await,
    ];
    let (core, mut output) = core(temp.path(), vec![], true).await;
    let mut requested = options(files);
    requested.scope = WholeGameScope::Range {
        first_move: Some(2),
        last_move: Some(2),
    };
    core.start_batch_analysis(requested).await.unwrap();
    let (status, _) = completed(&mut output).await;
    assert!(
        matches!(&status.items[0].state, BatchItemState::Failed { reason } if !reason.is_empty())
    );
    let bytes = tokio::fs::read(output_path(&status.items[1]))
        .await
        .unwrap();
    let collection = sgf::parse(&bytes).unwrap();
    let saved = document::GameDocument::from_sgf(&collection.games[0]).unwrap();
    let analyzed = saved
        .variation_tree()
        .unwrap()
        .into_iter()
        .filter(|node| saved.saved_analysis(node.id).unwrap().is_some())
        .map(|node| node.id)
        .collect::<Vec<_>>();
    // The second move needs both its parent (after move 1) and its own result.
    assert_eq!(analyzed, [document::NodeId(1), document::NodeId(2)]);
    for node in analyzed {
        assert_eq!(saved.saved_analysis(node).unwrap().unwrap().visits, 1);
    }
    let progress = core.whole_game_progress().await.unwrap().unwrap();
    assert_eq!(progress.total, 2);
    assert_eq!(progress.completed, 2);
    shutdown(core).await;
}

#[tokio::test]
async fn range_starting_at_first_move_saves_root_baseline_and_counts_it_in_progress() {
    let temp = tempfile::tempdir().unwrap();
    let file = source(temp.path(), "first-move.sgf", SMALL).await;
    let (core, mut output) = core(temp.path(), vec![], true).await;
    let mut requested = options(vec![file]);
    requested.scope = WholeGameScope::Range {
        first_move: Some(1),
        last_move: Some(1),
    };
    core.start_batch_analysis(requested).await.unwrap();
    let (status, _) = completed(&mut output).await;
    let bytes = tokio::fs::read(output_path(&status.items[0]))
        .await
        .unwrap();
    let collection = sgf::parse(&bytes).unwrap();
    let saved = document::GameDocument::from_sgf(&collection.games[0]).unwrap();
    let analyzed = saved
        .variation_tree()
        .unwrap()
        .into_iter()
        .filter(|node| saved.saved_analysis(node.id).unwrap().is_some())
        .map(|node| node.id)
        .collect::<Vec<_>>();
    assert_eq!(analyzed, [saved.root(), document::NodeId(1)]);
    let progress = core.whole_game_progress().await.unwrap().unwrap();
    assert_eq!(progress.total, 2);
    assert_eq!(progress.completed, 2);
    shutdown(core).await;
}

async fn gated_core(directory: &Path) -> (AppCore, AppSubscription, PathBuf) {
    let gate = directory.join("release-startup");
    let (core, output) = core(
        directory,
        vec![
            "--startup-gate-file".into(),
            gate.to_string_lossy().into_owned(),
        ],
        false,
    )
    .await;
    (core, output, gate)
}

#[tokio::test]
async fn pause_resume_stop_restart_preserve_waiting_items_until_gate_released() {
    let temp = tempfile::tempdir().unwrap();
    let file = source(temp.path(), "game.sgf", SMALL).await;
    let (core, mut output, gate) = gated_core(temp.path()).await;
    core.start_batch_analysis(options(vec![file.clone(), file]))
        .await
        .unwrap();
    let (running, _) = wait_running(&mut output).await;
    assert_eq!(running.phase, BatchPhase::Running);
    assert_eq!(running.current_index, Some(0));
    let paused = core.pause_batch_analysis().await.unwrap();
    assert_eq!(paused.phase, BatchPhase::Paused);
    assert!(
        matches!(&paused.items[0].state, BatchItemState::Running { progress: Some(progress) }
        if progress.status == WholeGameStatus::Paused)
    );
    assert_eq!(core.batch_analysis_status().await.unwrap(), paused);
    let resumed = core.resume_batch_analysis().await.unwrap();
    assert_eq!(resumed.phase, BatchPhase::Running);
    let stopped = core.stop_batch_analysis().await.unwrap();
    assert_eq!(stopped.phase, BatchPhase::Stopped);
    assert_waiting(&stopped, 2);
    core.start_batch_analysis(options(vec![])).await.unwrap();
    assert_eq!(core.batch_analysis_status().await.unwrap().items.len(), 2);
    tokio::fs::write(gate, b"ready").await.unwrap();
    let (status, _) = completed(&mut output).await;
    assert_eq!(status.items.len(), 2);
    assert!(
        status
            .items
            .iter()
            .all(|item| matches!(item.state, BatchItemState::Completed { .. }))
    );
    shutdown(core).await;
}

#[tokio::test]
async fn running_queue_add_remove_move_first_and_clear_preserve_current_item() {
    let temp = tempfile::tempdir().unwrap();
    let mut files = Vec::new();
    for name in ["a.sgf", "b.sgf", "c.sgf", "d.sgf", "e.sgf"] {
        files.push(source(temp.path(), name, SMALL).await);
    }
    let (core, mut output, gate) = gated_core(temp.path()).await;
    core.start_batch_analysis(options(files[..3].to_vec()))
        .await
        .unwrap();
    let (initial, _) = wait_running(&mut output).await;
    let a = initial.items[0].id;
    let b = initial.items[1].id;
    let c = initial.items[2].id;
    let added = core
        .add_batch_analysis_files(files[3..].to_vec())
        .await
        .unwrap();
    let d = added.items[3].id;
    let e = added.items[4].id;
    let ids =
        |status: &BatchAnalysisStatus| status.items.iter().map(|item| item.id).collect::<Vec<_>>();
    assert_eq!(
        ids(&core
            .move_batch_analysis_item(d, BatchMove::Up)
            .await
            .unwrap()),
        [a, b, d, c, e]
    );
    assert_eq!(
        ids(&core
            .move_batch_analysis_item(d, BatchMove::Down)
            .await
            .unwrap()),
        [a, b, c, d, e]
    );
    assert_eq!(
        ids(&core
            .move_batch_analysis_item(e, BatchMove::First)
            .await
            .unwrap()),
        [a, e, b, c, d]
    );
    assert_eq!(
        ids(&core.remove_batch_analysis_item(b).await.unwrap()),
        [a, e, c, d]
    );
    assert!(core.remove_batch_analysis_item(a).await.is_err());
    assert!(
        core.move_batch_analysis_item(a, BatchMove::Down)
            .await
            .is_err()
    );
    let cleared = core.clear_batch_analysis().await.unwrap();
    assert_eq!(ids(&cleared), [a]);
    assert_eq!(cleared.current_index, Some(0));
    assert_eq!(cleared.phase, BatchPhase::Running);
    core.pause_batch_analysis().await.unwrap();
    let paused_clear = core.clear_batch_analysis().await.unwrap();
    assert_eq!(ids(&paused_clear), [a]);
    assert_eq!(paused_clear.phase, BatchPhase::Paused);
    core.resume_batch_analysis().await.unwrap();
    tokio::fs::write(gate, b"ready").await.unwrap();
    let (finished, _) = completed(&mut output).await;
    assert_eq!(ids(&finished), [a]);
    let empty = core.clear_batch_analysis().await.unwrap();
    assert!(empty.items.is_empty());
    assert_eq!(empty.current_index, None);
    shutdown(core).await;
}

fn assert_guard<T: std::fmt::Debug>(result: Result<T, AppError>) {
    assert!(
        matches!(result, Err(AppError::BatchAnalysisRunning)),
        "{result:?}"
    );
}

#[tokio::test]
async fn running_and_paused_batches_reject_document_replacement_editing_and_review_entrypoints() {
    let temp = tempfile::tempdir().unwrap();
    let file = source(temp.path(), "game.sgf", SMALL).await;
    let (core, mut output, gate) = gated_core(temp.path()).await;
    core.start_batch_analysis(options(vec![file.clone()]))
        .await
        .unwrap();
    wait_running(&mut output).await;
    for pause in [false, true] {
        if pause {
            core.pause_batch_analysis().await.unwrap();
        }
        assert_guard(core.open_path(temp.path().join("missing.sgf")).await);
        assert_guard(core.open_bytes(SMALL.to_vec()).await);
        assert_guard(core.open_dropped_paths(vec![file.clone()]).await);
        assert_guard(core.paste_sgf().await);
        assert_guard(core.new_game(NewGameOptions::default()).await);
        assert_guard(core.fox_open_game("invalid-game-id".into()).await);
        assert_guard(core.set_comment("must not change".into()).await);
        assert_guard(core.play(go_core::Move::Pass(go_core::Color::Black)).await);
        assert_guard(core.undo().await);
        assert_guard(core.redo().await);
        assert_guard(core.clear_analysis(true).await);
        assert_guard(core.save(&file).await);
        assert_guard(core.convert_to_start_position().await);
        assert_guard(core.start_trial().await);
        assert_guard(core.end_trial().await);
        assert_guard(
            core.generate_tsumego(crate::TsumegoOptions::default())
                .await,
        );
        assert_guard(core.exit_tsumego().await);
        assert_guard(
            core.start_board_sync(
                temp.path().join("unused-helper"),
                crate::BoardSyncOptions::default(),
            )
            .await,
        );
        assert_guard(core.pause_whole_game_analysis().await);
        assert_guard(core.resume_whole_game_analysis().await);
        assert_guard(core.cancel_whole_game_analysis().await);
        assert_guard(
            core.start_whole_game_analysis(WholeGameOptions::default())
                .await,
        );
        assert_guard(
            core.start_whole_game_analysis(WholeGameOptions {
                visits: 0,
                ..Default::default()
            })
            .await,
        );
        assert_guard(core.set_auto_quick_analyze_on_load(true).await);
        assert_guard(core.start_batch_analysis(options(vec![])).await);
        // Navigation and read-only display queries remain usable in both phases.
        core.step(1).await.unwrap();
        core.go_to(0).await.unwrap();
        assert_eq!(core.variation_tree().await.unwrap().nodes.len(), 3);
        assert_eq!(core.recent_files().await.unwrap(), ["previous.sgf"]);
    }
    core.stop_batch_analysis().await.unwrap();
    core.open_bytes(SMALL.to_vec()).await.unwrap();
    // Let the fake process handle the queued shutdown request without waiting
    // for a startup timeout; this release is not used as test synchronization.
    tokio::fs::write(gate, b"ready").await.unwrap();
    shutdown(core).await;
}

#[tokio::test]
async fn engine_failure_stops_entire_queue_and_reports_a_reason() {
    let temp = tempfile::tempdir().unwrap();
    let file = source(temp.path(), "game.sgf", SMALL).await;
    let (core, mut output) = core(
        temp.path(),
        vec!["--crash-after-queries".into(), "1".into()],
        true,
    )
    .await;
    core.start_batch_analysis(options(vec![file.clone(), file]))
        .await
        .unwrap();
    let (status, _) = wait_batch(&mut output, |status| status.phase == BatchPhase::Stopped).await;
    assert!(
        status
            .reason
            .as_ref()
            .is_some_and(|reason| !reason.is_empty())
    );
    assert_eq!(status.items.len(), 2);
    assert_eq!(status.items[1].state, BatchItemState::Waiting);
    assert!(
        !status
            .items
            .iter()
            .any(|item| matches!(item.state, BatchItemState::Completed { .. }))
    );
    shutdown(core).await;
}

#[tokio::test]
async fn queue_is_not_persisted_across_application_restart() {
    let temp = tempfile::tempdir().unwrap();
    let file = source(temp.path(), "game.sgf", SMALL).await;
    let (core, _, gate) = gated_core(temp.path()).await;
    core.start_batch_analysis(options(vec![file.clone(), file]))
        .await
        .unwrap();
    core.stop_batch_analysis().await.unwrap();
    tokio::fs::write(gate, b"ready").await.unwrap();
    shutdown(core).await;
    let (restarted, _) = crate::spawn(temp.path()).await.unwrap();
    let status = restarted.batch_analysis_status().await.unwrap();
    assert_eq!(status.phase, BatchPhase::Idle);
    assert!(status.items.is_empty());
    shutdown(restarted).await;
}

#[test]
fn output_suffix_validation_rejects_empty_control_path_and_windows_reserved_characters() {
    for suffix in [
        "",
        " ",
        "../escape",
        "a/b",
        "a\\b",
        "a:b",
        "a?b",
        "a*b",
        "a|b",
        "a<b",
        "a>b",
        "a\"b",
        "a\nb",
        "a\0b",
    ] {
        let mut requested = options(vec![]);
        requested.output.suffix = suffix.into();
        assert!(
            matches!(
                requested.validate(),
                Err(AppError::Batch(crate::BatchError::InvalidSuffix))
            ),
            "{suffix:?}"
        );
    }
    let mut requested = options(vec![]);
    requested.output.suffix = "a".repeat(65);
    assert!(requested.validate().is_err());
    for suffix in ["已分析", "analysed", "a".repeat(64).as_str()] {
        requested.output.suffix = suffix.into();
        requested.validate().unwrap();
    }
}

#[tokio::test]
async fn query_error_marks_only_the_current_item_failed_and_next_review_runs() {
    let temp = tempfile::tempdir().unwrap();
    let file = source(temp.path(), "game.sgf", SMALL).await;
    let program = fake_katago::binary_path().unwrap();
    let (mut actor, _) = crate::actor::tests::actor();
    actor.directory = temp.path().to_owned();
    actor.config.batch_clock = Arc::new(FixedClock);
    actor.settings.pondering = false;
    actor.settings.auto_quick_analyze_on_load = false;
    actor.settings.engine = Some(EngineProfile {
        human_model: None,
        model: program.clone(),
        config: program.clone(),
        program,
        canonical_model_name: None,
        extra_args: vec![],
    });
    actor.start_engine().await;
    actor
        .batch_command(crate::batch::Request::Start(options(vec![
            file.clone(),
            file,
        ])))
        .await
        .unwrap();
    actor.advance_batch().await;
    // Inject exactly the ordinary ReviewRun query-failure signal before polling
    // its real subscription; subsequent work still uses the same fake engine.
    actor
        .review_signal(Some(Err(engine_session::QueryError::Engine(
            "one rejected request".into(),
        )
        .into())))
        .await;
    actor.sync_batch_progress().await;
    assert!(
        matches!(&actor.batch.status.items[0].state, BatchItemState::Failed { reason } if reason.contains("one rejected request"))
    );
    assert_eq!(actor.batch.status.phase, BatchPhase::Running);
    actor.advance_batch().await;
    within(async {
        while actor.review_running() {
            let result = crate::actor::review::receive(&mut actor.review).await;
            actor.review_signal(result).await;
        }
    })
    .await;
    actor.sync_batch_progress().await;
    actor.advance_batch().await;
    assert_eq!(actor.batch.status.phase, BatchPhase::Completed);
    assert!(output_path(&actor.batch.status.items[1]).is_file());
    within(actor.stop_engine()).await.unwrap();
}

#[tokio::test]
async fn shutdown_stops_an_active_batch_and_preserves_the_source() {
    let temp = tempfile::tempdir().unwrap();
    let file = source(temp.path(), "game.sgf", SMALL).await;
    let (core, mut output, gate) = gated_core(temp.path()).await;
    core.start_batch_analysis(options(vec![file.clone(), file.clone()]))
        .await
        .unwrap();
    wait_running(&mut output).await;
    core.pause_batch_analysis().await.unwrap();
    tokio::fs::write(gate, b"ready").await.unwrap();
    shutdown(core).await;
    let (status, _) = wait_batch(&mut output, |status| status.phase == BatchPhase::Stopped).await;
    assert_waiting(&status, 2);
    assert_eq!(tokio::fs::read(file).await.unwrap(), SMALL);
}

#[derive(Debug)]
struct InvalidClock;

impl BatchClock for InvalidClock {
    fn local_timestamp(&self) -> String {
        "2026-11-13T09:14:25".into()
    }
}

#[tokio::test]
async fn invalid_injected_timestamp_fails_the_item_without_creating_an_unsafe_filename() {
    let temp = tempfile::tempdir().unwrap();
    let file = source(temp.path(), "game.sgf", SMALL).await;
    let (core, mut output) =
        core_with_clock(temp.path(), vec![], true, Arc::new(InvalidClock)).await;
    core.start_batch_analysis(options(vec![file.clone()]))
        .await
        .unwrap();
    let (status, _) = completed(&mut output).await;
    assert!(
        matches!(&status.items[0].state, BatchItemState::Failed { reason } if reason.contains("timestamp"))
    );
    assert_eq!(tokio::fs::read(file).await.unwrap(), SMALL);
    shutdown(core).await;
}

#[tokio::test]
async fn unavailable_engine_stops_before_opening_or_consuming_waiting_files() {
    let temp = tempfile::tempdir().unwrap();
    let file = source(temp.path(), "game.sgf", SMALL).await;
    let (core, mut output) = crate::spawn(temp.path()).await.unwrap();
    core.start_batch_analysis(options(vec![file.clone()]))
        .await
        .unwrap();
    let (status, _) = wait_batch(&mut output, |status| status.phase == BatchPhase::Stopped).await;
    assert_waiting(&status, 1);
    assert!(
        status
            .reason
            .as_ref()
            .is_some_and(|reason| !reason.is_empty())
    );
    assert_eq!(core.variation_tree().await.unwrap().nodes.len(), 1);
    assert_eq!(tokio::fs::read(file).await.unwrap(), SMALL);
    shutdown(core).await;
}

#[tokio::test]
async fn rejected_start_leaves_existing_queue_and_item_ids_unchanged() {
    let (mut actor, _) = crate::actor::tests::actor();
    let first = PathBuf::from("first.sgf");
    let before = actor
        .batch_command(crate::batch::Request::Add(vec![first]))
        .await
        .unwrap();
    for invalid in [
        BatchOptions {
            visits: 0,
            ..options(vec![PathBuf::from("invalid-visits.sgf")])
        },
        BatchOptions {
            output: BatchOutput {
                suffix: "../escape".into(),
                directory: None,
            },
            ..options(vec![PathBuf::from("invalid-suffix.sgf")])
        },
    ] {
        assert!(
            actor
                .batch_command(crate::batch::Request::Start(invalid))
                .await
                .is_err()
        );
        assert_eq!(actor.batch.status, before);
    }
    actor.start_move_trial().await.unwrap();
    assert!(
        actor
            .batch_command(crate::batch::Request::Start(options(vec![PathBuf::from(
                "trial.sgf"
            )])))
            .await
            .is_err()
    );
    assert_eq!(actor.batch.status, before);
    actor.end_move_trial().await.unwrap();
    let after = actor
        .batch_command(crate::batch::Request::Add(vec![PathBuf::from(
            "second.sgf",
        )]))
        .await
        .unwrap();
    assert_eq!(after.items.len(), 2);
    assert_eq!(after.items[1].id, before.items[0].id + 1);
    actor.batch.next_id = u32::MAX;
    assert!(matches!(
        actor
            .batch_command(crate::batch::Request::Start(options(vec![PathBuf::from(
                "exhausted.sgf"
            )])))
            .await,
        Err(AppError::Batch(crate::BatchError::IdLimit))
    ));
    assert_eq!(actor.batch.status, after);
}

#[tokio::test]
async fn reordering_and_clearing_completed_prefix_keeps_current_identity_and_progress() {
    let (mut actor, _) = crate::actor::tests::actor();
    let progress = crate::WholeGameProgress {
        source: crate::WholeGameSource::Manual,
        completed: 1,
        total: 3,
        visits: 1,
        status: WholeGameStatus::Running,
    };
    actor.review.progress = Some(progress.clone());
    actor.batch.status = BatchAnalysisStatus {
        phase: BatchPhase::Running,
        items: [
            (
                0,
                BatchItemState::Completed {
                    output: PathBuf::from("done.sgf"),
                },
            ),
            (
                1,
                BatchItemState::Running {
                    progress: Some(progress.clone()),
                },
            ),
            (2, BatchItemState::Waiting),
            (
                3,
                BatchItemState::Failed {
                    reason: "previous failure".into(),
                },
            ),
            (4, BatchItemState::Waiting),
        ]
        .into_iter()
        .map(|(id, state)| crate::BatchItem {
            id,
            path: PathBuf::from(format!("{id}.sgf")),
            state,
        })
        .collect(),
        current_index: Some(1),
        reason: None,
    };
    for request in [
        crate::batch::Request::Move(4, BatchMove::First),
        crate::batch::Request::Move(4, BatchMove::Down),
        crate::batch::Request::Remove(2),
    ] {
        let status = actor.batch_command(request).await.unwrap();
        let current = &status.items[status.current_index.unwrap()];
        assert_eq!(current.id, 1);
        assert_eq!(
            current.state,
            BatchItemState::Running {
                progress: Some(progress.clone())
            }
        );
    }
    let cleared = actor
        .batch_command(crate::batch::Request::Clear)
        .await
        .unwrap();
    assert_eq!(cleared.current_index, Some(0));
    assert_eq!(cleared.items.len(), 1);
    assert_eq!(cleared.items[0].id, 1);
    let stopped = actor
        .batch_command(crate::batch::Request::Stop)
        .await
        .unwrap();
    assert_waiting(&stopped, 1);
    assert_eq!(stopped.items[0].id, 1);
}

#[tokio::test]
async fn slow_subscriber_reads_only_the_latest_batch_snapshot_in_publish_order() {
    let (mut actor, mut output) = crate::actor::tests::actor();
    for index in 0..100 {
        actor
            .batch_command(crate::batch::Request::Add(vec![PathBuf::from(format!(
                "{index}.sgf"
            ))]))
            .await
            .unwrap();
    }
    actor.publish_document().unwrap();
    let final_status = actor
        .batch_command(crate::batch::Request::Remove(0))
        .await
        .unwrap();
    drop(actor);
    let first = within(output.recv()).await.unwrap();
    assert!(matches!(first, AppEvent::DocumentChanged { .. }));
    assert_eq!(
        within(output.recv()).await,
        Some(AppEvent::BatchAnalysisProgress(final_status))
    );
    assert_eq!(within(output.recv()).await, None);
}
