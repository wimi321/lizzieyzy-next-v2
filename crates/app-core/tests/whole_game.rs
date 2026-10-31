#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{
    AnalysisSource, AnalysisView, AppConfig, AppCore, AppError, AppEvent, AppSubscription,
    DocumentView, EngineProfile, EngineStatus, WholeGameError, WholeGameOptions, WholeGameProgress,
    WholeGameStatus,
};
use serde_json::Value;
use std::{collections::BTreeSet, future::Future, path::Path, time::Duration};
use tempfile::TempDir;
use tokio::time::timeout;

async fn within<T>(future: impl Future<Output = T>) -> T {
    timeout(Duration::from_secs(5), future)
        .await
        .expect("whole-game operation exceeded safety deadline")
}

fn profile(directory: &Path, visits_per_second: u64, args: &[&str]) -> EngineProfile {
    let model = directory.join("model.bin");
    let config = directory.join("analysis.cfg");
    std::fs::write(&model, "fake model").unwrap();
    std::fs::write(&config, "numAnalysisThreads = 1\n").unwrap();
    let mut extra_args = vec![
        "--visits-per-sec".into(),
        visits_per_second.to_string(),
        "--request-log".into(),
        directory.join("requests.jsonl").to_str().unwrap().into(),
    ];
    extra_args.extend(args.iter().map(|arg| (*arg).into()));
    EngineProfile {
        canonical_model_name: None,
        program: fake_katago::binary_path().unwrap(),
        model,
        config,
        extra_args,
    }
}

async fn app(directory: &Path, live_visits: u64) -> (AppCore, AppSubscription) {
    let mut config = AppConfig::default();
    config.analysis.live.max_visits = live_visits;
    config.analysis.live.report_interval = Duration::from_millis(10);
    // Force persistent results to survive independent of the live LRU.
    config.analysis.cache_capacity = 1;
    // Fault injection is explicit; CPU load must not manufacture watchdog failures.
    config.session.startup_timeout = Duration::from_secs(60);
    config.session.response_timeout = Duration::from_secs(60);
    config.session.write_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(60);
    within(app_core::spawn_with_config(directory, config))
        .await
        .unwrap()
}

async fn document(output: &mut AppSubscription, revision: u64) -> DocumentView {
    within(async {
        loop {
            match output
                .recv()
                .await
                .expect("application event stream closed")
            {
                AppEvent::DocumentChanged {
                    revision: actual,
                    view,
                } if actual == revision => return view,
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("unexpected engine failure: {reason}")
                }
                _ => {}
            }
        }
    })
    .await
}

async fn completed(output: &mut AppSubscription, initial: &WholeGameProgress) -> WholeGameProgress {
    within(async {
        let mut previous = initial.completed;
        loop {
            match output
                .recv()
                .await
                .expect("application event stream closed")
            {
                AppEvent::WholeGameProgress(progress) => {
                    assert_eq!(progress.total, initial.total);
                    assert!(progress.completed >= previous, "progress moved backwards");
                    assert!(progress.completed <= progress.total);
                    previous = progress.completed;
                    match progress.status {
                        WholeGameStatus::Completed => {
                            assert_eq!(progress.completed, progress.total);
                            return progress;
                        }
                        WholeGameStatus::Running => {}
                        other => panic!("unexpected review state: {other:?}"),
                    }
                }
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("unexpected engine failure: {reason}")
                }
                _ => {}
            }
        }
    })
    .await
}

fn line(view: &DocumentView) -> Vec<u32> {
    view.graph
        .points
        .iter()
        .map(|point| point.node_id)
        .collect()
}

