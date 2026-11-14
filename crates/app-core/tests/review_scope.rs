#![allow(clippy::unwrap_used, clippy::expect_used)]
use app_core::{
    AppCore, AppEvent, AppSubscription, EngineProfile, EngineStatus, Settings, WholeGameOptions,
    WholeGameProgress, WholeGameScope, WholeGameStatus,
};
use std::{future::Future, path::Path, time::Duration};

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("review safety deadline")
}

async fn setup(directory: &Path) -> (AppCore, AppSubscription) {
    let program = fake_katago::binary_path().unwrap();
    Settings {
        pondering: false,
        cache_enabled: false,
        auto_quick_analyze_on_load: false,
        ..Default::default()
    }
    .save(directory)
    .await
    .unwrap();
    let mut config = app_core::AppConfig::default();
    config.session.startup_timeout = Duration::from_secs(60);
    config.session.response_timeout = Duration::from_secs(60);
    let (app, mut events) = app_core::spawn_with_config(directory, config)
        .await
        .unwrap();
    app.configure_engine(Some(EngineProfile {
        program: program.clone(),
        model: program.clone(),
        config: program,
        ..Default::default()
    }))
    .await
    .unwrap();
    within(async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::EngineStatusChanged(EngineStatus::Ready { .. }) => break,
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("{reason}")
                }
                _ => {}
            }
        }
    })
    .await;
    (app, events)
}

async fn run(
    app: &AppCore,
    events: &mut AppSubscription,
    options: WholeGameOptions,
) -> WholeGameProgress {
    let start = app.start_whole_game_analysis(options).await.unwrap();
    if start.status == WholeGameStatus::Completed {
        return start;
    }
    within(async {
        loop {
            if let AppEvent::WholeGameProgress(progress) = events.recv().await.unwrap() {
                match progress.status {
                    WholeGameStatus::Completed => return progress,
                    WholeGameStatus::Error { ref reason } => panic!("{reason}"),
                    _ => {}
                }
            }
        }
    })
    .await
}

