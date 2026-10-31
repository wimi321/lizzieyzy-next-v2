#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{
    AppConfig, AppCore, AppEvent, AppSubscription, ColorView, DocumentView, EngineProfile,
    EngineStatus, MoveRank, MoveRankEvaluationMode, MoveRankSettings, ProblemMoveSort,
    ProblemMovesQuery, ProblemMovesView, Settings, WholeGameOptions, WholeGameStatus,
};
use document::{GameDocument, NodeId};
use go_core::Color;
use serde_json::Value;
use std::{future::Future, path::Path, time::Duration};

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("problem-moves operation exceeded safety deadline")
}

async fn app(directory: &Path) -> (AppCore, AppSubscription) {
    let mut config = AppConfig::default();
    config.analysis.cache_capacity = 1;
    // Fault injection is explicit; slow CI must not create unrelated failures.
    config.session.startup_timeout = Duration::from_secs(60);
    config.session.response_timeout = Duration::from_secs(60);
    config.session.write_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(60);
    within(app_core::spawn_with_config(directory, config))
        .await
        .unwrap()
}

fn profile(directory: &Path, speed: u64) -> EngineProfile {
    let program = fake_katago::binary_path().unwrap();
    EngineProfile {
        canonical_model_name: None,
        model: program.clone(),
        config: program.clone(),
        program,
        extra_args: vec![
            "--visits-per-sec".into(),
            speed.to_string(),
            "--request-log".into(),
            directory.join("requests.jsonl").to_str().unwrap().into(),
        ],
    }
}

fn searches(directory: &Path) -> Vec<Value> {
    std::fs::read_to_string(directory.join("requests.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|request| request.get("analyzeTurns").is_some())
        .collect()
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
                    panic!("unexpected fake engine failure: {reason}")
                }
                _ => {}
            }
        }
    })
    .await
}

async fn completed(output: &mut AppSubscription) {
    within(async {
        loop {
            match output
                .recv()
                .await
                .expect("application event stream closed")
            {
                AppEvent::WholeGameProgress(progress) => match progress.status {
                    WholeGameStatus::Completed => {
                        assert_eq!(progress.completed, progress.total);
                        return;
                    }
                    WholeGameStatus::Running => {}
                    other => panic!("unexpected whole-game state: {other:?}"),
                },
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("unexpected fake engine failure: {reason}")
                }
                _ => {}
            }
        }
    })
    .await;
}

fn all_moves() -> ProblemMovesQuery {
    ProblemMovesQuery {
        minimum_rank: MoveRank::Best,
        ..ProblemMovesQuery::default()
    }
}

fn pass_game(moves: usize) -> Vec<u8> {
    let mut source = String::from("(;SZ[9]");
    for index in 0..moves {
        source.push_str(if index % 2 == 0 { ";B[]" } else { ";W[]" });
    }
    source.push(')');
    source.into_bytes()
}

async fn reviewed(
    directory: &Path,
    source: Vec<u8>,
) -> (AppCore, AppSubscription, DocumentView, u64) {
    let engine = profile(directory, 100_000);
    let (handle, mut output) = app(directory).await;
    within(handle.set_pondering(false)).await.unwrap();
    let revision = within(handle.open_bytes(source)).await.unwrap();
    let initial = document(&mut output, revision).await;
    within(handle.configure_engine(Some(engine))).await.unwrap();
    within(handle.start_whole_game_analysis(WholeGameOptions { visits: 500 }))
        .await
        .unwrap();
    completed(&mut output).await;
    (handle, output, initial, revision)
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-10,
        "expected {expected}, got {actual}"
    );
}

