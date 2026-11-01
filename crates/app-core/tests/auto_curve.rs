#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{
    AnalysisSource, AppConfig, AppCore, AppError, AppEvent, AppSubscription, DocumentView,
    EngineProfile, EngineStatus, Settings, WholeGameOptions, WholeGameProgress, WholeGameSource,
    WholeGameStatus,
    fox::{FoxClient, FoxEndpoints},
};
use serde_json::Value;
use std::{future::Future, path::Path, time::Duration};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("auto-curve operation exceeded safety deadline")
}

fn config() -> AppConfig {
    let mut config = AppConfig::default();
    config.analysis.live.report_interval = Duration::from_millis(10);
    config.analysis.cache_capacity = 1;
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

fn profile(directory: &Path, speed: &str) -> EngineProfile {
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
            speed.into(),
            "--request-log".into(),
            directory.join("requests.jsonl").to_str().unwrap().into(),
        ],
        canonical_model_name: None,
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

fn requests(directory: &Path) -> Vec<Value> {
    std::fs::read_to_string(directory.join("requests.jsonl"))
        .unwrap_or_default()
        .lines()
        // A concurrent writer may not yet have finished its final line.
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn reviews(directory: &Path) -> Vec<Value> {
    requests(directory)
        .into_iter()
        .filter(|query| query["priority"].as_i64().is_some_and(|p| p < 0))
        .collect()
}

async fn logged(directory: &Path, predicate: impl Fn(&Value) -> bool) -> Value {
    within(async {
        loop {
            if let Some(request) = requests(directory).into_iter().find(&predicate) {
                return request;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
}

async fn document(events: &mut AppSubscription, revision: u64) -> DocumentView {
    within(async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::DocumentChanged {
                    revision: actual,
                    view,
                } if actual == revision => return view,
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("unexpected engine failure: {reason}");
                }
                _ => {}
            }
        }
    })
    .await
}

async fn progress(app: &AppCore, source: WholeGameSource) -> WholeGameProgress {
    within(async {
        loop {
            if let Some(progress) = app.whole_game_progress().await.unwrap()
                && progress.source == source
                && progress.status != WholeGameStatus::Cancelled
            {
                return progress;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
}

async fn completed(app: &AppCore, source: WholeGameSource) -> WholeGameProgress {
    within(async {
        loop {
            if let Some(progress) = app.whole_game_progress().await.unwrap()
                && progress.source == source
            {
                match progress.status {
                    WholeGameStatus::Completed => {
                        assert_eq!(progress.completed, progress.total);
                        return progress;
                    }
                    WholeGameStatus::Running => {}
                    other => panic!("unexpected review status: {other:?}"),
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await
}

fn assert_not_automatic(progress: Option<WholeGameProgress>) {
    assert!(progress.is_none_or(|p| {
        p.source != WholeGameSource::AutomaticQuick
            || matches!(
                p.status,
                WholeGameStatus::Cancelled | WholeGameStatus::Completed
            )
    }));
}

#[tokio::test]
async fn opening_starts_one_visit_automatic_review_without_interrupting_live_analysis() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), "200");
    let (app, mut events) = app(directory.path()).await;
    within(app.configure_engine(Some(engine))).await.unwrap();
    logged(directory.path(), |q| q["priority"] == 0).await;
    let before = requests(directory.path()).len();
    let revision = within(app.open_bytes(pass_game(80))).await.unwrap();
    let opened = document(&mut events, revision).await;
    let started = progress(&app, WholeGameSource::AutomaticQuick).await;
    assert_eq!(started.visits, 1);
    assert_eq!(started.total, 81);
    assert_eq!(opened.tree.current, opened.tree.root);
    let first_live = within(async {
        loop {
            if let AppEvent::AnalysisUpdated(view) = events.recv().await.unwrap()
                && view.revision == revision
                && view.source == AnalysisSource::Live
                && view.visits > 0
            {
                return view;
            }
        }
    })
    .await;
    completed(&app, WholeGameSource::AutomaticQuick).await;
    within(async {
        loop {
            if let AppEvent::AnalysisUpdated(view) = events.recv().await.unwrap()
                && view.revision == revision
                && view.source == AnalysisSource::Live
                && view.visits > first_live.visits
            {
                assert_eq!(view.node_id, opened.tree.root);
                break;
            }
        }
    })
    .await;
    assert!(within(app.settings()).await.unwrap().pondering);
    let wire = requests(directory.path());
    let live: Vec<_> = wire[before..]
        .iter()
        .filter(|q| q["priority"] == 0)
        .collect();
    assert_eq!(
        live.len(),
        1,
        "automatic review must not restart live search"
    );
    assert!(
        !wire[before..]
            .iter()
            .any(|q| { q["action"] == "terminate" && q["terminateId"] == live[0]["id"] })
    );
    let review = reviews(directory.path());
    assert!(!review.is_empty());
    assert!(review.iter().all(|q| q["maxVisits"] == 1));
    for node in opened.tree.main_line {
        assert!(
            within(app.whole_game_result(node))
                .await
                .unwrap()
                .unwrap()
                .visits
                >= 1
        );
    }
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn disabled_setting_skips_local_file_auto_review() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), "100000");
    let (app, _events) = app(directory.path()).await;
    assert!(
        !within(app.set_auto_quick_analyze_on_load(false))
            .await
            .unwrap()
    );
    within(app.set_pondering(false)).await.unwrap();
    within(app.configure_engine(Some(engine))).await.unwrap();
    let path = directory.path().join("local.sgf");
    std::fs::write(&path, pass_game(4)).unwrap();
    within(app.open_path(path)).await.unwrap();
    assert!(within(app.whole_game_progress()).await.unwrap().is_none());
    within(app.shutdown()).await.unwrap();
    assert!(reviews(directory.path()).is_empty());
}

#[tokio::test]
async fn disabling_running_auto_review_preserves_live_analysis_and_reenabling_waits_for_open() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), "0.001");
    let (app, _events) = app(directory.path()).await;
    within(app.configure_engine(Some(engine))).await.unwrap();
    within(app.open_bytes(pass_game(4))).await.unwrap();
    let automatic = logged(directory.path(), |q| q["maxVisits"] == 1).await;
    let live = requests(directory.path())
        .into_iter()
        .rfind(|q| q["priority"] == 0)
        .unwrap();
    assert_eq!(
        progress(&app, WholeGameSource::AutomaticQuick).await.status,
        WholeGameStatus::Running
    );
    assert!(
        !within(app.set_auto_quick_analyze_on_load(false))
            .await
            .unwrap()
    );
    let cancelled = within(app.whole_game_progress()).await.unwrap().unwrap();
    assert_eq!(cancelled.source, WholeGameSource::AutomaticQuick);
    assert_eq!(cancelled.status, WholeGameStatus::Cancelled);
    logged(directory.path(), |q| {
        q["action"] == "terminate" && q["terminateId"] == automatic["id"]
    })
    .await;
    assert!(within(app.settings()).await.unwrap().pondering);
    assert!(
        !requests(directory.path())
            .iter()
            .any(|q| { q["action"] == "terminate" && q["terminateId"] == live["id"] })
    );

    assert!(
        within(app.set_auto_quick_analyze_on_load(true))
            .await
            .unwrap()
    );
    assert!(within(app.auto_quick_analyze_on_load()).await.unwrap());
    assert_eq!(
        within(app.whole_game_progress()).await.unwrap(),
        Some(cancelled.clone())
    );
    within(app.step(1)).await.unwrap();
    assert_eq!(
        within(app.whole_game_progress()).await.unwrap(),
        Some(cancelled)
    );
    assert_eq!(reviews(directory.path()).len(), 1);

    // Disabling the preference is not explicit user cancellation of this game.
    within(app.open_bytes(pass_game(4))).await.unwrap();
    assert_eq!(
        progress(&app, WholeGameSource::AutomaticQuick).await.status,
        WholeGameStatus::Running
    );
    logged(directory.path(), |q| {
        q["maxVisits"] == 1 && q["id"] != automatic["id"]
    })
    .await;
    assert!(within(app.settings()).await.unwrap().pondering);
    within(app.shutdown()).await.unwrap();
    assert_eq!(reviews(directory.path()).len(), 2);
}

#[tokio::test]
async fn automatic_review_pause_and_resume_preserve_its_source() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), "0.001");
    let (app, _events) = app(directory.path()).await;
    within(app.set_pondering(false)).await.unwrap();
    within(app.configure_engine(Some(engine))).await.unwrap();
    within(app.open_bytes(pass_game(4))).await.unwrap();
    let first = logged(directory.path(), |q| q["maxVisits"] == 1).await;
    let paused = within(app.pause_whole_game_analysis()).await.unwrap();
    assert_eq!(paused.source, WholeGameSource::AutomaticQuick);
    assert_eq!(paused.status, WholeGameStatus::Paused);
    logged(directory.path(), |q| {
        q["action"] == "terminate" && q["terminateId"] == first["id"]
    })
    .await;
    let resumed = within(app.resume_whole_game_analysis()).await.unwrap();
    assert_eq!(resumed.source, WholeGameSource::AutomaticQuick);
    assert_eq!(resumed.status, WholeGameStatus::Running);
    assert_eq!(
        (resumed.visits, resumed.total),
        (paused.visits, paused.total)
    );
    logged(directory.path(), |q| {
        q["maxVisits"] == 1 && q["id"] != first["id"]
    })
    .await;
    let cancelled = within(app.cancel_whole_game_analysis()).await.unwrap();
    assert_eq!(cancelled.source, WholeGameSource::AutomaticQuick);
    assert_eq!(cancelled.status, WholeGameStatus::Cancelled);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn fully_cached_game_reopens_without_another_automatic_query() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), "100000");
    let (app, mut events) = app(directory.path()).await;
    within(app.set_pondering(false)).await.unwrap();
    within(app.configure_engine(Some(engine))).await.unwrap();
    let path = directory.path().join("cached.sgf");
    std::fs::write(&path, pass_game(5)).unwrap();
    within(app.open_path(path)).await.unwrap();
    completed(&app, WholeGameSource::AutomaticQuick).await;
    let before = reviews(directory.path()).len();
    assert!(before > 0);
    let revision = within(app.open_bytes(pass_game(5))).await.unwrap();
    let reopened = document(&mut events, revision).await;
    assert_not_automatic(within(app.whole_game_progress()).await.unwrap());
    for node in reopened.tree.main_line {
        let result = within(app.whole_game_result(node)).await.unwrap().unwrap();
        assert_eq!(result.visits, 1);
        assert_eq!(result.revision, revision);
    }
    within(app.shutdown()).await.unwrap();
    assert_eq!(reviews(directory.path()).len(), before);
}

