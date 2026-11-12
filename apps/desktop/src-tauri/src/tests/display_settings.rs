use super::{Harness, json, local_url, request};
use app_core::{DisplaySettings, Settings};

#[tokio::test]
async fn display_settings_round_trip_through_ipc_and_reject_invalid_values() {
    let harness = Harness::new().await;
    assert_eq!(
        harness.invoke("settings", json!({})).await["display"],
        json!(DisplaySettings::default())
    );
    let display = DisplaySettings {
        show_coordinates: false,
        last_move_numbers: 23,
        confirm_overwrite: false,
        ..Default::default()
    };
    assert_eq!(
        harness
            .invoke("set_display_settings", json!({ "display": display }))
            .await,
        json!(display)
    );
    assert_eq!(
        harness.invoke("settings", json!({})).await["display"],
        json!(display)
    );
    assert_eq!(
        Settings::load(harness.directory.path())
            .await
            .unwrap()
            .display,
        display
    );
    let path = harness.directory.path().join("settings.json");
    let saved = std::fs::read(&path).unwrap();
    for invalid in [
        json!({ "last_move_numbers": 0 }),
        json!({ "last_move_numbers": -1 }),
        json!({ "last_move_numbers": 0.5 }),
        json!({ "last_move_numbers": 2147483648_u64 }),
        json!({ "last_move_rank_marks": 0 }),
        json!({ "move_numbers": "Unknown" }),
    ] {
        assert!(
            request(
                &harness.main,
                "set_display_settings",
                json!({ "display": invalid }),
                local_url(),
            )
            .await
            .is_err()
        );
        assert_eq!(
            harness.invoke("settings", json!({})).await["display"],
            json!(display)
        );
        assert_eq!(std::fs::read(&path).unwrap(), saved);
    }
    harness.finish().await;
}

#[tokio::test]
async fn display_settings_ipc_reports_save_failures_without_changing_settings() {
    let harness = Harness::new().await;
    std::fs::create_dir(harness.directory.path().join("settings.json")).unwrap();
    let display = DisplaySettings {
        show_coordinates: false,
        ..Default::default()
    };
    let error = request(
        &harness.main,
        "set_display_settings",
        json!({ "display": display }),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(error["type"], "App");
    assert_eq!(
        harness.invoke("settings", json!({})).await["display"],
        json!(DisplaySettings::default())
    );
    harness.finish().await;
}

#[tokio::test]
async fn display_settings_command_is_local_main_only() {
    let harness = Harness::new().await;
    let other = tauri::WebviewWindowBuilder::new(&harness.app, "display-other", Default::default())
        .build()
        .unwrap();
    for (window, url) in [
        (&other, local_url()),
        (&harness.main, "https://example.com/"),
    ] {
        let error = request(
            window,
            "set_display_settings",
            json!({ "display": DisplaySettings::default() }),
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
    harness.finish().await;
}
