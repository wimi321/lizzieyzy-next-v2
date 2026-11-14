use super::{Harness, json, local_url, profile, request, within};
use app_core::{AnalysisLimitSettings, AnalysisPauseReason, AnalysisState, AppConfig, AppEvent};
use std::time::Duration;

async fn next_state(harness: &mut Harness, callback: u32, expected: AnalysisState) {
    within(async {
        loop {
            let message = harness.next().await;
            if message.callback == callback
                && let AppEvent::AnalysisStateChanged(state) = message.event
                && state == expected
            {
                break;
            }
        }
    })
    .await;
}

#[tokio::test]
async fn analysis_limit_settings_round_trip_through_ipc_and_disk() {
    let harness = Harness::new().await;
    let defaults = AnalysisLimitSettings::default();
    assert_eq!(
        harness.invoke("analysis_limits", json!({})).await,
        json!(defaults)
    );
    let settings = AnalysisLimitSettings {
        limit_time: false,
        max_analyze_time_seconds: 27,
        limit_playout: true,
        limit_playouts: 12345,
    };
    assert_eq!(
        harness
            .invoke("set_analysis_limits", json!({ "settings": settings }))
            .await,
        json!(settings)
    );
    assert_eq!(
        harness.invoke("analysis_limits", json!({})).await,
        json!(settings)
    );
    assert_eq!(
        harness.invoke("settings", json!({})).await["analysis_limits"],
        json!(settings)
    );
    assert_eq!(
        app_core::Settings::load(harness.directory.path())
            .await
            .unwrap()
            .analysis_limits,
        settings
    );
    for invalid in [
        json!({ "max_analyze_time_seconds": 0 }),
        json!({ "max_analyze_time_seconds": -1 }),
        json!({ "max_analyze_time_seconds": 0.5 }),
        json!({ "limit_playouts": 0 }),
        json!({ "limit_playouts": 9007199254740992_u64 }),
    ] {
        assert!(
            request(
                &harness.main,
                "set_analysis_limits",
                json!({ "settings": invalid }),
                local_url(),
            )
            .await
            .is_err()
        );
        assert_eq!(
            harness.invoke("analysis_limits", json!({})).await,
            json!(settings)
        );
    }
    harness.finish().await;
}

#[tokio::test]
async fn analysis_state_pause_reason_and_toggle_survive_ipc_and_reconnect() {
    let mut harness = Harness::new().await;
    let first = harness.subscribe(39).await;
    harness.document(39, 0).await;
    let paused = AnalysisState {
        pure_network: false,
        pondering: false,
        pause_reason: Some(AnalysisPauseReason::User),
    };
    assert_eq!(
        harness.invoke("toggle_pondering", json!({})).await,
        json!(paused)
    );
    next_state(&mut harness, 39, paused).await;
    assert_eq!(
        harness.invoke("analysis_state", json!({})).await,
        json!(paused)
    );
    harness.invoke("unsubscribe", json!({ "id": first })).await;
    harness.subscribe(40).await;
    next_state(&mut harness, 40, paused).await;
    let enabled = AnalysisState {
        pure_network: false,
        pondering: true,
        pause_reason: None,
    };
    assert_eq!(
        harness.invoke("toggle_pondering", json!({})).await,
        json!(enabled)
    );
    next_state(&mut harness, 40, enabled).await;
    harness.finish().await;
}

#[tokio::test]
async fn automatic_limit_reason_is_queryable_and_space_resume_keeps_saved_pondering() {
    let mut config = AppConfig::default();
    config.analysis.live.report_interval = Duration::from_millis(10);
    config.session.write_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(60);
    let mut harness = Harness::with_config(config).await;
    harness.subscribe(41).await;
    harness.document(41, 0).await;
    harness
        .invoke(
            "set_analysis_limits",
            json!({ "settings": {
        "limit_time": false, "max_analyze_time_seconds": 600,
        "limit_playout": true, "limit_playouts": 1,
    } }),
        )
        .await;
    harness
        .invoke(
            "configure_engine",
            json!({ "profile": profile(harness.directory.path()) }),
        )
        .await;
    let paused = AnalysisState {
        pure_network: false,
        pondering: false,
        pause_reason: Some(AnalysisPauseReason::PlayoutLimit),
    };
    next_state(&mut harness, 41, paused).await;
    assert_eq!(
        harness.invoke("analysis_state", json!({})).await,
        json!(paused)
    );
    assert_eq!(
        harness.invoke("settings", json!({})).await["pondering"],
        true
    );
    // Disable the threshold before resuming so this IPC test observes the
    // resumed state without immediately hitting the same tiny test limit.
    harness
        .invoke(
            "set_analysis_limits",
            json!({ "settings": {
        "limit_time": false, "max_analyze_time_seconds": 600,
        "limit_playout": false, "limit_playouts": 1,
    } }),
        )
        .await;
    assert_eq!(
        harness.invoke("analysis_state", json!({})).await,
        json!(paused)
    );
    let resumed = AnalysisState {
        pure_network: false,
        pondering: true,
        pause_reason: None,
    };
    assert_eq!(
        harness.invoke("toggle_pondering", json!({})).await,
        json!(resumed)
    );
    next_state(&mut harness, 41, resumed).await;
    assert!(
        app_core::Settings::load(harness.directory.path())
            .await
            .unwrap()
            .pondering
    );
    harness.finish().await;
}

#[tokio::test]
async fn analysis_limit_commands_are_local_main_only() {
    let harness = Harness::new().await;
    let other =
        tauri::WebviewWindowBuilder::new(&harness.app, "analysis-limit-other", Default::default())
            .build()
            .unwrap();
    for command in [
        "analysis_limits",
        "set_analysis_limits",
        "analysis_state",
        "toggle_pondering",
    ] {
        for (window, url) in [
            (&other, local_url()),
            (&harness.main, "https://example.com/"),
        ] {
            let error = request(
                window,
                command,
                json!({ "settings": AnalysisLimitSettings::default() }),
                url,
            )
            .await
            .unwrap_err();
            assert!(
                error
                    .as_str()
                    .is_some_and(|text| text.contains("not allowed"))
            );
        }
    }
    harness.finish().await;
}
