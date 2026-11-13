#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{
    AnalysisSource, AnalysisView, AppConfig, AppCore, AppEvent, AppSubscription, EngineProfile,
    EngineStatus, Settings, WholeGameOptions, WholeGameSource, WholeGameStatus,
};
use std::{future::Future, path::Path, time::Duration};

const JAVA: &[u8] = include_bytes!("../../../fixtures/lz/java-analysis.sgf");
const WITHOUT_ANALYSIS: &[u8] = b"(;SZ[5]KM[7.5];B[aa];W[bb])";

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("saved focus operation exceeded safety deadline")
}

async fn initialize(directory: &Path, cache_enabled: bool) {
    let model = directory.join("model.bin");
    let config = directory.join("analysis.cfg");
    std::fs::write(&model, b"fake model").unwrap();
    std::fs::write(&config, b"numAnalysisThreads = 2").unwrap();
    Settings {
        engine: Some(EngineProfile {
            program: fake_katago::binary_path().unwrap(),
            model,
            config,
            extra_args: vec!["--visits-per-sec".into(), "100000".into()],
            canonical_model_name: None,
        }),
        pondering: false,
        auto_quick_analyze_on_load: true,
        cache_enabled,
        ..Settings::default()
    }
    .save(directory)
    .await
    .unwrap();
}

async fn start(directory: &Path) -> (AppCore, AppSubscription) {
    let mut config = AppConfig::default();
    config.analysis.live.max_visits = 1;
    config.session.startup_timeout = Duration::from_secs(60);
    config.session.write_timeout = Duration::from_secs(60);
    config.session.response_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(60);
    within(app_core::spawn_with_config(directory, config))
        .await
        .unwrap()
}

async fn completed(app: &AppCore) {
    within(async {
        loop {
            if let Some(progress) = app.whole_game_progress().await.unwrap() {
                assert_eq!(progress.source, WholeGameSource::AutomaticQuick);
                assert_eq!(progress.visits, 1);
                match progress.status {
                    WholeGameStatus::Completed => {
                        assert_eq!((progress.completed, progress.total), (3, 3));
                        return;
                    }
                    WholeGameStatus::Running => {}
                    status => panic!("automatic curve failed: {status:?}"),
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
}

fn assert_saved(view: &AnalysisView, revision: u64) {
    assert_eq!(view.node_id, 0);
    assert_eq!(view.revision, revision);
    assert_eq!(view.source, AnalysisSource::Saved);
    assert_eq!(view.visits, 12345);
    assert!((view.winrate - 0.55).abs() < 1e-12);
    assert_eq!(view.score_lead, 2.5);
    assert_eq!(view.summary.visits, 12345);
    assert_eq!(view.candidates.len(), 2);
    assert_eq!(view.candidates[0].coordinate, "C3");
    assert_eq!(view.candidates[0].visits, 50000);
    assert_eq!(view.candidates[1].coordinate, "pass");
    assert_eq!(view.ownership, Some(vec![0.2; 25]));
}

async fn delivered_focus(events: &mut AppSubscription, revision: u64) -> AnalysisView {
    let mut document = None;
    let mut displayed = None;
    // Deliberately leave events unread until the actor has finished restoring
    // caches and generating the curve. A frontend accepts analysis only after
    // the matching document, so merely finding a Saved event is insufficient.
    loop {
        let event = tokio::select! {
            biased;
            event = events.recv() => event,
            () = std::future::ready(()) => None,
        };
        match event {
            Some(AppEvent::DocumentChanged { revision, view }) => {
                let focus = (revision, view.tree.current);
                if displayed.as_ref().is_some_and(|analysis: &AnalysisView| {
                    (analysis.revision, analysis.node_id) != focus
                }) {
                    displayed = None;
                }
                document = Some(focus);
            }
            Some(AppEvent::AnalysisUpdated(view)) => {
                if document == Some((view.revision, view.node_id)) {
                    displayed = Some(view);
                }
            }
            Some(AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. })) => {
                panic!("engine failed: {reason}")
            }
            Some(_) => {}
            None => break,
        }
    }
    assert_eq!(document, Some((revision, 0)));
    displayed.expect("latest document must retain its focus analysis event")
}

async fn assert_delivered_saved_focus(events: &mut AppSubscription, revision: u64) {
    assert_saved(&delivered_focus(events, revision).await, revision);
}

async fn saved_focus_survives_automatic_curve(cache_enabled: bool, seed_cache: bool) {
    let directory = tempfile::tempdir().unwrap();
    initialize(directory.path(), seed_cache || cache_enabled).await;
    if seed_cache {
        // Populate the same position keys with actual one-visit fake engine
        // results, then reopen through a new actor to exercise SQLite recovery.
        let (app, _events) = start(directory.path()).await;
        within(app.open_bytes(WITHOUT_ANALYSIS.to_vec()))
            .await
            .unwrap();
        completed(&app).await;
        assert_eq!(
            within(app.whole_game_result(0))
                .await
                .unwrap()
                .unwrap()
                .visits,
            1
        );
        within(app.shutdown()).await.unwrap();
        let mut settings = Settings::load(directory.path()).await.unwrap();
        settings.cache_enabled = cache_enabled;
        settings.save(directory.path()).await.unwrap();
    }
    let (app, mut events) = start(directory.path()).await;
    let revision = within(app.open_bytes(JAVA.to_vec())).await.unwrap();
    completed(&app).await;
    assert_delivered_saved_focus(&mut events, revision).await;
    assert_saved(
        &within(app.whole_game_result(0)).await.unwrap().unwrap(),
        revision,
    );
    assert!(!within(app.settings()).await.unwrap().pondering);
    // Revisiting the saved node must also survive the focus cache lookup.
    within(app.go_to(2)).await.unwrap();
    let revision = within(app.go_to(0)).await.unwrap();
    assert_delivered_saved_focus(&mut events, revision).await;
    // Pausing clears unread engine frames. It must still republish the file's
    // result when the resumed one-visit search cannot improve its depth.
    within(app.set_pondering(true)).await.unwrap();
    within(app.set_pondering(false)).await.unwrap();
    let paused = within(async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::AnalysisUpdated(view) => break view,
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("engine failed: {reason}")
                }
                _ => {}
            }
        }
    })
    .await;
    assert_saved(&paused, revision);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn paused_open_keeps_saved_focus_after_automatic_curve_with_cache_enabled() {
    saved_focus_survives_automatic_curve(true, false).await;
}

