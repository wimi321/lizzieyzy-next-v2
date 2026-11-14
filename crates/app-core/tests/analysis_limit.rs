#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use app_core::{
    AnalysisLimitSettings, AnalysisPauseReason, AnalysisSource, AnalysisState, AnalysisView,
    AppConfig, AppCore, AppEvent, AppSubscription, EngineProfile, EngineStatus, Settings,
    WholeGameOptions, WholeGameSource, WholeGameStatus,
};
use common::within;
use document::{Command, GameDocument};
use go_core::{Color, Move, Point};
use serde_json::Value;
use std::{path::Path, time::Duration};
use tempfile::TempDir;

fn config() -> AppConfig {
    let mut config = AppConfig::default();
    config.analysis.live.report_interval = Duration::from_millis(10);
    // Faults must be explicit, not side effects of a loaded test runner.
    config.session.startup_timeout = Duration::from_secs(60);
    config.session.response_timeout = Duration::from_secs(60);
    config.session.write_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(60);
    config
}

async fn app(directory: &Path) -> (AppCore, AppSubscription) {
    within(app_core::spawn_with_config(directory, config()))
        .await
        .unwrap()
}

fn profile(directory: &Path, speed: u64) -> EngineProfile {
    let program = fake_katago::binary_path().unwrap();
    let model = directory.join("model.bin");
    let config = directory.join("analysis.cfg");
    std::fs::write(&model, "fake model").unwrap();
    std::fs::write(&config, "numAnalysisThreads = 1\n").unwrap();
    EngineProfile {
        program,
        model,
        config,
        extra_args: vec![
            "--visits-per-sec".into(),
            speed.to_string(),
            "--request-log".into(),
            directory.join("requests.jsonl").to_str().unwrap().into(),
        ],
        canonical_model_name: None,
    }
}

fn visit_limit() -> AnalysisLimitSettings {
    AnalysisLimitSettings {
        limit_time: false,
        max_analyze_time_seconds: 600,
        limit_playout: true,
        limit_playouts: 1,
    }
}

async fn state(app: &AppCore) -> AnalysisState {
    within(app.analysis_state()).await.unwrap()
}

async fn assert_paused(app: &AppCore, reason: AnalysisPauseReason) {
    let current = state(app).await;
    assert!(!current.pondering, "unexpected running state: {current:?}");
    assert_eq!(current.pause_reason, Some(reason));
}

async fn stopped(events: &mut AppSubscription, reason: AnalysisPauseReason) {
    within(async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::AnalysisStateChanged(state) if state.pause_reason == Some(reason) => {
                    assert!(!state.pondering);
                    return;
                }
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("unexpected engine failure: {reason}");
                }
                _ => {}
            }
        }
    })
    .await;
}

async fn live(events: &mut AppSubscription, minimum_visits: u64) -> AnalysisView {
    within(async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::AnalysisUpdated(view)
                    if view.source == AnalysisSource::Live && view.visits >= minimum_visits =>
                {
                    return view;
                }
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("unexpected engine failure: {reason}");
                }
                _ => {}
            }
        }
    })
    .await
}