async fn assert_deep_cache_is_first_after_reopen(pondering: bool) {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), "100000");
    let mut tuning = config();
    // A new live query can never catch up to the retained manual review depth.
    tuning.analysis.live.max_visits = 100;
    let (app, mut events) = within(app_core::spawn_with_config(directory.path(), tuning))
        .await
        .unwrap();
    within(app.set_pondering(false)).await.unwrap();
    within(app.configure_engine(Some(engine))).await.unwrap();
    within(app.open_bytes(pass_game(3))).await.unwrap();
    completed(&app, WholeGameSource::AutomaticQuick).await;
    within(app.start_whole_game_analysis(WholeGameOptions { visits: 500 }))
        .await
        .unwrap();
    completed(&app, WholeGameSource::Manual).await;
    let before = within(app.whole_game_result(0)).await.unwrap().unwrap();
    assert_eq!(before.visits, 500);
    within(app.set_pondering(pondering)).await.unwrap();
    let review_count = reviews(directory.path()).len();
    let revision = within(app.open_bytes(pass_game(3))).await.unwrap();
    let first = within(async {
        let mut document_seen = false;
        loop {
            match events.recv().await.unwrap() {
                AppEvent::DocumentChanged {
                    revision: actual,
                    view,
                } => {
                    assert_eq!(actual, revision);
                    assert!(view.graph.points.iter().all(|point| {
                        point.summary.is_some_and(|summary| summary.visits == 500)
                    }));
                    document_seen = true;
                }
                AppEvent::AnalysisUpdated(view) => {
                    assert!(
                        document_seen,
                        "analysis must follow the replacement document"
                    );
                    return view;
                }
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("unexpected engine failure: {reason}");
                }
                _ => {}
            }
        }
    })
    .await;
    assert_eq!(first.revision, revision);
    assert_eq!(first.node_id, before.node_id);
    assert_eq!(first.source, AnalysisSource::Cache);
    assert_eq!(first.visits, 500);
    assert_eq!(first.summary.visits, 500);
    assert_eq!(first.candidates, before.candidates);
    assert_eq!(within(app.settings()).await.unwrap().pondering, pondering);
    assert_eq!(
        within(app.whole_game_result(0))
            .await
            .unwrap()
            .unwrap()
            .visits,
        500
    );
    within(app.shutdown()).await.unwrap();
    assert_eq!(reviews(directory.path()).len(), review_count);
}

