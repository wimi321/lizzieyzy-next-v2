use super::{Harness, json, local_url, request};
use app_core::{
    DisplaySettings, EstimateLocation, EstimateStyle, Settings, SubBoardHeat, TopToolbarLayout,
};

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
        estimate_location: EstimateLocation::Hidden,
        estimate_style: EstimateStyle::BySize,
        pure_estimate_style: EstimateStyle::TransparentOffStones,
        remember_estimate: true,
        moves_ownership: false,
        estimate_shortcut: true,
        sub_board_heat: SubBoardHeat::AfterSearch,
        sub_board_raw: true,
        top_toolbar_layout: TopToolbarLayout::Combined,
        top_toolbar_wrap: true,
        top_toolbar_hidden: vec!["Engine-1".into()],
        bottom_toolbar_hidden: vec!["previous_move".into()],
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
        json!({ "estimate_location": "Unknown" }),
        json!({ "estimate_style": "Unknown" }),
        json!({ "pure_estimate_style": "Unknown" }),
        json!({ "remember_estimate": 1 }),
        json!({ "moves_ownership": "true" }),
        json!({ "estimate_shortcut": null }),
        json!({ "sub_board_heat": "Unknown" }),
        json!({ "sub_board_raw": 0 }),
        json!({ "top_toolbar_layout": "Unknown" }),
        json!({ "top_toolbar_wrap": "false" }),
        json!({ "top_toolbar_hidden": "next" }),
        json!({ "bottom_toolbar_hidden": [1] }),
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
async fn toolbar_ids_are_normalized_and_invalid_ids_have_typed_ipc_errors() {
    let harness = Harness::new().await;
    let display = DisplaySettings {
        top_toolbar_hidden: vec!["next".into(), "prev".into(), "next".into()],
        bottom_toolbar_hidden: vec!["play".into(), "play".into()],
        ..Default::default()
    };
    let saved = harness
        .invoke("set_display_settings", json!({ "display": display }))
        .await;
    assert_eq!(saved["top_toolbar_hidden"], json!(["next", "prev"]));
    assert_eq!(saved["bottom_toolbar_hidden"], json!(["play"]));
    let path = harness.directory.path().join("settings.json");
    let before = std::fs::read(&path).unwrap();
    for field in ["top_toolbar_hidden", "bottom_toolbar_hidden"] {
        for (ids, failure) in [
            (
                json!(["ok", "bad/id"]),
                json!({
                    "type": "InvalidToolbarItem", "data": {"field": field, "index": 1}
                }),
            ),
            (
                json!(vec!["x"; 65]),
                json!({
                    "type": "TooManyToolbarItems", "data": {"field": field}
                }),
            ),
        ] {
            let error = request(
                &harness.main,
                "set_display_settings",
                json!({"display": {field: ids}}),
                local_url(),
            )
            .await
            .unwrap_err();
            assert_eq!(error, json!({"type": "DisplaySettings", "data": failure}));
            assert_eq!(
                harness.invoke("settings", json!({})).await["display"],
                saved
            );
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
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
