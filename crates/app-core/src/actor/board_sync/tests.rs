#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::{
    AppSubscription, EngineProfile,
    actor::{Input, Signal, next_signal, tests::actor},
};
use go_core::{BoardSize, Color};
use readboard::{Cell, Event, RemoteContext, Snapshot};
use serde_json::Value;
use std::{future::Future, path::Path, time::Duration};

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("board sync test exceeded safety deadline")
}

fn frame(second: bool) -> Snapshot {
    let mut cells = vec![Cell::Empty; 81];
    cells[0] = if second { Cell::Black } else { Cell::LastBlack };
    if second {
        cells[10] = Cell::LastWhite;
    }
    Snapshot {
        size: BoardSize::new(9, 9).unwrap(),
        cells,
        fox_move_number: None,
        force_rebuild: false,
        context: RemoteContext {
            last_move_source: Some("redBlueMarker".into()),
            ..Default::default()
        },
    }
}

async fn engine_actor(
    directory: &Path,
    max_visits: u64,
    visits_per_second: &str,
) -> (Actor, AppSubscription) {
    let program = fake_katago::binary_path().unwrap();
    let (mut actor, output) = actor();
    actor.directory = directory.to_owned();
    actor.settings.pondering = false;
    actor.settings.engine = Some(EngineProfile {
        canonical_model_name: None,
        model: program.clone(),
        config: program.clone(),
        program,
        extra_args: vec![
            "--visits-per-sec".into(),
            visits_per_second.into(),
            "--request-log".into(),
            directory.join("requests.jsonl").to_str().unwrap().into(),
        ],
    });
    actor.config.analysis.live.max_visits = max_visits;
    actor.config.analysis.live.report_interval = Duration::from_secs(60);
    actor.config.session.write_timeout = Duration::from_secs(60);
    actor.config.session.shutdown_timeout = Duration::from_secs(60);
    within(actor.settings.save(directory)).await.unwrap();
    within(actor.apply(Input::OpenBytes(b"(;SZ[9])".to_vec())))
        .await
        .unwrap();
    within(actor.start_engine()).await;
    (actor, output)
}

async fn retained_final(actor: &mut Actor, node: NodeId) {
    within(async {
        loop {
            let signal = next_signal(&mut actor.running).await;
            if let Signal::Engine(engine_session::EngineStatus::Failed { reason, .. }) = &signal {
                panic!("fake engine failed: {reason}");
            }
            actor.signal(signal);
            if actor
                .stored_analysis(node.0)
                .is_some_and(|stored| !stored.response.is_during_search)
            {
                return;
            }
        }
    })
    .await;
}

