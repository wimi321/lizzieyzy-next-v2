use super::{Harness, json, local_url, request};
use crate::window_preferences::{WindowPlatform, WindowPreferences};
use tauri::Manager;
use tokio::sync::mpsc;

struct FakeWindow {
    calls: mpsc::UnboundedSender<bool>,
    failure: bool,
}

impl WindowPlatform for FakeWindow {
    fn set_always_on_top(&self, enabled: bool) -> Result<(), String> {
        self.calls.send(enabled).unwrap();
        if self.failure {
            Err("native window unavailable".into())
        } else {
            Ok(())
        }
    }
}

fn window(harness: &Harness, failure: bool) -> mpsc::UnboundedReceiver<bool> {
    let (calls, received) = mpsc::unbounded_channel();
    assert!(
        harness
            .app
            .manage(WindowPreferences::with_platform(FakeWindow {
                calls,
                failure,
            }))
    );
    received
}

#[tokio::test]
async fn daily_preferences_round_trip_and_initialize_the_saved_main_window_setting() {
    let harness = Harness::new().await;
    let mut calls = window(&harness, false);
    let window = harness.app.state::<WindowPreferences>();
    window.initialize(&harness.core).await.unwrap();
    assert!(!calls.try_recv().unwrap());
    for (getter, setter, argument, value, default) in [
        (
            "always_on_top",
            "set_always_on_top",
            "enabled",
            json!(true),
            json!(false),
        ),
        ("ui_scale", "set_ui_scale", "scale", json!(1.5), json!(1.0)),
        (
            "play_sound",
            "set_play_sound",
            "enabled",
            json!(false),
            json!(true),
        ),
    ] {
        assert_eq!(harness.invoke(getter, json!({})).await, default);
        assert_eq!(
            harness.invoke(setter, json!({ argument: value })).await,
            value
        );
        assert_eq!(harness.invoke(getter, json!({})).await, value);
        assert_eq!(harness.invoke("settings", json!({})).await[getter], value);
    }
    assert!(calls.try_recv().unwrap());
    window.initialize(&harness.core).await.unwrap();
    assert!(calls.try_recv().unwrap());
    assert!(calls.try_recv().is_err());
    let saved = app_core::Settings::load(harness.directory.path())
        .await
        .unwrap();
    assert!(saved.always_on_top);
    assert_eq!(saved.ui_scale, 1.5);
    assert!(!saved.play_sound);
    harness.finish().await;
}

#[tokio::test]
async fn failed_preference_save_does_not_apply_a_window_change() {
    let harness = Harness::new().await;
    let mut calls = window(&harness, false);
    std::fs::create_dir(harness.directory.path().join("settings.json")).unwrap();
    for (command, arguments) in [
        ("set_always_on_top", json!({ "enabled": true })),
        ("set_ui_scale", json!({ "scale": 1.5 })),
        ("set_play_sound", json!({ "enabled": false })),
    ] {
        let error = request(&harness.main, command, arguments, local_url())
            .await
            .unwrap_err();
        assert_eq!(error["type"], "App");
    }
    assert!(calls.try_recv().is_err());
    assert_eq!(harness.invoke("always_on_top", json!({})).await, false);
    assert_eq!(harness.invoke("ui_scale", json!({})).await, 1.0);
    assert_eq!(harness.invoke("play_sound", json!({})).await, true);
    harness.finish().await;
}

#[tokio::test]
async fn window_failure_reports_native_failure_and_restores_the_previous_preference() {
    let harness = Harness::new().await;
    let mut calls = window(&harness, true);
    let error = request(
        &harness.main,
        "set_always_on_top",
        json!({ "enabled": true }),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(
        error,
        json!({ "type": "WindowUnavailable", "data": "native window unavailable" })
    );
    assert!(calls.try_recv().unwrap());
    assert!(
        !app_core::Settings::load(harness.directory.path())
            .await
            .unwrap()
            .always_on_top
    );
    assert!(!harness.core.always_on_top().await.unwrap());
    harness.finish().await;
}

struct FailWindowAndBlockRollback(std::path::PathBuf);

impl WindowPlatform for FailWindowAndBlockRollback {
    fn set_always_on_top(&self, enabled: bool) -> Result<(), String> {
        assert!(enabled);
        // The native effect is the deterministic barrier: the initial save
        // completed, and the rollback has not started. No timing assumptions.
        std::fs::rename(
            self.0.join("settings.json"),
            self.0.join("previous-settings.json"),
        )
        .unwrap();
        std::fs::create_dir(self.0.join("settings.json")).unwrap();
        Err("native window unavailable".into())
    }
}

#[tokio::test]
async fn rollback_failure_reports_both_failures_and_preserves_the_actual_requested_value() {
    let harness = Harness::new().await;
    assert!(harness.app.manage(WindowPreferences::with_platform(
        FailWindowAndBlockRollback(harness.directory.path().to_owned())
    )));
    let error = request(
        &harness.main,
        "set_always_on_top",
        json!({ "enabled": true }),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(error["type"], "WindowUnavailable");
    let detail = error["data"].as_str().unwrap();
    assert!(detail.contains("native window unavailable"));
    assert!(detail.contains("could not restore the previous saved preference"));
    assert!(detail.contains("the requested preference may remain saved"));
    assert!(harness.core.always_on_top().await.unwrap());
    let saved: app_core::Settings = serde_json::from_slice(
        &std::fs::read(harness.directory.path().join("previous-settings.json")).unwrap(),
    )
    .unwrap();
    assert!(saved.always_on_top);
    assert!(harness.directory.path().join("settings.json").is_dir());
    harness.finish().await;
}

#[tokio::test]
async fn daily_preferences_validate_scale_and_accept_all_language_options() {
    let harness = Harness::new().await;
    for scale in [0.49, 3.01] {
        assert!(
            request(
                &harness.main,
                "set_ui_scale",
                json!({ "scale": scale }),
                local_url()
            )
            .await
            .is_err()
        );
        assert_eq!(harness.invoke("ui_scale", json!({})).await, 1.0);
    }
    for language in ["System", "ZhCn", "En", "ZhTw", "Ja", "Ko", "Th"] {
        assert_eq!(
            harness
                .invoke("set_language", json!({ "language": language }))
                .await,
            language
        );
    }
    harness.finish().await;
}