async fn retained(app: &AppCore, node: u32, minimum_visits: u64) -> AnalysisView {
    // Limit status and the trailing throttled snapshot are separate deliveries.
    // A pause notification does not promise the snapshot was already retained.
    within(async {
        loop {
            if let Some(result) = app.whole_game_result(node).await.unwrap()
                && result.visits >= minimum_visits
            {
                return result;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
}

fn requests(directory: &Path) -> Vec<Value> {
    std::fs::read_to_string(directory.join("requests.jsonl"))
        .unwrap_or_default()
        .lines()
        // A concurrent writer may not yet have finished its final line.
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn live_count(directory: &Path) -> usize {
    requests(directory)
        .iter()
        .filter(|request| request["priority"] == 0)
        .count()
}

async fn new_live_request(directory: &Path, previous: usize) {
    within(async {
        while live_count(directory) <= previous {
            tokio::task::yield_now().await;
        }
    })
    .await;
}

async fn completed(app: &AppCore, source: WholeGameSource) {
    within(async {
        loop {
            if let Some(progress) = app.whole_game_progress().await.unwrap() {
                assert_eq!(progress.source, source);
                match progress.status {
                    WholeGameStatus::Completed => {
                        assert_eq!(progress.completed, progress.total);
                        return;
                    }
                    WholeGameStatus::Running => {}
                    other => panic!("unexpected review state: {other:?}"),
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
}

#[tokio::test]
async fn old_settings_use_java_defaults_and_new_limits_survive_restart() {
    let directory = TempDir::new().unwrap();
    std::fs::write(
        directory.path().join("settings.json"),
        r#"{"version":1,"pondering":false}"#,
    )
    .unwrap();
    let (app, _events) = app(directory.path()).await;
    let defaults = within(app.analysis_limits()).await.unwrap();
    assert!(defaults.limit_time);
    assert_eq!(defaults.max_analyze_time_seconds, 600);
    assert!(!defaults.limit_playout);
    assert_eq!(defaults.limit_playouts, 100_000);
    assert_eq!(
        within(app.settings()).await.unwrap().analysis_limits,
        defaults
    );
    assert_paused(&app, AnalysisPauseReason::User).await;

    let changed = AnalysisLimitSettings {
        limit_time: false,
        max_analyze_time_seconds: 37,
        limit_playout: true,
        limit_playouts: 12_345,
    };
    within(app.set_analysis_limits(changed)).await.unwrap();
    assert_eq!(within(app.analysis_limits()).await.unwrap(), changed);
    assert_eq!(
        within(Settings::load(directory.path()))
            .await
            .unwrap()
            .analysis_limits,
        changed
    );
    within(app.shutdown()).await.unwrap();
    let (reopened, _events) = within(app_core::spawn(directory.path())).await.unwrap();
    assert_eq!(within(reopened.analysis_limits()).await.unwrap(), changed);
    assert_paused(&reopened, AnalysisPauseReason::User).await;
    within(reopened.shutdown()).await.unwrap();
}

#[tokio::test]
async fn invalid_limits_and_failed_save_preserve_previous_settings() {
    let directory = TempDir::new().unwrap();
    let (app, _events) = app(directory.path()).await;
    let original = within(app.analysis_limits()).await.unwrap();
    for invalid in [
        AnalysisLimitSettings {
            max_analyze_time_seconds: 0,
            ..original
        },
        AnalysisLimitSettings {
            limit_playouts: 0,
            ..original
        },
        AnalysisLimitSettings {
            limit_playouts: 9_007_199_254_740_992,
            ..original
        },
    ] {
        assert!(within(app.set_analysis_limits(invalid)).await.is_err());
        assert_eq!(within(app.analysis_limits()).await.unwrap(), original);
    }
    std::fs::create_dir(directory.path().join("settings.json")).unwrap();
    assert!(
        within(app.set_analysis_limits(visit_limit()))
            .await
            .is_err()
    );
    assert_eq!(within(app.analysis_limits()).await.unwrap(), original);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn root_visit_limit_preserves_result_and_persisted_analysis_intent() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), 10_000);
    let (app, mut events) = app(directory.path()).await;
    within(app.set_analysis_limits(visit_limit()))
        .await
        .unwrap();
    within(app.configure_engine(Some(engine))).await.unwrap();
    stopped(&mut events, AnalysisPauseReason::PlayoutLimit).await;
    assert_paused(&app, AnalysisPauseReason::PlayoutLimit).await;
    let result = retained(&app, 0, 2).await;
    assert!(result.visits > 1);
    assert!(!result.candidates.is_empty());
    assert!(within(app.settings()).await.unwrap().pondering);
    assert!(
        within(Settings::load(directory.path()))
            .await
            .unwrap()
            .pondering
    );
    let old_queries = live_count(directory.path());
    within(app.shutdown()).await.unwrap();

    let (reopened, mut events) = within(app_core::spawn_with_config(directory.path(), config()))
        .await
        .unwrap();
    new_live_request(directory.path(), old_queries).await;
    stopped(&mut events, AnalysisPauseReason::PlayoutLimit).await;
    assert!(within(reopened.settings()).await.unwrap().pondering);
    within(reopened.shutdown()).await.unwrap();
}

#[tokio::test]
async fn toggle_and_explicit_enable_resume_limit_pause_without_saving_a_false_preference() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), 10_000);
    let (app, mut events) = app(directory.path()).await;
    within(app.set_analysis_limits(visit_limit()))
        .await
        .unwrap();
    within(app.configure_engine(Some(engine))).await.unwrap();
    stopped(&mut events, AnalysisPauseReason::PlayoutLimit).await;
    let result = retained(&app, 0, 2).await;
    within(app.toggle_pondering()).await.unwrap();
    live(&mut events, result.visits.saturating_add(100)).await;
    assert!(state(&app).await.pondering);
    assert_eq!(state(&app).await.pause_reason, None);
    assert!(
        within(Settings::load(directory.path()))
            .await
            .unwrap()
            .pondering
    );

    // A move starts a fresh visit budget after the same-position override.
    within(app.play(Move::Play(Color::Black, Point { x: 0, y: 0 })))
        .await
        .unwrap();
    stopped(&mut events, AnalysisPauseReason::PlayoutLimit).await;
    let saved = directory.path().join("settings.json");
    std::fs::remove_file(&saved).unwrap();
    std::fs::create_dir(&saved).unwrap();
    // Saved pondering is already true: resuming must not attempt any settings write.
    within(app.set_pondering(true)).await.unwrap();
    live(&mut events, 1_000).await;
    assert!(state(&app).await.pondering);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn forward_moves_resume_but_backward_empty_comment_and_failed_moves_do_not() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), 10_000);
    let (app, mut events) = app(directory.path()).await;
    within(app.set_auto_quick_analyze_on_load(false))
        .await
        .unwrap();
    let source = b"(;SZ[9](;B[aa];C[empty];W[bb])(;B[cc]))";
    let parsed = sgf::parse(source).unwrap();
    let reference = GameDocument::from_sgf(&parsed.games[0]).unwrap();
    let root = reference.root();
    let first = reference.node(root).unwrap().children()[0];
    let sibling = reference.node(root).unwrap().children()[1];
    let empty = reference.node(first).unwrap().children()[0];
    within(app.open_bytes(source.to_vec())).await.unwrap();
    within(app.set_analysis_limits(visit_limit()))
        .await
        .unwrap();
    within(app.configure_engine(Some(engine))).await.unwrap();
    stopped(&mut events, AnalysisPauseReason::PlayoutLimit).await;

    assert!(within(app.play(Move::Pass(Color::White))).await.is_err());
    within(app.apply(Command::SetComment("still paused".into())))
        .await
        .unwrap();
    assert_paused(&app, AnalysisPauseReason::PlayoutLimit).await;
    let previous = live_count(directory.path());
    within(app.step(1)).await.unwrap();
    new_live_request(directory.path(), previous).await;
    stopped(&mut events, AnalysisPauseReason::PlayoutLimit).await;

    within(app.step(-1)).await.unwrap();
    assert_paused(&app, AnalysisPauseReason::PlayoutLimit).await;
    let previous = live_count(directory.path());
    within(app.go_to(sibling.0)).await.unwrap();
    new_live_request(directory.path(), previous).await;
    stopped(&mut events, AnalysisPauseReason::PlayoutLimit).await;

    // Sibling navigation crosses the common ancestor and plays a different move.
    let previous = live_count(directory.path());
    within(app.go_to(first.0)).await.unwrap();
    new_live_request(directory.path(), previous).await;
    stopped(&mut events, AnalysisPauseReason::PlayoutLimit).await;
    within(app.go_to(empty.0)).await.unwrap();
    assert_paused(&app, AnalysisPauseReason::PlayoutLimit).await;
    let previous = live_count(directory.path());
    within(app.step(1)).await.unwrap();
    new_live_request(directory.path(), previous).await;
    stopped(&mut events, AnalysisPauseReason::PlayoutLimit).await;
    within(app.go_to(root.0)).await.unwrap();
    assert_paused(&app, AnalysisPauseReason::PlayoutLimit).await;
    let previous = live_count(directory.path());
    within(app.play(Move::Pass(Color::Black))).await.unwrap();
    new_live_request(directory.path(), previous).await;
    stopped(&mut events, AnalysisPauseReason::PlayoutLimit).await;
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn user_pause_is_distinct_and_playing_does_not_resume_it() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), 10_000);
    let (app, mut events) = app(directory.path()).await;
    within(app.configure_engine(Some(engine))).await.unwrap();
    live(&mut events, 1).await;
    within(app.set_pondering(false)).await.unwrap();
    assert_paused(&app, AnalysisPauseReason::User).await;
    within(app.play(Move::Pass(Color::Black))).await.unwrap();
    assert_paused(&app, AnalysisPauseReason::User).await;
    assert!(
        !within(Settings::load(directory.path()))
            .await
            .unwrap()
            .pondering
    );
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn manual_and_automatic_whole_game_analysis_complete_despite_live_limit_pause() {
    for automatic in [false, true] {
        let directory = TempDir::new().unwrap();
        // Automatic review needs to remain active when the live limit fires.
        // At 20 visits/s the short review still spans multiple live reports.
        let engine = profile(directory.path(), if automatic { 20 } else { 10_000 });
        let (app, mut events) = app(directory.path()).await;
        within(app.set_auto_quick_analyze_on_load(automatic))
            .await
            .unwrap();
        within(app.set_analysis_limits(visit_limit()))
            .await
            .unwrap();
        // Eight moves exercise automatic completion past the live limit without
        // making the safety deadline depend on an 80-move replay.
        let moves = if automatic { 8 } else { 3 };
        let mut source = String::from("(;SZ[9]");
        for index in 0..moves {
            source.push_str(if index % 2 == 0 { ";B[]" } else { ";W[]" });
        }
        source.push(')');
        within(app.open_bytes(source.into_bytes())).await.unwrap();
        within(app.configure_engine(Some(engine))).await.unwrap();
        stopped(&mut events, AnalysisPauseReason::PlayoutLimit).await;
        let expected_source = if automatic {
            assert_eq!(
                within(app.whole_game_progress())
                    .await
                    .unwrap()
                    .unwrap()
                    .status,
                WholeGameStatus::Running,
                "automatic review must still be running when live analysis is limited"
            );
            WholeGameSource::AutomaticQuick
        } else {
            within(app.start_whole_game_analysis(WholeGameOptions {
                visits: 500,
                ..Default::default()
            }))
            .await
            .unwrap();
            WholeGameSource::Manual
        };
        completed(&app, expected_source).await;
        assert_paused(&app, AnalysisPauseReason::PlayoutLimit).await;
        assert!(within(app.settings()).await.unwrap().pondering);
        for node in within(app.variation_tree()).await.unwrap().nodes {
            let retained = within(app.whole_game_result(node.id))
                .await
                .unwrap()
                .unwrap();
            assert!(retained.visits >= if automatic { 1 } else { 500 });
        }
        within(app.shutdown()).await.unwrap();
    }
}

#[tokio::test]
async fn disabled_limits_allow_a_live_request_to_finish_above_visit_threshold() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), 10_000);
    let mut config = config();
    config.analysis.live.max_visits = 1_000;
    let (app, mut events) = within(app_core::spawn_with_config(directory.path(), config))
        .await
        .unwrap();
    within(app.set_analysis_limits(AnalysisLimitSettings {
        limit_time: false,
        max_analyze_time_seconds: 1,
        limit_playout: false,
        limit_playouts: 1,
    }))
    .await
    .unwrap();
    within(app.configure_engine(Some(engine))).await.unwrap();
    let finished = live(&mut events, 1_000).await;
    assert!(!finished.is_during_search);
    assert_eq!(state(&app).await.pause_reason, None);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn time_limit_uses_tokio_time_and_keeps_saved_analysis_enabled() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), 10_000);
    let (app, mut events) = app(directory.path()).await;
    within(app.set_analysis_limits(AnalysisLimitSettings {
        limit_time: true,
        max_analyze_time_seconds: 1,
        limit_playout: false,
        limit_playouts: 100_000,
    }))
    .await
    .unwrap();
    within(app.configure_engine(Some(engine))).await.unwrap();
    live(&mut events, 1).await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(2)).await;
    tokio::time::resume();
    stopped(&mut events, AnalysisPauseReason::TimeLimit).await;
    assert_paused(&app, AnalysisPauseReason::TimeLimit).await;
    assert!(
        within(Settings::load(directory.path()))
            .await
            .unwrap()
            .pondering
    );
    assert!(within(app.whole_game_result(0)).await.unwrap().is_some());
    within(app.shutdown()).await.unwrap();
}
