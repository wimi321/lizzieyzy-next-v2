#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{
    AnalysisSource, AnalysisView, AppConfig, AppCore, AppEvent, AppSubscription, EngineProfile,
    EngineStatus, Settings, WholeGameOptions, WholeGameStatus,
};
use std::{future::Future, path::Path, time::Duration};

const GAME: &[u8] = b"(;SZ[9]KM[6.5];B[aa];W[bb])";

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("safety deadline")
}

fn engine(directory: &Path) -> EngineProfile {
    let model = directory.join("model.bin");
    let config = directory.join("analysis.cfg");
    std::fs::write(&model, b"model one").unwrap();
    std::fs::write(&config, b"numAnalysisThreads = 2").unwrap();
    EngineProfile {
        program: fake_katago::binary_path().unwrap(),
        model,
        config,
        extra_args: vec!["--visits-per-sec".into(), "100000".into()],
        canonical_model_name: None,
    }
}

async fn start(directory: &Path, visits: u64) -> (AppCore, AppSubscription) {
    let mut config = AppConfig::default();
    config.analysis.live.max_visits = visits;
    config.analysis.live.report_interval = Duration::from_secs(60);
    config.session.write_timeout = Duration::from_secs(60);
    config.session.response_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(60);
    within(app_core::spawn_with_config(directory, config))
        .await
        .unwrap()
}

async fn initialize(directory: &Path, visits: u64) -> (AppCore, AppSubscription) {
    let profile = engine(directory);
    let settings = Settings {
        engine: Some(profile),
        auto_quick_analyze_on_load: false,
        pondering: false,
        ..Settings::default()
    };
    settings.save(directory).await.unwrap();
    start(directory, visits).await
}

async fn analysis(
    events: &mut AppSubscription,
    revision: u64,
    source: AnalysisSource,
) -> AnalysisView {
    within(async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::AnalysisUpdated(view)
                    if view.revision == revision
                        && view.source == source
                        && !view.is_during_search =>
                {
                    return view;
                }
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("analysis failed: {reason}")
                }
                _ => {}
            }
        }
    })
    .await
}

async fn analyze(app: &AppCore, events: &mut AppSubscription, node: u32) -> AnalysisView {
    let revision = app.go_to(node).await.unwrap();
    app.set_pondering(true).await.unwrap();
    let view = analysis(events, revision, AnalysisSource::Live).await;
    app.set_pondering(false).await.unwrap();
    view
}