#[tokio::test]
async fn same_game_reopen_publishes_deep_cache_first_when_live_analysis_is_paused() {
    assert_deep_cache_is_first_after_reopen(false).await;
}

#[tokio::test]
async fn same_game_reopen_publishes_deep_cache_before_a_shallower_live_query() {
    assert_deep_cache_is_first_after_reopen(true).await;
}

#[tokio::test]
async fn cancellation_survives_same_game_reopen_and_failed_open_until_a_different_game() {
    let directory = TempDir::new().unwrap();
    // One visit takes 1000 seconds; cancellation follows an observed wire request.
    let engine = profile(directory.path(), "0.001");
    let (app, _events) = app(directory.path()).await;
    within(app.set_pondering(false)).await.unwrap();
    within(app.configure_engine(Some(engine))).await.unwrap();
    within(app.open_bytes(pass_game(4))).await.unwrap();
    let first = logged(directory.path(), |q| q["maxVisits"] == 1).await;
    let cancelled = within(app.cancel_whole_game_analysis()).await.unwrap();
    assert_eq!(cancelled.source, WholeGameSource::AutomaticQuick);
    assert_eq!(cancelled.status, WholeGameStatus::Cancelled);
    logged(directory.path(), |q| {
        q["action"] == "terminate" && q["terminateId"] == first["id"]
    })
    .await;
    let before = reviews(directory.path()).len();
    assert!(
        within(app.open_bytes(b"(;C[unterminated".to_vec()))
            .await
            .is_err()
    );
    within(app.step(1)).await.unwrap();
    within(app.open_bytes(pass_game(4))).await.unwrap();
    assert_not_automatic(within(app.whole_game_progress()).await.unwrap());
    assert_eq!(reviews(directory.path()).len(), before);

    within(app.open_bytes(pass_game(3))).await.unwrap();
    let next = progress(&app, WholeGameSource::AutomaticQuick).await;
    assert_eq!((next.visits, next.total), (1, 4));
    logged(directory.path(), |q| {
        q["maxVisits"] == 1 && q["id"] != first["id"]
    })
    .await;
    within(app.cancel_whole_game_analysis()).await.unwrap();
    within(app.shutdown()).await.unwrap();
    assert_eq!(reviews(directory.path()).len(), before + 1);
}