fn requests(directory: &Path) -> Vec<Value> {
    std::fs::read_to_string(directory.join("requests.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn review_requests(directory: &Path) -> Vec<Value> {
    requests(directory)
        .into_iter()
        .filter(|request| request["priority"].as_i64().is_some_and(|value| value < 0))
        .collect()
}

async fn logged(directory: &Path, predicate: impl Fn(&Value) -> bool) -> Value {
    within(async {
        loop {
            if let Ok(text) = std::fs::read_to_string(directory.join("requests.jsonl")) {
                // A concurrent write can leave the final line incomplete.
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

fn pass_game(moves: usize) -> Vec<u8> {
    let mut source = String::from("(;SZ[9]");
    for index in 0..moves {
        source.push_str(if index % 2 == 0 { ";B[]" } else { ";W[]" });
    }
    source.push(')');
    source.into_bytes()
}

#[tokio::test]
async fn selected_line_including_root_empty_and_setup_nodes_has_full_results_and_graph() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), 100_000, &[]);
    let (handle, mut output) = app(directory.path(), 100_000).await;
    within(handle.set_pondering(false)).await.unwrap();
    let source = b"(;SZ[9];B[ee](;W[cc];B[dd])(;W[gg];C[empty];AB[aa]PL[B];B[ff];W[]))";
    let parsed = sgf::parse(source).unwrap();
    let reference = document::GameDocument::from_sgf(&parsed.games[0]).unwrap();
    let fork = reference.node(reference.root()).unwrap().children()[0];
    let alternate = reference.node(fork).unwrap().children()[1];
    let unselected = reference.node(fork).unwrap().children()[0];
    within(handle.open_bytes(source.to_vec())).await.unwrap();
    let revision = within(handle.go_to(alternate.0)).await.unwrap();
    let initial = document(&mut output, revision).await;
    let nodes = line(&initial);
    assert_eq!(nodes.len(), 7);
    assert_eq!(nodes[0], reference.root().0);
    within(handle.configure_engine(Some(engine))).await.unwrap();
    let started = within(handle.start_whole_game_analysis(WholeGameOptions { visits: 500 }))
        .await
        .unwrap();
    assert_eq!((started.completed, started.total), (0, nodes.len()));
    completed(&mut output, &started).await;

    for node in &nodes {
        let result = within(handle.whole_game_result(*node))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result.node_id, *node);
        assert!(result.visits >= 500);
        assert!(result.winrate.is_finite() && (0.0..=1.0).contains(&result.winrate));
        assert!(result.score_lead.is_finite());
        assert!(!result.is_during_search);
        assert!(!result.candidates.is_empty());
        let position = reference.position_at(document::NodeId(*node)).unwrap();
        let raw = within(handle.whole_game_analysis(*node))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(raw.node_id.0, *node);
        assert_eq!(raw.key, position.analysis_key());
        assert_eq!(raw.response.root_info.winrate, result.winrate);
        assert_eq!(raw.response.root_info.score_lead, result.score_lead);
        assert_eq!(raw.response.root_info.visits, result.visits);
        assert_eq!(raw.response.ownership, result.ownership);
        assert_eq!(raw.response.move_infos.len(), result.candidates.len());
        for (candidate, raw_candidate) in result.candidates.iter().zip(&raw.response.move_infos) {
            assert!(!candidate.pv.is_empty());
            let first: go_core::Move = candidate.pv[0].into();
            assert_eq!(first.color(), position.board.to_play());
            assert_eq!(
                first.to_gtp(position.board.size()).unwrap(),
                candidate.coordinate
            );
            assert_eq!(candidate.coordinate, raw_candidate.coordinate);
            assert_eq!(candidate.visits, raw_candidate.visits);
            assert_eq!(candidate.winrate, raw_candidate.winrate);
            assert_eq!(candidate.score_lead, raw_candidate.score_lead);
            assert_eq!(candidate.pv.len(), raw_candidate.pv.len());
            for (mv, coordinate) in candidate.pv.iter().zip(&raw_candidate.pv) {
                let mv: go_core::Move = (*mv).into();
                assert_eq!(&mv.to_gtp(position.board.size()).unwrap(), coordinate);
            }
        }
    }
    assert!(
        within(handle.whole_game_result(unselected.0))
            .await
            .unwrap()
            .is_none()
    );
    let revision = within(handle.apply(document::Command::SetComment("refresh graph".into())))
        .await
        .unwrap();
    let graph = document(&mut output, revision).await;
    assert_eq!(line(&graph), nodes);
    assert!(
        graph
            .graph
            .points
            .iter()
            .all(|point| point.summary.is_some_and(|value| value.visits >= 500))
    );
    // Saved review results remain displayable with live pondering disabled.
    let revision = within(handle.go_to(initial.tree.root)).await.unwrap();
    document(&mut output, revision).await;
    let cached = within(async {
        loop {
            if let AppEvent::AnalysisUpdated(value) = output.recv().await.unwrap() {
                return value;
            }
        }
    })
    .await;
    assert_eq!(
        (cached.node_id, cached.revision),
        (initial.tree.root, revision)
    );
    assert_eq!(cached.source, AnalysisSource::Cache);
    assert_eq!(cached.visits, 500);
    assert!(!within(handle.settings()).await.unwrap().pondering);
    within(handle.shutdown()).await.unwrap();
    let wire = review_requests(directory.path());
    assert!(
        wire.iter()
            .any(|query| query["analyzeTurns"].as_array().unwrap().len() > 1)
    );
    assert!(wire.iter().all(|query| query["maxVisits"] == 500));
}

#[tokio::test]
async fn invalid_visits_and_missing_engine_leave_task_state_unchanged() {
    let directory = TempDir::new().unwrap();
    let (handle, _output) = app(directory.path(), 10_000).await;
    for visits in [0, 499, 1_000_001, u64::MAX] {
        let error = within(handle.start_whole_game_analysis(WholeGameOptions { visits }))
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            AppError::WholeGame(WholeGameError::InvalidVisits)
        ));
        assert!(
            within(handle.whole_game_progress())
                .await
                .unwrap()
                .is_none()
        );
    }
    for visits in [500, 1_000, 1_000_000] {
        let error = within(handle.start_whole_game_analysis(WholeGameOptions { visits }))
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            AppError::WholeGame(WholeGameError::NoEngine)
        ));
        assert!(
            within(handle.whole_game_progress())
                .await
                .unwrap()
                .is_none()
        );
    }
    let engine = profile(directory.path(), 100_000, &[]);
    within(handle.set_pondering(false)).await.unwrap();
    within(handle.configure_engine(Some(engine))).await.unwrap();
    within(handle.start_whole_game_analysis(WholeGameOptions { visits: 1_000_000 }))
        .await
        .unwrap();
    let active = logged(directory.path(), |query| query["maxVisits"] == 1_000_000).await;
    let paused = within(handle.pause_whole_game_analysis()).await.unwrap();
    assert!(
        within(handle.start_whole_game_analysis(WholeGameOptions { visits: 499 }))
            .await
            .is_err()
    );
    assert_eq!(
        within(handle.whole_game_progress()).await.unwrap(),
        Some(paused)
    );
    logged(directory.path(), |query| {
        query["action"] == "terminate" && query["terminateId"] == active["id"]
    })
    .await;
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn deleting_a_captured_node_while_paused_reports_error_on_resume() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), 100_000, &[]);
    let (handle, mut output) = app(directory.path(), 10_000).await;
    within(handle.set_pondering(false)).await.unwrap();
    let revision = within(handle.open_bytes(pass_game(3))).await.unwrap();
    let initial = document(&mut output, revision).await;
    within(handle.configure_engine(Some(engine))).await.unwrap();
    within(handle.start_whole_game_analysis(WholeGameOptions { visits: 1_000_000 }))
        .await
        .unwrap();
    let active = logged(directory.path(), |query| query["maxVisits"] == 1_000_000).await;
    let paused = within(handle.pause_whole_game_analysis()).await.unwrap();
    assert_eq!(paused.status, WholeGameStatus::Paused);
    let removed = initial.graph.points[1].node_id;
    within(handle.apply(document::Command::DeleteNode(document::NodeId(removed))))
        .await
        .unwrap();
    let resumed = within(handle.resume_whole_game_analysis()).await.unwrap();
    match resumed.status {
        WholeGameStatus::Error { reason } => assert!(reason.contains("removed or changed")),
        other => panic!("removed review line must not remain running: {other:?}"),
    }
    assert!(
        within(handle.whole_game_result(removed))
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        within(handle.whole_game_analysis(removed))
            .await
            .unwrap()
            .is_none()
    );
    logged(directory.path(), |query| {
        query["action"] == "terminate" && query["terminateId"] == active["id"]
    })
    .await;
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn live_analysis_keeps_producing_results_during_review_and_after_navigation() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), 20_000, &[]);
    let (handle, mut output) = app(directory.path(), 1_000_000).await;
    let revision = within(handle.open_bytes(pass_game(12))).await.unwrap();
    let initial = document(&mut output, revision).await;
    within(handle.configure_engine(Some(engine))).await.unwrap();
    let started = within(handle.start_whole_game_analysis(WholeGameOptions { visits: 5_000 }))
        .await
        .unwrap();
    within(async {
        let mut live_visits = Vec::new();
        loop {
            match output.recv().await.unwrap() {
                AppEvent::AnalysisUpdated(value) if value.source == AnalysisSource::Live => {
                    assert_eq!(
                        (value.revision, value.node_id),
                        (revision, initial.tree.root)
                    );
                    live_visits.push(value.visits);
                    if live_visits.len() >= 3 {
                        assert!(live_visits.windows(2).all(|pair| pair[1] > pair[0]));
                        break;
                    }
                }
                AppEvent::WholeGameProgress(progress) => {
                    assert_eq!(progress.status, WholeGameStatus::Running);
                }
                _ => {}
            }
        }
    })
    .await;
    let node = initial.graph.points[3].node_id;
    let revision = within(handle.go_to(node)).await.unwrap();
    document(&mut output, revision).await;
    within(async {
        loop {
            if let AppEvent::AnalysisUpdated(value) = output.recv().await.unwrap()
                && value.source == AnalysisSource::Live
            {
                assert_eq!((value.revision, value.node_id), (revision, node));
                break;
            }
        }
    })
    .await;
    completed(&mut output, &started).await;
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn pause_resume_submits_only_missing_turns_and_cancel_reaches_engine() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), 5_000, &[]);
    let (handle, mut output) = app(directory.path(), 10_000).await;
    within(handle.set_pondering(false)).await.unwrap();
    let revision = within(handle.open_bytes(pass_game(20))).await.unwrap();
    let initial = document(&mut output, revision).await;
    within(handle.configure_engine(Some(engine))).await.unwrap();
    within(handle.start_whole_game_analysis(WholeGameOptions { visits: 500 }))
        .await
        .unwrap();
    within(async {
        loop {
            if let AppEvent::WholeGameProgress(progress) = output.recv().await.unwrap()
                && progress.completed > 0
            {
                assert!(progress.completed < progress.total);
                break;
            }
        }
    })
    .await;
    let paused = within(handle.pause_whole_game_analysis()).await.unwrap();
    assert_eq!(paused.status, WholeGameStatus::Paused);
    assert!(paused.completed > 0 && paused.completed < paused.total);
    let mut finished = BTreeSet::new();
    for (turn, node) in line(&initial).into_iter().enumerate() {
        if within(handle.whole_game_result(node))
            .await
            .unwrap()
            .is_some()
        {
            finished.insert(turn as u64);
        }
    }
    assert_eq!(finished.len(), paused.completed);
    let before_resume = review_requests(directory.path()).len();
    let resumed = within(handle.resume_whole_game_analysis()).await.unwrap();
    assert_eq!(resumed.completed, paused.completed);
    completed(&mut output, &resumed).await;
    let wire = review_requests(directory.path());
    assert!(wire.len() > before_resume);
    for request in &wire[before_resume..] {
        for turn in request["analyzeTurns"].as_array().unwrap() {
            assert!(
                !finished.contains(&turn.as_u64().unwrap()),
                "completed turn submitted again"
            );
        }
    }

    // A higher target starts actual work, so cancellation must reach the engine.
    within(handle.start_whole_game_analysis(WholeGameOptions { visits: 1_000_000 }))
        .await
        .unwrap();
    let active = logged(directory.path(), |query| query["maxVisits"] == 1_000_000).await;
    let cancelled = within(handle.cancel_whole_game_analysis()).await.unwrap();
    assert_eq!(cancelled.status, WholeGameStatus::Cancelled);
    logged(directory.path(), |query| {
        query["action"] == "terminate" && query["terminateId"] == active["id"]
    })
    .await;
    let after_cancel = within(handle.whole_game_progress()).await.unwrap().unwrap();
    assert_eq!(after_cancel, cancelled);
    within(handle.shutdown()).await.unwrap();
    within(async {
        while let Some(event) = output.recv().await {
            if let AppEvent::WholeGameProgress(progress) = event {
                assert_eq!(progress, cancelled, "progress continued after cancellation");
            }
        }
    })
    .await;
    let wire = requests(directory.path());
    let reviews: Vec<_> = wire
        .iter()
        .filter(|request| request["priority"].as_i64().is_some_and(|p| p < 0))
        .collect();
    let last_id = &reviews.last().unwrap()["id"];
    assert!(
        wire.iter()
            .any(|request| request["action"] == "terminate" && &request["terminateId"] == last_id)
    );
}