#[tokio::test]
async fn restart_hit_then_deeper_live_result_is_persisted_with_complete_payload() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = initialize(directory.path(), 100).await;
    app.open_bytes(GAME.to_vec()).await.unwrap();
    let original = analyze(&app, &mut events, 1).await;
    assert_eq!(original.visits, 100);
    app.shutdown().await.unwrap();

    let (app, mut events) = start(directory.path(), 200).await;
    app.open_bytes(GAME.to_vec()).await.unwrap();
    let revision = app.go_to(1).await.unwrap();
    let cached = analysis(&mut events, revision, AnalysisSource::Cache).await;
    assert_eq!(cached.visits, 100);
    assert_eq!(cached.candidates, original.candidates);
    assert_eq!(cached.ownership, original.ownership);
    assert_eq!(
        (cached.winrate, cached.score_lead),
        (original.winrate, original.score_lead)
    );
    app.set_pondering(true).await.unwrap();
    let deeper = analysis(&mut events, revision, AnalysisSource::Live).await;
    assert_eq!(deeper.visits, 200);
    app.set_pondering(false).await.unwrap();
    app.shutdown().await.unwrap();

    let (app, mut events) = start(directory.path(), 1).await;
    app.open_bytes(GAME.to_vec()).await.unwrap();
    let revision = app.go_to(1).await.unwrap();
    assert_eq!(
        analysis(&mut events, revision, AnalysisSource::Cache)
            .await
            .visits,
        200
    );
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn changed_weight_at_same_path_misses_persisted_result() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = initialize(directory.path(), 100).await;
    app.open_bytes(GAME.to_vec()).await.unwrap();
    analyze(&app, &mut events, 1).await;
    app.shutdown().await.unwrap();
    std::fs::write(
        directory.path().join("model.bin"),
        b"a different weight file",
    )
    .unwrap();
    let (app, mut events) = start(directory.path(), 1).await;
    app.open_bytes(GAME.to_vec()).await.unwrap();
    let view = analyze(&app, &mut events, 1).await;
    assert_eq!(view.visits, 1);
    assert_eq!(view.summary.visits, 1);
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn disabled_cache_neither_reads_nor_writes_and_setting_survives_restart() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = initialize(directory.path(), 100).await;
    app.open_bytes(GAME.to_vec()).await.unwrap();
    analyze(&app, &mut events, 1).await;
    app.set_cache_enabled(false).await.unwrap();
    app.shutdown().await.unwrap();
    let path = directory.path().join("analysis-cache.sqlite3");
    let before = std::fs::read(&path).unwrap();

    let (app, mut events) = start(directory.path(), 1).await;
    assert!(!app.cache_enabled().await.unwrap());
    app.open_bytes(GAME.to_vec()).await.unwrap();
    assert_eq!(analyze(&app, &mut events, 1).await.visits, 1);
    analyze(&app, &mut events, 2).await;
    app.shutdown().await.unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), before);

    let (app, mut events) = start(directory.path(), 1).await;
    app.set_cache_enabled(true).await.unwrap();
    app.open_bytes(GAME.to_vec()).await.unwrap();
    let revision = app.go_to(1).await.unwrap();
    assert_eq!(
        analysis(&mut events, revision, AnalysisSource::Cache)
            .await
            .visits,
        100
    );
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn clear_position_only_changes_database_and_all_clears_other_positions() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = initialize(directory.path(), 100).await;
    app.open_bytes(GAME.to_vec()).await.unwrap();
    analyze(&app, &mut events, 1).await;
    analyze(&app, &mut events, 2).await;
    let before = app.whole_game_analysis(2).await.unwrap();
    app.clear_cache(false).await.unwrap();
    assert_eq!(app.whole_game_analysis(2).await.unwrap(), before);
    assert!(app.undo().await.is_err());
    app.shutdown().await.unwrap();

    let (app, mut events) = start(directory.path(), 1).await;
    app.open_bytes(GAME.to_vec()).await.unwrap();
    let revision = app.go_to(1).await.unwrap();
    assert_eq!(
        analysis(&mut events, revision, AnalysisSource::Cache)
            .await
            .visits,
        100
    );
    assert_eq!(analyze(&app, &mut events, 2).await.visits, 1);
    app.clear_cache(true).await.unwrap();
    app.shutdown().await.unwrap();

    let (app, mut events) = start(directory.path(), 1).await;
    app.open_bytes(GAME.to_vec()).await.unwrap();
    assert_eq!(analyze(&app, &mut events, 1).await.visits, 1);
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn corrupt_or_unwritable_database_does_not_interrupt_fake_analysis() {
    for corrupt in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("analysis-cache.sqlite3");
        if corrupt {
            std::fs::write(&path, b"broken sqlite").unwrap();
        } else {
            std::fs::create_dir(&path).unwrap();
        }
        let (app, mut events) = initialize(directory.path(), 5).await;
        app.open_bytes(GAME.to_vec()).await.unwrap();
        assert_eq!(analyze(&app, &mut events, 1).await.visits, 5);
        assert_eq!(analyze(&app, &mut events, 2).await.visits, 5);
        app.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn whole_game_final_results_are_reused_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = initialize(directory.path(), 1).await;
    app.open_bytes(GAME.to_vec()).await.unwrap();
    app.start_whole_game_analysis(WholeGameOptions {
        visits: 500,
        ..Default::default()
    })
    .await
    .unwrap();
    within(async {
        loop {
            if let AppEvent::WholeGameProgress(progress) = events.recv().await.unwrap()
                && progress.status == WholeGameStatus::Completed
            {
                break;
            }
        }
    })
    .await;
    app.shutdown().await.unwrap();
    let (app, _) = start(directory.path(), 1).await;
    app.open_bytes(GAME.to_vec()).await.unwrap();
    let progress = app
        .start_whole_game_analysis(WholeGameOptions {
            visits: 500,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(progress.status, WholeGameStatus::Completed);
    assert_eq!((progress.completed, progress.total), (3, 3));
    assert_eq!(
        app.whole_game_analysis(2)
            .await
            .unwrap()
            .unwrap()
            .response
            .root_info
            .visits,
        500
    );
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn cache_setting_defaults_for_old_files_and_failed_save_preserves_value() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("settings.json"), b"{\"version\":1}").unwrap();
    let (app, _) = start(directory.path(), 1).await;
    assert!(app.cache_enabled().await.unwrap());
    std::fs::remove_file(directory.path().join("settings.json")).unwrap();
    std::fs::create_dir(directory.path().join("settings.json")).unwrap();
    assert!(app.set_cache_enabled(false).await.is_err());
    assert!(app.cache_enabled().await.unwrap());
    app.shutdown().await.unwrap();
}