#[tokio::test]
async fn manual_review_replaces_automatic_request_and_owns_progress() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), "0.001");
    let (app, _events) = app(directory.path()).await;
    within(app.set_pondering(false)).await.unwrap();
    within(app.configure_engine(Some(engine))).await.unwrap();
    within(app.open_bytes(pass_game(3))).await.unwrap();
    let automatic = logged(directory.path(), |q| q["maxVisits"] == 1).await;
    let manual = within(app.start_whole_game_analysis(WholeGameOptions { visits: 500 }))
        .await
        .unwrap();
    assert_eq!(manual.source, WholeGameSource::Manual);
    assert_eq!(manual.visits, 500);
    assert_eq!(manual.status, WholeGameStatus::Running);
    logged(directory.path(), |q| {
        q["action"] == "terminate" && q["terminateId"] == automatic["id"]
    })
    .await;
    logged(directory.path(), |q| q["maxVisits"] == 500).await;
    assert_eq!(
        within(app.whole_game_progress()).await.unwrap(),
        Some(manual)
    );
    within(app.cancel_whole_game_analysis()).await.unwrap();
    within(app.shutdown()).await.unwrap();
    assert_eq!(reviews(directory.path()).len(), 2);
}

#[tokio::test]
async fn deeper_manual_results_replace_one_visit_curve_results() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), "100000");
    let (app, mut events) = app(directory.path()).await;
    within(app.set_pondering(false)).await.unwrap();
    within(app.configure_engine(Some(engine))).await.unwrap();
    let revision = within(app.open_bytes(pass_game(3))).await.unwrap();
    let opened = document(&mut events, revision).await;
    completed(&app, WholeGameSource::AutomaticQuick).await;
    for node in &opened.tree.main_line {
        assert_eq!(
            within(app.whole_game_result(*node))
                .await
                .unwrap()
                .unwrap()
                .visits,
            1
        );
    }
    within(app.start_whole_game_analysis(WholeGameOptions { visits: 500 }))
        .await
        .unwrap();
    completed(&app, WholeGameSource::Manual).await;
    for node in opened.tree.main_line {
        assert_eq!(
            within(app.whole_game_result(node))
                .await
                .unwrap()
                .unwrap()
                .visits,
            500
        );
    }
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn opening_during_running_or_paused_manual_review_does_not_start_auto_review() {
    for paused in [false, true] {
        let directory = TempDir::new().unwrap();
        let engine = profile(directory.path(), "0.001");
        let (app, _events) = app(directory.path()).await;
        within(app.set_pondering(false)).await.unwrap();
        within(app.configure_engine(Some(engine))).await.unwrap();
        within(app.start_whole_game_analysis(WholeGameOptions { visits: 500 }))
            .await
            .unwrap();
        logged(directory.path(), |q| q["maxVisits"] == 500).await;
        if paused {
            assert_eq!(
                within(app.pause_whole_game_analysis())
                    .await
                    .unwrap()
                    .status,
                WholeGameStatus::Paused
            );
        }
        within(app.open_bytes(pass_game(3))).await.unwrap();
        assert_not_automatic(within(app.whole_game_progress()).await.unwrap());
        within(app.shutdown()).await.unwrap();
        assert!(
            reviews(directory.path())
                .iter()
                .all(|q| q["maxVisits"] == 500)
        );
    }
}