#[tokio::test]
async fn enough_visits_are_skipped_and_persistent_result_precedes_live_after_lru_eviction() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), 10_000, &[]);
    let (handle, mut output) = app(directory.path(), 10_000).await;
    within(handle.set_pondering(false)).await.unwrap();
    let revision = within(handle.open_bytes(pass_game(3))).await.unwrap();
    let initial = document(&mut output, revision).await;
    within(handle.configure_engine(Some(engine))).await.unwrap();
    let started = within(handle.start_whole_game_analysis(WholeGameOptions { visits: 500 }))
        .await
        .unwrap();
    completed(&mut output, &started).await;
    let root = within(handle.whole_game_result(initial.tree.root))
        .await
        .unwrap()
        .unwrap();
    let wire_count = review_requests(directory.path()).len();
    let skipped = within(handle.start_whole_game_analysis(WholeGameOptions { visits: 500 }))
        .await
        .unwrap();
    assert_eq!(skipped.status, WholeGameStatus::Completed);
    assert_eq!((skipped.completed, skipped.total), (4, 4));

    // Occupy the one-entry live cache with the last node, then revisit the root.
    let tail = initial.graph.points.last().unwrap().node_id;
    within(handle.go_to(tail)).await.unwrap();
    within(handle.set_pondering(true)).await.unwrap();
    within(async {
        loop {
            if let AppEvent::AnalysisUpdated(value) = output.recv().await.unwrap()
                && value.node_id == tail
                && value.source == AnalysisSource::Live
            {
                break;
            }
        }
    })
    .await;
    let revision = within(handle.go_to(initial.tree.root)).await.unwrap();
    document(&mut output, revision).await;
    let cached: AnalysisView = within(async {
        loop {
            if let AppEvent::AnalysisUpdated(value) = output.recv().await.unwrap() {
                return value;
            }
        }
    })
    .await;
    assert_eq!(
        (cached.node_id, cached.revision),
        (initial.tree.root, revision)
    );
    assert_eq!(cached.source, AnalysisSource::Cache);
    assert_eq!(cached.visits, root.visits);
    assert_eq!(cached.candidates, root.candidates);
    within(handle.shutdown()).await.unwrap();
    assert_eq!(review_requests(directory.path()).len(), wire_count);
}