#[tokio::test]
async fn paused_open_keeps_saved_focus_after_automatic_curve_with_cache_disabled() {
    saved_focus_survives_automatic_curve(false, false).await;
}

#[tokio::test]
async fn restored_one_visit_cache_cannot_replace_saved_focus() {
    saved_focus_survives_automatic_curve(true, true).await;
}

#[tokio::test]
async fn disabled_cache_keeps_saved_focus_even_when_database_contains_one_visit_results() {
    saved_focus_survives_automatic_curve(false, true).await;
}

#[tokio::test]
async fn deeper_manual_review_replaces_saved_focus_after_the_latest_document() {
    let directory = tempfile::tempdir().unwrap();
    initialize(directory.path(), true).await;
    let (app, mut events) = start(directory.path()).await;
    let revision = within(app.open_bytes(JAVA.to_vec())).await.unwrap();
    completed(&app).await;
    let visits = 12346;
    within(app.start_whole_game_analysis(WholeGameOptions { visits }))
        .await
        .unwrap();
    within(async {
        loop {
            let progress = app.whole_game_progress().await.unwrap().unwrap();
            assert_eq!(progress.source, WholeGameSource::Manual);
            assert_eq!(progress.visits, visits);
            match progress.status {
                WholeGameStatus::Completed => {
                    assert_eq!((progress.completed, progress.total), (3, 3));
                    break;
                }
                WholeGameStatus::Running => {}
                status => panic!("manual review failed: {status:?}"),
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    // Slow subscribers must receive the replacement after the latest graph
    // document, not lose it while the remaining review turns finish.
    let focus = delivered_focus(&mut events, revision).await;
    assert_eq!(focus.node_id, 0);
    assert_eq!(focus.source, AnalysisSource::Cache);
    assert_eq!(focus.visits, visits);
    assert_eq!(focus.summary.visits, visits);
    assert!(!focus.candidates.is_empty());
    let stored = within(app.whole_game_analysis(0)).await.unwrap().unwrap();
    assert!(stored.saved.is_none());
    assert_eq!(stored.response.root_info.visits, visits);
    assert!(!within(app.settings()).await.unwrap().pondering);
    within(app.shutdown()).await.unwrap();
}