#[tokio::test]
async fn opening_before_engine_configuration_only_analyzes_the_latest_game() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), "100000");
    let (app, _events) = app(directory.path()).await;
    within(app.set_pondering(false)).await.unwrap();
    within(app.open_bytes(pass_game(7))).await.unwrap();
    within(app.open_bytes(pass_game(2))).await.unwrap();
    assert!(within(app.whole_game_progress()).await.unwrap().is_none());
    within(app.configure_engine(Some(engine))).await.unwrap();
    let done = completed(&app, WholeGameSource::AutomaticQuick).await;
    assert_eq!((done.visits, done.total), (1, 3));
    within(app.shutdown()).await.unwrap();
    let wire = reviews(directory.path());
    assert_eq!(wire.len(), 1);
    assert_eq!(wire[0]["analyzeTurns"], serde_json::json!([0, 1, 2]));
}

#[tokio::test]
async fn deferred_automatic_review_keeps_main_line_when_the_user_navigates_a_variation() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), "100000");
    let (app, _events) = app(directory.path()).await;
    within(app.set_pondering(false)).await.unwrap();
    let source = b"(;SZ[9];B[aa](;W[bb])(;W[cc];B[dd]))";
    let parsed = sgf::parse(source).unwrap();
    let reference = document::GameDocument::from_sgf(&parsed.games[0]).unwrap();
    let first = reference.node(reference.root()).unwrap().children()[0];
    let main = reference.node(first).unwrap().children()[0];
    let alternate = reference.node(first).unwrap().children()[1];
    within(app.open_bytes(source.to_vec())).await.unwrap();
    within(app.go_to(alternate.0)).await.unwrap();
    within(app.configure_engine(Some(engine))).await.unwrap();
    let done = completed(&app, WholeGameSource::AutomaticQuick).await;
    assert_eq!((done.visits, done.total), (1, 3));
    assert!(
        within(app.whole_game_result(main.0))
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        within(app.whole_game_result(alternate.0))
            .await
            .unwrap()
            .is_none()
    );
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn empty_or_setup_only_games_do_not_start_auto_review() {
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), "100000");
    let (app, _events) = app(directory.path()).await;
    within(app.set_pondering(false)).await.unwrap();
    within(app.configure_engine(Some(engine))).await.unwrap();
    for source in ["(;SZ[9])", "(;SZ[9]AB[aa]AW[ii];C[empty];PL[W])"] {
        within(app.open_bytes(source.as_bytes().to_vec()))
            .await
            .unwrap();
        assert!(within(app.whole_game_progress()).await.unwrap().is_none());
    }
    within(app.shutdown()).await.unwrap();
    assert!(reviews(directory.path()).is_empty());
}

