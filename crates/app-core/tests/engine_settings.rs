#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{
    AppConfig, AppCore, AppEvent, AppSubscription, EngineProfile, EngineRules, EngineSettings,
    Settings, WholeGameOptions, WholeGameStatus,
};
use serde_json::Value;
use std::{future::Future, path::Path, time::Duration};
use tempfile::TempDir;

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("safety deadline")
}

fn profile(directory: &Path) -> EngineProfile {
    let model = directory.join("test.bin");
    let config = directory.join("test.cfg");
    std::fs::write(&model, "model").unwrap();
    std::fs::write(&config, "").unwrap();
    EngineProfile {
        human_model: None,
        program: fake_katago::binary_path().unwrap(),
        model,
        config,
        extra_args: vec![
            "--request-log".into(),
            directory.join("requests.jsonl").to_string_lossy().into(),
            "--visits-per-sec".into(),
            "100000".into(),
        ],
        canonical_model_name: None,
    }
}

async fn app(directory: &Path) -> (AppCore, AppSubscription) {
    let mut config = AppConfig::default();
    config.analysis.live.max_visits = 1;
    config.session.startup_timeout = Duration::from_secs(60);
    config.session.response_timeout = Duration::from_secs(60);
    config.session.write_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(60);
    app_core::spawn_with_config(directory, config)
        .await
        .unwrap()
}

async fn final_live(output: &mut AppSubscription) -> app_core::AnalysisView {
    within(async {
        loop {
            match output.recv().await.unwrap() {
                AppEvent::AnalysisUpdated(view)
                    if !view.is_during_search && view.source == app_core::AnalysisSource::Live =>
                {
                    return view;
                }
                AppEvent::EngineStatusChanged(app_core::EngineStatus::Failed {
                    reason, ..
                }) => panic!("engine failed: {reason}"),
                _ => (),
            }
        }
    })
    .await
}