async fn assert_raw_losses(handle: &AppCore, reference: &GameDocument, view: &ProblemMovesView) {
    let nodes = reference.variation_tree().unwrap();
    for bar in &view.error_bar {
        let node = nodes.iter().find(|node| node.id.0 == bar.node_id).unwrap();
        assert_eq!(bar.move_number, node.move_number);
        assert_eq!(bar.mv, node.mv.map(Into::into));
        let Some(mv) = node.mv else {
            assert!(bar.loss.is_none(), "empty/root node must have no loss");
            continue;
        };
        let parent = node.parent.unwrap();
        let before = within(handle.whole_game_analysis(parent.0))
            .await
            .unwrap()
            .unwrap();
        let after = within(handle.whole_game_analysis(node.id.0))
            .await
            .unwrap()
            .unwrap();
        let size = reference.position_at(node.id).unwrap().board.size();
        let coordinate = mv.to_gtp(size).unwrap();
        let best = before
            .response
            .move_infos
            .iter()
            .min_by_key(|m| m.order)
            .unwrap();
        let candidate = before
            .response
            .move_infos
            .iter()
            .find(|candidate| candidate.coordinate == coordinate);
        let loss = bar
            .loss
            .as_ref()
            .expect("both adjacent positions were analyzed");
        let is_best = best.coordinate == coordinate;
        let sign = if mv.color() == Color::Black {
            1.0
        } else {
            -1.0
        };
        let expected_winrate = if is_best {
            0.0
        } else {
            ((best.winrate - after.response.root_info.winrate) * sign * 100.0).max(0.0)
        };
        let expected_score = if is_best {
            0.0
        } else {
            ((best.score_lead - after.response.root_info.score_lead) * sign).max(0.0)
        };
        assert_close(loss.winrate_loss, expected_winrate);
        assert_close(loss.score_loss, expected_score);
        assert_eq!(loss.is_best, is_best);
        assert_eq!(
            loss.candidate_rank,
            candidate.map(|candidate| candidate.order + 1)
        );
        assert_eq!(loss.before_visits, before.response.root_info.visits);
        assert_eq!(loss.visits, after.response.root_info.visits);
        let listed = view
            .entries
            .iter()
            .find(|entry| entry.node_id == bar.node_id);
        if is_best {
            assert!(listed.is_none());
        } else {
            let entry = listed.expect("minimum Best includes every non-best move");
            assert_eq!(entry.coordinate, coordinate);
            assert_eq!(entry.color, ColorView::from(mv.color()));
            assert_eq!(entry.move_number, node.move_number);
            assert_eq!(&entry.loss, loss);
        }
    }
}