#[tokio::test]
async fn failed_open_retains_results_successful_open_cancels_and_clears_them() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), 10_000, &[]);
    let (handle, mut output) = app(directory.path(), 10_000).await;
    within(handle.set_pondering(false)).await.unwrap();
    let revision = within(handle.open_bytes(pass_game(3))).await.unwrap();
    let initial = document(&mut output, revision).await;
    within(handle.configure_engine(Some(engine))).await.unwrap();
    let started = within(handle.start_whole_game_analysis(WholeGameOptions { visits: 500 }))
        .await
        .unwrap();
    completed(&mut output, &started).await;
    let root = within(handle.whole_game_result(initial.tree.root))
        .await
        .unwrap()
        .unwrap();
    assert!(
        within(handle.open_bytes(b"not SGF".to_vec()))
            .await
            .is_err()
    );
    assert_eq!(
        within(handle.whole_game_result(initial.tree.root))
            .await
            .unwrap()
            .unwrap(),
        root
    );
    within(handle.start_whole_game_analysis(WholeGameOptions { visits: 1_000_000 }))
        .await
        .unwrap();
    let active = logged(directory.path(), |query| query["maxVisits"] == 1_000_000).await;
    let revision = within(handle.open_bytes(b"(;SZ[13];B[aa])".to_vec()))
        .await
        .unwrap();
    let replacement = document(&mut output, revision).await;
    assert_eq!(replacement.tree.root, initial.tree.root);
    assert!(
        replacement
            .graph
            .points
            .iter()
            .all(|point| point.summary.is_none())
    );
    assert!(
        within(handle.whole_game_result(replacement.tree.root))
            .await
            .unwrap()
            .is_none()
    );
    let progress = within(handle.whole_game_progress()).await.unwrap();
    assert!(progress.is_none_or(|progress| progress.status == WholeGameStatus::Cancelled));
    logged(directory.path(), |query| {
        query["action"] == "terminate" && query["terminateId"] == active["id"]
    })
    .await;
    within(handle.shutdown()).await.unwrap();
    let wire = requests(directory.path());
    let last = review_requests(directory.path()).pop().unwrap();
    assert!(wire.iter().any(|request| request["action"] == "terminate" && request["terminateId"] == last["id"]));
}