#[tokio::test]
async fn range_endpoints_include_the_before_position_with_fake_engine() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = setup(directory.path()).await;
    for (first_move, last_move, expected) in [
        (None, None, vec![0, 1, 2, 3]),
        (Some(2), None, vec![1, 2, 3]),
        (None, Some(2), vec![0, 1, 2]),
        (Some(2), Some(2), vec![1, 2]),
    ] {
        app.open_bytes(b"(;SZ[9];B[];W[];B[])".to_vec())
            .await
            .unwrap();
        let progress = run(
            &app,
            &mut events,
            WholeGameOptions {
                visits: 2,
                scope: WholeGameScope::Range {
                    first_move,
                    last_move,
                },
                force: false,
            },
        )
        .await;
        assert_eq!(
            (progress.completed, progress.total),
            (expected.len(), expected.len())
        );
        let mut analyzed = Vec::new();
        for node in app.variation_tree().await.unwrap().nodes {
            if app.whole_game_analysis(node.id).await.unwrap().is_some() {
                analyzed.push(node);
            }
        }
        assert_eq!(
            analyzed.iter().map(|node| node.id).collect::<Vec<_>>(),
            expected
        );
        assert_eq!(
            analyzed.first().unwrap().move_number,
            first_move.unwrap_or(1) - 1
        );
        if first_move.unwrap_or(1) == 1 {
            assert_eq!(analyzed.first().unwrap().id, 0);
        }
    }
    // Range validation must leave the completed review and its results alone.
    let before = app.whole_game_progress().await.unwrap();
    for (first_move, last_move) in [
        (Some(0), None),
        (Some(4), None),
        (None, Some(4)),
        (Some(3), Some(2)),
    ] {
        assert!(matches!(
            app.start_whole_game_analysis(WholeGameOptions {
                visits: 2,
                scope: WholeGameScope::Range {
                    first_move,
                    last_move
                },
                force: false,
            })
            .await,
            Err(app_core::AppError::WholeGame(
                app_core::WholeGameError::InvalidRange { .. }
            ))
        ));
        assert_eq!(app.whole_game_progress().await.unwrap(), before);
    }
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn first_selected_move_has_loss_evaluation_after_range_analysis() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = setup(directory.path()).await;
    for first_move in [1, 2] {
        // The empty node is White's actual parent. Analysing only the preceding
        // move node would still leave White without a before-position result.
        app.open_bytes(b"(;SZ[9];B[aa];C[before white];W[bb];B[cc];W[dd])".to_vec())
            .await
            .unwrap();
        let first_node = if first_move == 1 { 1 } else { 3 };
        assert!(
            app.problem_moves(Default::default())
                .await
                .unwrap()
                .error_bar
                .iter()
                .find(|point| point.node_id == first_node)
                .unwrap()
                .loss
                .is_none()
        );
        let progress = run(
            &app,
            &mut events,
            WholeGameOptions {
                visits: 3,
                scope: WholeGameScope::Range {
                    first_move: Some(first_move),
                    last_move: Some(3),
                },
                force: false,
            },
        )
        .await;
        assert_eq!(progress.total, if first_move == 1 { 5 } else { 3 });
        let problems = app.problem_moves(Default::default()).await.unwrap();
        let first = problems
            .error_bar
            .iter()
            .find(|point| point.node_id == first_node)
            .unwrap();
        let loss = first
            .loss
            .as_ref()
            .expect("the selected first move must have a before/after evaluation");
        assert_eq!(first.move_number, first_move);
        assert_eq!((loss.before_visits, loss.visits), (3, 3));
        assert!(loss.score_loss_available);
        assert!(loss.winrate_loss.is_finite() && loss.score_loss.is_finite());
        let outside = problems
            .error_bar
            .iter()
            .find(|point| point.move_number == 4)
            .unwrap();
        assert!(
            outside.loss.is_none(),
            "the next unselected move is not evaluated"
        );
    }
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn all_branches_fills_every_node_without_changing_main_line_consumers() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = setup(directory.path()).await;
    app.open_bytes(b"(;SZ[9];B[](;W[];B[])(;W[aa];B[bb](;W[cc])(;W[dd])))".to_vec())
        .await
        .unwrap();
    run(
        &app,
        &mut events,
        WholeGameOptions {
            visits: 2,
            ..Default::default()
        },
    )
    .await;
    let problems = app.problem_moves(Default::default()).await.unwrap();
    let strength = app.strength_estimate(None).await.unwrap();
    assert_eq!(problems.error_bar.len(), 4);
    assert_eq!(strength.total_moves, 3);
    let progress = run(
        &app,
        &mut events,
        WholeGameOptions {
            visits: 2,
            scope: WholeGameScope::AllBranches,
            force: false,
        },
    )
    .await;
    assert_eq!((progress.total, progress.completed), (8, 8));
    let tree = app.variation_tree().await.unwrap();
    for node in tree.nodes {
        assert_eq!(
            app.whole_game_analysis(node.id)
                .await
                .unwrap()
                .unwrap()
                .response
                .root_info
                .visits,
            2
        );
    }
    assert_eq!(
        app.problem_moves(Default::default()).await.unwrap(),
        problems
    );
    assert_eq!(app.strength_estimate(None).await.unwrap(), strength);
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn force_replaces_deeper_memory_and_saved_results_with_fresh_analysis() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = setup(directory.path()).await;
    app.open_bytes(b"(;SZ[9];B[];W[])".to_vec()).await.unwrap();
    run(
        &app,
        &mut events,
        WholeGameOptions {
            visits: 11,
            ..Default::default()
        },
    )
    .await;
    let root = app.whole_game_analysis(0).await.unwrap().unwrap();
    let skipped = run(
        &app,
        &mut events,
        WholeGameOptions {
            visits: 3,
            ..Default::default()
        },
    )
    .await;
    assert_eq!(skipped.completed, skipped.total);
    assert_eq!(app.whole_game_analysis(0).await.unwrap().unwrap(), root);
    run(
        &app,
        &mut events,
        WholeGameOptions {
            visits: 3,
            force: true,
            ..Default::default()
        },
    )
    .await;
    for node in 0..=2 {
        let result = app.whole_game_analysis(node).await.unwrap().unwrap();
        assert_eq!(result.response.root_info.visits, 3);
        assert!(result.saved.is_none());
    }
    let path = directory.path().join("saved.sgf");
    app.save(&path).await.unwrap();
    app.open_path(&path).await.unwrap();
    assert!(
        app.whole_game_analysis(0)
            .await
            .unwrap()
            .unwrap()
            .saved
            .is_some()
    );
    run(
        &app,
        &mut events,
        WholeGameOptions {
            visits: 2,
            force: true,
            ..Default::default()
        },
    )
    .await;
    for node in 0..=2 {
        let result = app.whole_game_analysis(node).await.unwrap().unwrap();
        assert_eq!(result.response.root_info.visits, 2);
        assert!(result.saved.is_none());
    }
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn forced_shallower_results_survive_navigation_with_both_caches_enabled() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = setup(directory.path()).await;
    app.set_cache_enabled(true).await.unwrap();
    app.open_bytes(b"(;SZ[9];B[];W[])".to_vec()).await.unwrap();
    run(
        &app,
        &mut events,
        WholeGameOptions {
            visits: 17,
            ..Default::default()
        },
    )
    .await;
    // Populate the live LRU as well as the persistent cache with the old result.
    for node in [1, 0] {
        app.go_to(node).await.unwrap();
        within(async {
            loop {
                if let AppEvent::AnalysisUpdated(view) = events.recv().await.unwrap()
                    && view.node_id == node
                {
                    assert_eq!(view.visits, 17);
                    break;
                }
            }
        })
        .await;
    }
    let started = app
        .start_whole_game_analysis(WholeGameOptions {
            visits: 3,
            force: true,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(started.completed, 0);
    let mut fresh_focus = false;
    within(async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::AnalysisUpdated(view) if view.node_id == 0 && view.visits == 3 => {
                    fresh_focus = true
                }
                AppEvent::WholeGameProgress(progress)
                    if progress.status == WholeGameStatus::Completed =>
                {
                    break;
                }
                AppEvent::WholeGameProgress(progress)
                    if matches!(progress.status, WholeGameStatus::Error { .. }) =>
                {
                    panic!("{:?}", progress.status)
                }
                _ => {}
            }
        }
    })
    .await;
    assert!(
        fresh_focus,
        "forced result must replace the displayed candidates too"
    );
    for node in [1, 0, 2, 0] {
        app.go_to(node).await.unwrap();
        within(async {
            loop {
                if let AppEvent::AnalysisUpdated(view) = events.recv().await.unwrap()
                    && view.node_id == node
                {
                    assert_eq!(view.visits, 3);
                    break;
                }
            }
        })
        .await;
        assert_eq!(
            app.whole_game_analysis(node)
                .await
                .unwrap()
                .unwrap()
                .response
                .root_info
                .visits,
            3
        );
    }
    app.shutdown().await.unwrap();
}