async fn logged(directory: &Path, predicate: impl Fn(&Value) -> bool) -> Value {
    within(async {
        loop {
            if let Ok(text) = std::fs::read_to_string(directory.join("requests.jsonl")) {
                for line in text.lines() {
                    if let Ok(value) = serde_json::from_str::<Value>(line)
                        && predicate(&value)
                    {
                        return value;
                    }
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await
}

#[tokio::test]
async fn synchronization_analyzes_remote_latest_without_overpainting_a_browsed_position() {
    let directory = tempfile::tempdir().unwrap();
    let (mut actor, mut output) = engine_actor(directory.path(), 100, "100000").await;
    within(actor.board_sync_event(Event::Snapshot(frame(false))))
        .await
        .unwrap();
    let first = actor.board_sync.resolved_node.unwrap();
    retained_final(&mut actor, first).await;
    assert!(!actor.settings.pondering);
    assert!(actor.pondering());

    let root = actor.document.root();
    within(actor.apply(Input::Edit(document::Command::GoTo(root))))
        .await
        .unwrap();
    within(actor.board_sync_event(Event::Snapshot(frame(true))))
        .await
        .unwrap();
    let latest = actor.board_sync.resolved_node.unwrap();
    assert_ne!(first, latest);
    assert_eq!(actor.document.current(), root);
    assert_eq!(actor.focus.as_ref().unwrap().node_id, latest.0);
    retained_final(&mut actor, latest).await;
    assert_eq!(actor.document.current(), root);
    let result = actor.review_view(latest.0).unwrap().unwrap();
    assert_eq!(result.visits, 100);
    assert_eq!(result.node_id, latest.0);
    assert_eq!(actor.document.position_at(latest).unwrap().moves.len(), 2);

    // An ordered status barrier drains all published events without sleeping or
    // guessing whether a wrong candidate-overlay event might arrive later.
    actor.publish_sync();
    let mut saw_graph = false;
    within(async {
        loop {
            match output.recv().await.unwrap() {
                AppEvent::AnalysisUpdated(view) => {
                    assert_eq!(
                        view.node_id, root.0,
                        "remote candidates overpainted browsing"
                    );
                }
                AppEvent::DocumentChanged { view, .. } => {
                    assert_eq!(view.tree.current, root.0);
                    let point = view
                        .graph
                        .points
                        .iter()
                        .find(|p| p.node_id == latest.0)
                        .unwrap();
                    assert_eq!(point.summary.unwrap().visits, 100);
                    saw_graph = true;
                }
                AppEvent::BoardSyncStatusChanged(_) => break,
                AppEvent::EngineStatusChanged(_) | AppEvent::WholeGameProgress(_) => {}
            }
        }
    })
    .await;
    assert!(saw_graph);
    let request = logged(directory.path(), |value| {
        value["moves"].as_array().is_some_and(|m| m.len() == 2)
    })
    .await;
    assert_eq!(
        request["moves"],
        serde_json::json!([["B", "A9"], ["W", "B8"]])
    );
    assert!(
        !crate::Settings::load(directory.path())
            .await
            .unwrap()
            .pondering
    );
    within(actor.stop_board_sync()).await.unwrap();
    within(actor.stop_engine()).await.unwrap();
}

#[tokio::test]
async fn helper_pause_and_resume_cancel_and_restart_analysis_without_persisting_preferences() {
    let directory = tempfile::tempdir().unwrap();
    let (mut actor, _output) = engine_actor(directory.path(), 1_000_000, "100").await;
    let settings = tokio::fs::read(directory.path().join("settings.json"))
        .await
        .unwrap();
    within(actor.board_sync_event(Event::Snapshot(frame(false))))
        .await
        .unwrap();
    let first = logged(directory.path(), |value| {
        value["moves"].as_array().is_some_and(|m| m.len() == 1)
    })
    .await;
    within(actor.board_sync_event(Event::PauseAnalysis))
        .await
        .unwrap();
    assert!(!actor.pondering());
    assert!(matches!(
        *actor
            .running
            .as_ref()
            .unwrap()
            .coordinator
            .status()
            .borrow(),
        analysis::AnalysisStatus::Idle
    ));
    logged(directory.path(), |value| {
        value["action"] == "terminate" && value["terminateId"] == first["id"]
    })
    .await;

    // Paused recognition continues updating the game, but must not re-enable
    // search. Resume starts one query for the newly received remote position.
    within(actor.board_sync_event(Event::Snapshot(frame(true))))
        .await
        .unwrap();
    assert!(!actor.pondering());
    assert!(matches!(
        *actor
            .running
            .as_ref()
            .unwrap()
            .coordinator
            .status()
            .borrow(),
        analysis::AnalysisStatus::Idle
    ));
    within(actor.board_sync_event(Event::ResumeAnalysis))
        .await
        .unwrap();
    let resumed = logged(directory.path(), |value| {
        value["moves"].as_array().is_some_and(|m| m.len() == 2)
    })
    .await;
    assert_ne!(first["id"], resumed["id"]);
    assert!(actor.pondering());
    assert!(!actor.settings.pondering);
    assert_eq!(
        tokio::fs::read(directory.path().join("settings.json"))
            .await
            .unwrap(),
        settings
    );
    within(actor.board_sync_event(Event::Ended)).await.unwrap();
    assert_eq!(actor.board_sync.status.phase, BoardSyncPhase::Stopped);
    assert!(!actor.pondering());
    assert!(matches!(
        *actor
            .running
            .as_ref()
            .unwrap()
            .coordinator
            .status()
            .borrow(),
        analysis::AnalysisStatus::Idle
    ));
    within(actor.stop_engine()).await.unwrap();
}

#[tokio::test]
async fn helper_clear_keeps_rectangular_dimensions_and_removes_the_remote_position() {
    let (mut actor, mut output) = actor();
    let parsed = sgf::parse(b"(;SZ[9:13];B[aa])").unwrap();
    actor.document = document::GameDocument::from_sgf(&parsed.games[0]).unwrap();
    actor.document.apply(document::Command::Step(1)).unwrap();
    actor.board_sync.status.phase = BoardSyncPhase::Syncing;
    actor.board_sync.status.remote_move_number = Some(42);
    actor.board_sync.resolved_node = Some(actor.document.current());
    within(actor.board_sync_event(Event::ClearBoard))
        .await
        .unwrap();
    let position = actor
        .document
        .position_at(actor.document.current())
        .unwrap();
    assert_eq!(position.board.size(), BoardSize::new(9, 13).unwrap());
    assert!(position.board.stones().iter().all(Option::is_none));
    assert!(position.moves.is_empty());
    assert_eq!(position.board.to_play(), Color::Black);
    assert_eq!(actor.board_sync.status.remote_move_number, None);
    assert_eq!(actor.board_sync.resolved_node, None);
    let mut saw_clear = false;
    within(async {
        loop {
            match output.recv().await.unwrap() {
                AppEvent::DocumentChanged { view, .. } => {
                    let board = view.board.unwrap();
                    assert_eq!((board.width, board.height), (9, 13));
                    saw_clear = true;
                }
                AppEvent::BoardSyncStatusChanged(_) => break,
                _ => {}
            }
        }
    })
    .await;
    assert!(saw_clear);
}

#[tokio::test]
async fn helper_end_is_normal_stopped_and_keeps_the_last_board() {
    let (mut actor, mut output) = actor();
    actor
        .board_sync_event(Event::Snapshot(frame(false)))
        .await
        .unwrap();
    let before = actor.document.snapshot();
    actor.board_sync_event(Event::Ended).await.unwrap();
    assert_eq!(actor.board_sync.status.phase, BoardSyncPhase::Stopped);
    assert!(actor.board_sync.status.error.is_none());
    assert_eq!(actor.document.snapshot(), before);
    assert!(actor.board_sync.resolved_node.is_none());
    within(async {
        loop {
            if let AppEvent::BoardSyncStatusChanged(status) = output.recv().await.unwrap() {
                assert_eq!(status.phase, BoardSyncPhase::Stopped);
                assert!(status.error.is_none());
                break;
            }
        }
    })
    .await;
}

#[tokio::test]
async fn failed_preference_save_does_not_pause_an_active_sync_session() {
    let directory = tempfile::tempdir().unwrap();
    let settings_path = directory.path().join("settings.json");
    // A directory at the atomic replacement destination fails on every platform.
    tokio::fs::create_dir(&settings_path).await.unwrap();
    let (mut actor, _output) = actor();
    actor.directory = directory.path().to_owned();
    actor.settings.pondering = true;
    actor.board_sync.status.phase = BoardSyncPhase::Syncing;
    actor.board_sync.analysis_paused = false;
    let original_document = actor.document.snapshot();
    let original_revision = actor.revision;

    let result = within(actor.apply(Input::SetPondering(false))).await;
    assert!(matches!(result, Err(AppError::Settings(_))));
    assert!(actor.settings.pondering);
    assert!(!actor.board_sync.analysis_paused);
    assert!(actor.pondering());
    assert_eq!(actor.board_sync.status.phase, BoardSyncPhase::Syncing);
    assert_eq!(actor.document.snapshot(), original_document);
    assert_eq!(actor.revision, original_revision);
    assert!(tokio::fs::metadata(settings_path).await.unwrap().is_dir());
}

#[tokio::test]
async fn repeating_saved_preference_still_updates_a_temporary_sync_pause() {
    let directory = tempfile::tempdir().unwrap();
    let (mut actor, _output) = actor();
    actor.directory = directory.path().to_owned();
    actor.board_sync.status.phase = BoardSyncPhase::Syncing;
    for enabled in [false, true] {
        actor.settings.pondering = enabled;
        actor.board_sync.analysis_paused = enabled;
        actor.settings.save(directory.path()).await.unwrap();
        let before = tokio::fs::read(directory.path().join("settings.json"))
            .await
            .unwrap();
        within(actor.apply(Input::SetPondering(enabled)))
            .await
            .unwrap();
        assert_eq!(actor.settings.pondering, enabled);
        assert_eq!(actor.board_sync.analysis_paused, !enabled);
        assert_eq!(actor.pondering(), enabled);
        assert_eq!(
            tokio::fs::read(directory.path().join("settings.json"))
                .await
                .unwrap(),
            before
        );
    }
}