#[tokio::test]
async fn same_fingerprint_preserves_results_metadata_change_and_disable_clear_them() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), 100_000, &[]);
    let (handle, mut output) = app(directory.path(), 10_000).await;
    within(handle.set_pondering(false)).await.unwrap();
    within(handle.configure_engine(Some(engine.clone())))
        .await
        .unwrap();
    let started = within(handle.start_whole_game_analysis(WholeGameOptions { visits: 500 }))
        .await
        .unwrap();
    completed(&mut output, &started).await;
    let root = within(handle.whole_game_result(0)).await.unwrap().unwrap();
    within(handle.configure_engine(Some(engine.clone())))
        .await
        .unwrap();
    assert_eq!(
        within(handle.whole_game_result(0)).await.unwrap().unwrap(),
        root
    );
    std::fs::write(&engine.model, "replacement model of a different size").unwrap();
    within(handle.configure_engine(Some(engine))).await.unwrap();
    assert!(within(handle.whole_game_result(0)).await.unwrap().is_none());
    let started = within(handle.start_whole_game_analysis(WholeGameOptions { visits: 500 }))
        .await
        .unwrap();
    completed(&mut output, &started).await;
    within(handle.configure_engine(None)).await.unwrap();
    assert!(within(handle.whole_game_result(0)).await.unwrap().is_none());
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn injected_engine_exit_finishes_review_or_reports_explicit_error() {
    let directory = TempDir::new().unwrap();
    let marker = directory.path().join("crashed-once");
    let engine = profile(
        directory.path(),
        100_000,
        &[
            "--crash-after-queries",
            "1",
            "--fault-once-file",
            marker.to_str().unwrap(),
        ],
    );
    let (handle, mut output) = app(directory.path(), 10_000).await;
    within(handle.set_pondering(false)).await.unwrap();
    within(handle.open_bytes(pass_game(3))).await.unwrap();
    within(handle.configure_engine(Some(engine))).await.unwrap();
    let started = within(handle.start_whole_game_analysis(WholeGameOptions { visits: 500 }))
        .await
        .unwrap();
    let terminal = within(async {
        loop {
            if let AppEvent::WholeGameProgress(progress) = output.recv().await.unwrap() {
                match &progress.status {
                    WholeGameStatus::Completed => {
                        assert_eq!(progress.completed, started.total);
                        return progress;
                    }
                    WholeGameStatus::Error { reason } => {
                        assert!(!reason.trim().is_empty());
                        return progress;
                    }
                    _ => {}
                }
            }
        }
    })
    .await;
    assert!(marker.is_file());
    assert_eq!(
        within(handle.whole_game_progress()).await.unwrap(),
        Some(terminal)
    );
    within(handle.shutdown()).await.unwrap();
}