#[tokio::test]
async fn automatic_setting_defaults_on_for_old_files_and_persists_changes() {
    let directory = TempDir::new().unwrap();
    std::fs::write(
        directory.path().join("settings.json"),
        r#"{"version":1,"pondering":false}"#,
    )
    .unwrap();
    let (app, _events) = app(directory.path()).await;
    assert!(within(app.auto_quick_analyze_on_load()).await.unwrap());
    assert!(
        within(app.settings())
            .await
            .unwrap()
            .auto_quick_analyze_on_load
    );
    assert!(
        !within(app.set_auto_quick_analyze_on_load(false))
            .await
            .unwrap()
    );
    assert!(
        !within(Settings::load(directory.path()))
            .await
            .unwrap()
            .auto_quick_analyze_on_load
    );
    within(app.shutdown()).await.unwrap();
    let (app, _events) = within(app_core::spawn(directory.path())).await.unwrap();
    assert!(!within(app.auto_quick_analyze_on_load()).await.unwrap());
    assert!(
        within(app.set_auto_quick_analyze_on_load(true))
            .await
            .unwrap()
    );
    assert!(
        within(Settings::load(directory.path()))
            .await
            .unwrap()
            .auto_quick_analyze_on_load
    );
    assert!(!within(app.settings()).await.unwrap().pondering);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn failed_setting_save_leaves_automatic_setting_unchanged() {
    let directory = TempDir::new().unwrap();
    let (app, _events) = app(directory.path()).await;
    std::fs::create_dir(directory.path().join("settings.json")).unwrap();
    assert!(matches!(
        within(app.set_auto_quick_analyze_on_load(false)).await,
        Err(AppError::Settings(_))
    ));
    assert!(within(app.auto_quick_analyze_on_load()).await.unwrap());
    assert!(
        within(app.settings())
            .await
            .unwrap()
            .auto_quick_analyze_on_load
    );
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn fox_download_uses_the_same_automatic_review_path() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let client = FoxClient::with_endpoints(
        FoxEndpoints {
            user_url: format!("{base}/user"),
            list_url: format!("{base}/games"),
            sgf_urls: vec![format!("{base}/sgf")],
        },
        Duration::from_secs(4),
    )
    .unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            let count = stream.read(&mut buffer).await.unwrap();
            assert!(count > 0);
            request.extend_from_slice(&buffer[..count]);
            if let Some(offset) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&request[..offset]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                if request.len() >= offset + 4 + length {
                    break;
                }
            }
        }
        let body = r#"{"result":0,"chess":"(;SZ[9];B[aa];W[bb])"}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.shutdown().await.unwrap();
    });
    let directory = TempDir::new().unwrap();
    let engine = profile(directory.path(), "100000");
    let mut config = config();
    config.fox_client = Some(client);
    let (app, _events) = within(app_core::spawn_with_config(directory.path(), config))
        .await
        .unwrap();
    within(app.set_pondering(false)).await.unwrap();
    within(app.configure_engine(Some(engine))).await.unwrap();
    within(app.fox_open_game("12345".into())).await.unwrap();
    let done = completed(&app, WholeGameSource::AutomaticQuick).await;
    assert_eq!((done.visits, done.total), (1, 3));
    within(server).await.unwrap();
    within(app.shutdown()).await.unwrap();
    let wire = reviews(directory.path());
    assert_eq!(wire.len(), 1);
    assert_eq!(wire[0]["maxVisits"], 1);
}