#[tokio::test]
async fn whole_game_problem_moves_match_protocol_losses_filters_sort_and_jump_nodes() {
    let directory = tempfile::tempdir().unwrap();
    // The comment-only node separates node IDs from move numbers, and pass is
    // intentionally outside fake-katago's three ordinary-point candidates.
    let source = b"(;SZ[9];B[aa];C[between];W[bb];B[];W[cc];B[dd])";
    let parsed = sgf::parse(source).unwrap();
    let reference = GameDocument::from_sgf(&parsed.games[0]).unwrap();
    let (handle, mut output, initial, revision) = reviewed(directory.path(), source.to_vec()).await;
    let original_sgf = directory.path().join("before.sgf");
    within(handle.save(&original_sgf)).await.unwrap();
    let original_bytes = std::fs::read(&original_sgf).unwrap();
    let original_searches = searches(directory.path());

    let view = within(handle.problem_moves(all_moves())).await.unwrap();
    assert_eq!(view.revision, revision);
    assert_eq!(view.current_index, initial.graph.current_index);
    assert_eq!(view.error_bar.len(), initial.graph.points.len());
    for (bar, graph) in view.error_bar.iter().zip(&initial.graph.points) {
        assert_eq!(
            (bar.node_id, bar.move_number),
            (graph.node_id, graph.move_number)
        );
    }
    assert_raw_losses(&handle, &reference, &view).await;
    assert_eq!(
        view.error_bar
            .iter()
            .filter(|bar| bar.loss.is_some())
            .count(),
        5
    );
    let pass = view
        .entries
        .iter()
        .find(|entry| entry.coordinate == "pass")
        .unwrap();
    assert_eq!(pass.loss.candidate_rank, None);
    assert!(!pass.loss.is_best);
    for pair in view.entries.windows(2) {
        assert!(pair[0].loss.winrate_loss >= pair[1].loss.winrate_loss);
        if pair[0].loss.winrate_loss == pair[1].loss.winrate_loss {
            assert!(pair[0].move_number < pair[1].move_number);
        }
    }

    for color in [ColorView::Black, ColorView::White] {
        let filtered = within(handle.problem_moves(ProblemMovesQuery {
            color: Some(color),
            ..all_moves()
        }))
        .await
        .unwrap();
        let expected: Vec<_> = view
            .entries
            .iter()
            .filter(|entry| entry.color == color)
            .cloned()
            .collect();
        assert!(
            !expected.is_empty(),
            "fixture must exercise both color filters"
        );
        assert_eq!(filtered.entries, expected);
        assert_eq!(
            filtered.error_bar, view.error_bar,
            "filter must not truncate the bar"
        );
    }
    for sort in [ProblemMoveSort::MoveAsc, ProblemMoveSort::MoveDesc] {
        let sorted = within(handle.problem_moves(ProblemMovesQuery {
            sort,
            ..all_moves()
        }))
        .await
        .unwrap();
        let mut expected = view.entries.clone();
        expected.sort_by_key(|entry| entry.move_number);
        if sort == ProblemMoveSort::MoveDesc {
            expected.reverse();
        }
        assert_eq!(sorted.entries, expected);
    }
    let after_sgf = directory.path().join("after.sgf");
    within(handle.save(&after_sgf)).await.unwrap();
    assert_eq!(std::fs::read(&after_sgf).unwrap(), original_bytes);
    assert_eq!(
        within(handle.problem_moves(all_moves())).await.unwrap(),
        view
    );

    // Navigate using the supplied ID, not the move number or list index.
    let entry = view
        .entries
        .iter()
        .find(|entry| entry.node_id != entry.move_number as u32)
        .unwrap();
    let moved = within(handle.go_to(entry.node_id)).await.unwrap();
    let focused = document(&mut output, moved).await;
    assert_eq!(focused.tree.current, entry.node_id);
    assert_eq!(focused.board.unwrap().move_number, entry.move_number);
    within(handle.shutdown()).await.unwrap();
    // Closing drains the process pipes, so a mistakenly submitted analysis
    // cannot hide behind an asynchronous write when checking the request log.
    assert_eq!(searches(directory.path()), original_searches);
}