fn requests(directory: &Path) -> Vec<Value> {
    std::fs::read_to_string(directory.join("requests.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|line| line.get("moves").is_some())
        .collect()
}

#[tokio::test]
async fn settings_apply_to_live_and_review_queries_without_rewriting_sgf() {
    let directory = TempDir::new().unwrap();
    let profile = profile(directory.path());
    let (app, mut events) = app(directory.path()).await;
    app.set_auto_quick_analyze_on_load(false).await.unwrap();
    app.open_bytes(b"(;SZ[9]KM[6.5]RU[japanese];B[aa])".to_vec())
        .await
        .unwrap();
    app.configure_engine(Some(profile)).await.unwrap();
    final_live(&mut events).await;
    let initial = requests(directory.path()).pop().unwrap();
    assert_eq!(initial["rules"], "japanese");
    assert_eq!(initial["komi"], 6.5);
    assert_eq!(initial["reportDuringSearchEvery"], 0.1);

    let settings = EngineSettings {
        rules: EngineRules::Aga,
        read_komi: false,
        default_komi: 7.5,
        auto_check_alive: false,
        report_interval_ms: 270,
    };
    assert_eq!(
        app.set_engine_settings(settings.clone()).await.unwrap(),
        settings
    );
    final_live(&mut events).await;
    let changed = requests(directory.path()).pop().unwrap();
    assert_eq!(changed["rules"], "aga");
    assert_eq!(changed["komi"], 7.5);
    assert_eq!(changed["reportDuringSearchEvery"], 0.27);
    assert_eq!(changed["firstReportDuringSearchAfter"], 0.27);
    app.start_whole_game_analysis(WholeGameOptions {
        visits: 500,
        ..Default::default()
    })
    .await
    .unwrap();
    within(async {
        loop {
            if let AppEvent::WholeGameProgress(progress) = events.recv().await.unwrap() {
                assert!(!matches!(progress.status, WholeGameStatus::Error { .. }));
                if progress.status == WholeGameStatus::Completed {
                    break;
                }
            }
        }
    })
    .await;
    let review = requests(directory.path())
        .into_iter()
        .find(|request| request["priority"] == -100)
        .unwrap();
    assert_eq!(review["rules"], "aga");
    assert_eq!(review["komi"], 7.5);
    assert_eq!(review["reportDuringSearchEvery"], 0.27);
    assert_eq!(
        Settings::load(directory.path())
            .await
            .unwrap()
            .engine_settings,
        settings
    );
    let saved = directory.path().join("saved.sgf");
    app.save(saved.clone()).await.unwrap();
    let text = std::fs::read_to_string(saved).unwrap();
    assert!(text.contains("KM[6.5]"));
    assert!(text.contains("RU[japanese]"));
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn changing_query_rules_does_not_reuse_deeper_old_engine_results() {
    let directory = TempDir::new().unwrap();
    let profile = profile(directory.path());
    let (app, mut events) = app(directory.path()).await;
    app.configure_engine(Some(profile)).await.unwrap();
    final_live(&mut events).await;
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
    assert_eq!(
        app.whole_game_analysis(0)
            .await
            .unwrap()
            .unwrap()
            .response
            .root_info
            .visits,
        500
    );
    app.set_engine_settings(EngineSettings {
        rules: EngineRules::TrompTaylor,
        ..EngineSettings::default()
    })
    .await
    .unwrap();
    let fresh = final_live(&mut events).await;
    assert_eq!(fresh.visits, 1);
    assert_eq!(
        app.whole_game_analysis(0)
            .await
            .unwrap()
            .unwrap()
            .response
            .root_info
            .visits,
        1
    );
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn invalid_and_failed_settings_writes_keep_the_original_value() {
    let directory = TempDir::new().unwrap();
    let (app, _) = app(directory.path()).await;
    let invalid = EngineSettings {
        report_interval_ms: 0,
        ..EngineSettings::default()
    };
    assert!(app.set_engine_settings(invalid).await.is_err());
    assert_eq!(
        app.settings().await.unwrap().engine_settings,
        EngineSettings::default()
    );
    std::fs::create_dir(directory.path().join("settings.json")).unwrap();
    let changed = EngineSettings {
        read_komi: false,
        ..EngineSettings::default()
    };
    assert!(app.set_engine_settings(changed).await.is_err());
    assert_eq!(
        app.settings().await.unwrap().engine_settings,
        EngineSettings::default()
    );
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn undo_clear_after_rule_change_does_not_restore_old_engine_analysis() {
    let directory = TempDir::new().unwrap();
    let profile = profile(directory.path());
    let (app, mut events) = app(directory.path()).await;
    app.configure_engine(Some(profile)).await.unwrap();
    final_live(&mut events).await;
    app.set_pondering(false).await.unwrap();
    assert!(app.whole_game_analysis(0).await.unwrap().is_some());
    app.clear_analysis(false).await.unwrap();
    app.set_engine_settings(EngineSettings {
        rules: EngineRules::Japanese,
        ..EngineSettings::default()
    })
    .await
    .unwrap();
    app.undo().await.unwrap();
    assert!(app.whole_game_analysis(0).await.unwrap().is_none());
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn explicit_rules_do_not_skip_review_using_saved_analysis_from_the_sgf() {
    let directory = TempDir::new().unwrap();
    let profile = profile(directory.path());
    let (app, mut events) = app(directory.path()).await;
    app.set_auto_quick_analyze_on_load(false).await.unwrap();
    app.set_pondering(false).await.unwrap();
    app.open_bytes(include_bytes!("../../../fixtures/lz/java-analysis.sgf").to_vec())
        .await
        .unwrap();
    app.configure_engine(Some(profile)).await.unwrap();
    app.set_engine_settings(EngineSettings {
        rules: EngineRules::Aga,
        ..EngineSettings::default()
    })
    .await
    .unwrap();
    assert!(
        app.whole_game_analysis(0)
            .await
            .unwrap()
            .unwrap()
            .saved
            .is_some()
    );
    app.start_whole_game_analysis(WholeGameOptions {
        visits: 500,
        ..Default::default()
    })
    .await
    .unwrap();
    within(async {
        loop {
            if let AppEvent::WholeGameProgress(progress) = events.recv().await.unwrap() {
                assert!(!matches!(progress.status, WholeGameStatus::Error { .. }));
                if progress.status == WholeGameStatus::Completed {
                    break;
                }
            }
        }
    })
    .await;
    let analyzed = requests(directory.path());
    assert!(analyzed.iter().all(|request| request["rules"] == "aga"));
    assert!(analyzed.iter().any(|request| {
        request["analyzeTurns"]
            .as_array()
            .unwrap()
            .contains(&Value::from(0))
    }));
    let result = app.whole_game_analysis(0).await.unwrap().unwrap();
    assert!(result.saved.is_none());
    assert_eq!(result.response.root_info.visits, 500);
    within(app.shutdown()).await.unwrap();
}