#[tokio::test]
async fn progress_refreshes_partial_losses_and_resume_fills_the_bar() {
    let directory = tempfile::tempdir().unwrap();
    let engine = profile(directory.path(), 1_000);
    let (handle, mut output) = app(directory.path()).await;
    within(handle.set_pondering(false)).await.unwrap();
    within(handle.open_bytes(pass_game(8))).await.unwrap();
    let empty = within(handle.problem_moves(all_moves())).await.unwrap();
    assert!(empty.entries.is_empty());
    assert!(empty.error_bar.iter().all(|point| point.loss.is_none()));
    within(handle.configure_engine(Some(engine))).await.unwrap();
    within(handle.start_whole_game_analysis(WholeGameOptions { visits: 500 }))
        .await
        .unwrap();
    within(async {
        loop {
            if let AppEvent::WholeGameProgress(progress) = output.recv().await.unwrap()
                && progress.completed >= 2
            {
                assert_eq!(progress.status, WholeGameStatus::Running);
                break;
            }
        }
    })
    .await;
    let paused = within(handle.pause_whole_game_analysis()).await.unwrap();
    assert_eq!(paused.status, WholeGameStatus::Paused);
    let partial = within(handle.problem_moves(all_moves())).await.unwrap();
    let filled = partial
        .error_bar
        .iter()
        .filter(|point| point.loss.is_some())
        .count();
    assert!(
        filled > 0 && filled < 8,
        "pause must preserve a genuinely partial review"
    );
    assert_eq!(
        partial.entries.len(),
        filled,
        "pass never matches an ordinary candidate"
    );
    assert_eq!(partial.revision, empty.revision);
    within(handle.resume_whole_game_analysis()).await.unwrap();
    completed(&mut output).await;
    let final_view = within(handle.problem_moves(all_moves())).await.unwrap();
    assert_eq!(
        final_view
            .error_bar
            .iter()
            .filter(|point| point.loss.is_some())
            .count(),
        8
    );
    for old in partial
        .error_bar
        .iter()
        .filter(|point| point.loss.is_some())
    {
        assert_eq!(
            final_view
                .error_bar
                .iter()
                .find(|point| point.node_id == old.node_id)
                .unwrap(),
            old
        );
    }
    assert_eq!(final_view.revision, empty.revision);
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn threshold_changes_reclassify_retained_results_and_survive_restart_without_search() {
    let directory = tempfile::tempdir().unwrap();
    let (handle, mut output, _initial, revision) = reviewed(directory.path(), pass_game(8)).await;
    let original_searches = searches(directory.path());
    let query = ProblemMovesQuery {
        minimum_rank: MoveRank::Blunder,
        ..all_moves()
    };
    let restrictive = MoveRankSettings {
        mode: MoveRankEvaluationMode::Score,
        score_thresholds: [100.0, 200.0, 300.0, 400.0, 500.0],
        ..MoveRankSettings::default()
    };
    assert_eq!(
        within(handle.set_move_rank_settings(restrictive.clone()))
            .await
            .unwrap(),
        restrictive
    );
    document(&mut output, revision).await;
    assert!(
        within(handle.problem_moves(query))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    let sensitive = MoveRankSettings {
        score_thresholds: [0.0001, 0.0002, 0.0003, 0.0004, 0.0005],
        ..restrictive
    };
    within(handle.set_move_rank_settings(sensitive.clone()))
        .await
        .unwrap();
    document(&mut output, revision).await;
    let changed = within(handle.problem_moves(query)).await.unwrap();
    assert_eq!(changed.settings, sensitive);
    assert_eq!(changed.revision, revision);
    assert!(!changed.entries.is_empty());
    assert!(
        changed
            .entries
            .iter()
            .all(|entry| entry.loss.rank == MoveRank::Blunder)
    );
    assert_eq!(
        within(handle.settings()).await.unwrap().move_rank,
        sensitive
    );
    within(handle.shutdown()).await.unwrap();
    assert_eq!(searches(directory.path()), original_searches);

    let (reopened, _) = app(directory.path()).await;
    assert_eq!(
        within(reopened.settings()).await.unwrap().move_rank,
        sensitive
    );
    assert_eq!(
        within(reopened.problem_moves(all_moves()))
            .await
            .unwrap()
            .settings,
        sensitive
    );
    within(reopened.shutdown()).await.unwrap();
}

#[tokio::test]
async fn invalid_or_unwritable_rank_settings_preserve_the_last_saved_and_active_values() {
    let directory = tempfile::tempdir().unwrap();
    let (handle, _) = app(directory.path()).await;
    let initial = MoveRankSettings {
        mode: MoveRankEvaluationMode::Combined,
        ..MoveRankSettings::default()
    };
    within(handle.set_move_rank_settings(initial.clone()))
        .await
        .unwrap();
    let path = directory.path().join("settings.json");
    let saved = std::fs::read(&path).unwrap();
    for bad in [
        MoveRankSettings {
            winrate_thresholds: [0.0, 3.0, 6.0, 12.0, 24.0],
            ..initial.clone()
        },
        MoveRankSettings {
            score_thresholds: [0.5, 1.5, 1.5, 6.0, 12.0],
            ..initial.clone()
        },
        MoveRankSettings {
            score_thresholds: [0.5, 1.5, 3.0, 6.0, f64::NAN],
            ..initial.clone()
        },
    ] {
        assert!(within(handle.set_move_rank_settings(bad)).await.is_err());
        assert_eq!(within(handle.settings()).await.unwrap().move_rank, initial);
        assert_eq!(std::fs::read(&path).unwrap(), saved);
    }
    // A directory at the exact destination reliably fails atomic replacement
    // on all test platforms, including privileged CI users.
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("keep"), b"existing contents").unwrap();
    let attempted = MoveRankSettings {
        mode: MoveRankEvaluationMode::Winrate,
        ..initial.clone()
    };
    assert!(
        within(handle.set_move_rank_settings(attempted))
            .await
            .is_err()
    );
    assert_eq!(within(handle.settings()).await.unwrap().move_rank, initial);
    assert_eq!(
        within(handle.problem_moves(all_moves()))
            .await
            .unwrap()
            .settings,
        initial
    );
    assert_eq!(
        std::fs::read(path.join("keep")).unwrap(),
        b"existing contents"
    );
    within(handle.shutdown()).await.unwrap();
    std::fs::remove_file(path.join("keep")).unwrap();
    std::fs::remove_dir(&path).unwrap();
    std::fs::write(&path, &saved).unwrap();
    assert_eq!(
        Settings::load(directory.path()).await.unwrap().move_rank,
        initial
    );
}

#[tokio::test]
async fn current_variation_and_document_namespace_control_problem_move_results() {
    let directory = tempfile::tempdir().unwrap();
    let source = b"(;SZ[9];B[](;W[];B[])(;W[bb];B[cc]))";
    let (handle, mut output, initial, _) = reviewed(directory.path(), source.to_vec()).await;
    let main = within(handle.problem_moves(all_moves())).await.unwrap();
    assert_eq!(
        main.error_bar
            .iter()
            .filter(|point| point.loss.is_some())
            .count(),
        3
    );
    let tree = within(handle.variation_tree()).await.unwrap();
    let fork = tree
        .nodes
        .iter()
        .find(|node| node.children.len() == 2)
        .unwrap();
    let alternate = fork.children[1];
    let revision = within(handle.go_to(alternate)).await.unwrap();
    let alternate_document = document(&mut output, revision).await;
    let branch = within(handle.problem_moves(all_moves())).await.unwrap();
    assert_eq!(
        branch
            .error_bar
            .iter()
            .map(|point| point.node_id)
            .collect::<Vec<_>>(),
        alternate_document
            .graph
            .points
            .iter()
            .map(|point| point.node_id)
            .collect::<Vec<_>>()
    );
    assert_eq!(branch.current_index, alternate_document.graph.current_index);
    assert_eq!(
        branch
            .error_bar
            .iter()
            .filter(|point| point.loss.is_some())
            .count(),
        1
    );
    assert!(
        branch
            .error_bar
            .iter()
            .any(|point| point.node_id == alternate && point.loss.is_none())
    );

    within(handle.apply(document::Command::GoTo(NodeId(initial.tree.root))))
        .await
        .unwrap();
    assert_eq!(
        within(handle.problem_moves(all_moves()))
            .await
            .unwrap()
            .entries,
        main.entries
    );
    assert!(
        within(handle.open_bytes(b"(;C[unterminated".to_vec()))
            .await
            .is_err()
    );
    assert_eq!(
        within(handle.problem_moves(all_moves()))
            .await
            .unwrap()
            .entries,
        main.entries
    );
    within(handle.configure_engine(None)).await.unwrap();
    let cleared = within(handle.problem_moves(all_moves())).await.unwrap();
    assert!(cleared.entries.is_empty());
    assert!(cleared.error_bar.iter().all(|point| point.loss.is_none()));
    within(handle.open_bytes(b"(;SZ[9];B[])".to_vec()))
        .await
        .unwrap();
    let reopened = within(handle.problem_moves(all_moves())).await.unwrap();
    assert_eq!(reopened.error_bar.len(), 2);
    assert!(reopened.error_bar.iter().all(|point| point.loss.is_none()));
    within(handle.shutdown()).await.unwrap();
}
