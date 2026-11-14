use app_core::{
    AppError, DisplaySettings, DisplaySettingsError, EstimateLocation, EstimateStyle,
    MoveNumberMode, MoveRankMarkMode, NextMoveHint, Settings, SettingsError, SubBoardHeat,
    TopToolbarLayout, WinrateGraphMode,
};
use serde_json::json;

#[tokio::test]
async fn display_settings_save_restart_and_whole_object_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let (app, _events) = app_core::spawn(directory.path()).await.unwrap();
    assert_eq!(
        app.settings().await.unwrap().display,
        DisplaySettings::default()
    );
    let display = DisplaySettings {
        show_coordinates: false,
        move_numbers: MoveNumberMode::LastN,
        last_move_numbers: 42,
        move_numbers_from_one: true,
        branch_move_numbers_from_one: false,
        show_all_branch_move_numbers: true,
        move_rank_marks: MoveRankMarkMode::All,
        last_move_rank_marks: 7,
        show_winrate_in_suggestion: false,
        show_visits_in_suggestion: false,
        show_score_in_suggestion: false,
        show_score_as_diff: true,
        show_suggestion_order: false,
        winrate_always_black: true,
        show_hover_variation: false,
        max_suggestions: 0,
        suggestion_max_red: true,
        next_moves: NextMoveHint::Simple,
        next_move_min_visits: 0,
        show_winrate_graph: false,
        show_comment: false,
        show_variation_graph: false,
        show_sub_board: false,
        show_list_pane: false,
        show_information_panel: false,
        show_status_bar: false,
        show_top_toolbar: false,
        show_bottom_toolbar: false,
        winrate_graph_mode: WinrateGraphMode::ScoreLead,
        fill_winrate_graph: false,
        show_blunder_bar: true,
        score_lead_includes_komi: false,
        confirm_new_board: false,
        confirm_overwrite: false,
        show_names_below_board: false,
        estimate_location: EstimateLocation::Both,
        estimate_style: EstimateStyle::BySize,
        pure_estimate_style: EstimateStyle::TransparentOffStones,
        remember_estimate: true,
        moves_ownership: false,
        estimate_shortcut: true,
        sub_board_heat: SubBoardHeat::AfterSearch,
        sub_board_raw: true,
        top_toolbar_layout: TopToolbarLayout::Combined,
        top_toolbar_wrap: true,
        top_toolbar_hidden: vec!["Menu-1".into(), "analysis".into()],
        bottom_toolbar_hidden: vec!["navigation_2".into()],
    };
    assert_eq!(
        app.set_display_settings(display.clone()).await.unwrap(),
        display
    );
    assert_eq!(app.settings().await.unwrap().display, display);
    assert_eq!(
        Settings::load(directory.path()).await.unwrap().display,
        display
    );
    app.shutdown().await.unwrap();
    let (app, _events) = app_core::spawn(directory.path()).await.unwrap();
    assert_eq!(app.settings().await.unwrap().display, display);
    // The API is replacement, including defaults for omitted JSON fields.
    let replacement: DisplaySettings =
        serde_json::from_value(json!({"show_comment": false})).unwrap();
    app.set_display_settings(replacement.clone()).await.unwrap();
    let settings = app.settings().await.unwrap();
    assert_eq!(settings.display, replacement);
    assert!(settings.display.show_coordinates);
    assert!(settings.display.show_sub_board);
    assert!(settings.display.show_list_pane);
    assert!(settings.pondering);
    assert_eq!(settings.version, 1);
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn estimate_location_resets_only_on_load_unless_remembered() {
    let directory = tempfile::tempdir().unwrap();
    for remember_estimate in [false, true] {
        for estimate_location in [
            EstimateLocation::Off,
            EstimateLocation::Hidden,
            EstimateLocation::Main,
            EstimateLocation::Sub,
            EstimateLocation::Both,
        ] {
            let (app, _events) = app_core::spawn(directory.path()).await.unwrap();
            let display = DisplaySettings {
                estimate_location,
                remember_estimate,
                ..DisplaySettings::default()
            };
            assert_eq!(
                app.set_display_settings(display.clone()).await.unwrap(),
                display
            );
            assert_eq!(app.settings().await.unwrap().display, display);
            let stored: Settings = serde_json::from_slice(
                &tokio::fs::read(directory.path().join("settings.json"))
                    .await
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(stored.display, display);
            app.shutdown().await.unwrap();
            let (app, _events) = app_core::spawn(directory.path()).await.unwrap();
            let mut expected = display;
            if !remember_estimate {
                expected.estimate_location = EstimateLocation::Off;
            }
            assert_eq!(app.settings().await.unwrap().display, expected);
            app.shutdown().await.unwrap();
        }
    }
}

#[tokio::test]
async fn toolbar_ids_are_deduplicated_in_reply_memory_and_storage() {
    let directory = tempfile::tempdir().unwrap();
    let (app, _events) = app_core::spawn(directory.path()).await.unwrap();
    let mut display = DisplaySettings {
        top_toolbar_hidden: vec!["z".into(), "a".into(), "z".into(), "a".into()],
        bottom_toolbar_hidden: vec!["next-step".into(), "next-step".into(), "prev_step".into()],
        ..DisplaySettings::default()
    };
    let saved = app.set_display_settings(display.clone()).await.unwrap();
    display.top_toolbar_hidden = vec!["z".into(), "a".into()];
    display.bottom_toolbar_hidden = vec!["next-step".into(), "prev_step".into()];
    assert_eq!(saved, display);
    assert_eq!(app.settings().await.unwrap().display, display);
    let stored: Settings = serde_json::from_slice(
        &tokio::fs::read(directory.path().join("settings.json"))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(stored.display, display);
    app.shutdown().await.unwrap();
    assert_eq!(
        Settings::load(directory.path()).await.unwrap().display,
        display
    );
    // A hand-written older file is normalized through the same loading path.
    tokio::fs::write(
        directory.path().join("settings.json"),
        br#"{"display":{"top_toolbar_hidden":["b","a","b"],"bottom_toolbar_hidden":["x","x"]}}"#,
    )
    .await
    .unwrap();
    let loaded = Settings::load(directory.path()).await.unwrap();
    assert_eq!(loaded.display.top_toolbar_hidden, ["b", "a"]);
    assert_eq!(loaded.display.bottom_toolbar_hidden, ["x"]);
}

#[tokio::test]
async fn invalid_estimate_settings_and_toolbar_ids_return_typed_errors() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    let (app, _events) = app_core::spawn(directory.path()).await.unwrap();
    let original = DisplaySettings::default();
    app.set_display_settings(original.clone()).await.unwrap();
    let before = tokio::fs::read(&path).await.unwrap();
    for field in ["top_toolbar_hidden", "bottom_toolbar_hidden"] {
        for items in [vec!["bad/id"], vec!["x"; 65]] {
            let invalid: DisplaySettings = serde_json::from_value(json!({field: items})).unwrap();
            assert!(matches!(
                app.set_display_settings(invalid).await,
                Err(AppError::Settings(SettingsError::Display(
                    DisplaySettingsError::InvalidToolbarItem { .. }
                        | DisplaySettingsError::TooManyToolbarItems { .. }
                )))
            ));
            assert_eq!(app.settings().await.unwrap().display, original);
            assert_eq!(tokio::fs::read(&path).await.unwrap(), before);
        }
    }
    app.shutdown().await.unwrap();
    for field in [
        "estimate_location",
        "estimate_style",
        "pure_estimate_style",
        "sub_board_heat",
        "top_toolbar_layout",
        "remember_estimate",
        "moves_ownership",
        "estimate_shortcut",
        "sub_board_raw",
        "top_toolbar_wrap",
    ] {
        tokio::fs::write(
            &path,
            serde_json::to_vec(&json!({"display": {field: "invalid"}})).unwrap(),
        )
        .await
        .unwrap();
        assert!(matches!(
            Settings::load(directory.path()).await,
            Err(SettingsError::Json(_))
        ));
    }
    for field in ["top_toolbar_hidden", "bottom_toolbar_hidden"] {
        tokio::fs::write(
            &path,
            serde_json::to_vec(&json!({"display": {field: ["bad/id"]}})).unwrap(),
        )
        .await
        .unwrap();
        assert!(matches!(
            Settings::load(directory.path()).await,
            Err(SettingsError::Display(_))
        ));
    }
}

#[tokio::test]
async fn legacy_settings_and_partial_display_load_defaults_without_version_bump() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    for version in [0, 1] {
        tokio::fs::write(
            &path,
            serde_json::to_vec(&json!({"version": version, "pondering": false})).unwrap(),
        )
        .await
        .unwrap();
        let (app, _events) = app_core::spawn(directory.path()).await.unwrap();
        let settings = app.settings().await.unwrap();
        assert_eq!(settings.display, DisplaySettings::default());
        assert!(settings.display.show_sub_board);
        assert!(settings.display.show_list_pane);
        assert_eq!(settings.version, 1);
        assert!(!settings.pondering);
        app.shutdown().await.unwrap();
    }
    tokio::fs::write(
        &path,
        br#"{"version":1,"display":{"move_numbers":"LastFive","show_coordinates":false}}"#,
    )
    .await
    .unwrap();
    let settings = Settings::load(directory.path()).await.unwrap();
    assert!(settings.display.show_sub_board);
    assert!(settings.display.show_list_pane);
    assert_eq!(
        settings.display,
        DisplaySettings {
            move_numbers: MoveNumberMode::LastFive,
            show_coordinates: false,
            ..DisplaySettings::default()
        }
    );
    tokio::fs::write(&path, br#"{"version":2}"#).await.unwrap();
    assert!(matches!(
        Settings::load(directory.path()).await,
        Err(SettingsError::FutureVersion(2))
    ));
}

#[tokio::test]
async fn invalid_display_settings_leave_memory_and_saved_file_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let (app, _events) = app_core::spawn(directory.path()).await.unwrap();
    let original = DisplaySettings {
        move_numbers: MoveNumberMode::All,
        ..DisplaySettings::default()
    };
    app.set_display_settings(original.clone()).await.unwrap();
    let path = directory.path().join("settings.json");
    let before = tokio::fs::read(&path).await.unwrap();
    for (field, value) in [
        ("last_move_numbers", 0),
        ("last_move_numbers", 1_000_000_000),
        ("last_move_rank_marks", 0),
        ("last_move_rank_marks", i32::MAX as u32 + 1),
        ("max_suggestions", i32::MAX as u32 + 1),
        ("next_move_min_visits", i32::MAX as u32 + 1),
    ] {
        let mut value_json = serde_json::to_value(&original).unwrap();
        value_json[field] = value.into();
        let invalid: DisplaySettings = serde_json::from_value(value_json).unwrap();
        assert!(matches!(
            app.set_display_settings(invalid).await,
            Err(AppError::Settings(SettingsError::Display(_)))
        ));
        assert_eq!(app.settings().await.unwrap().display, original);
        assert_eq!(tokio::fs::read(&path).await.unwrap(), before);
    }
    app.shutdown().await.unwrap();
    tokio::fs::write(&path, br#"{"display":{"last_move_numbers":0}}"#)
        .await
        .unwrap();
    assert!(matches!(
        Settings::load(directory.path()).await,
        Err(SettingsError::Display(_))
    ));
}

#[tokio::test]
async fn failed_display_write_keeps_in_memory_settings() {
    let directory = tempfile::tempdir().unwrap();
    let (app, _events) = app_core::spawn(directory.path()).await.unwrap();
    let original = app.settings().await.unwrap();
    // A directory at the destination makes atomic replacement fail on all hosts.
    tokio::fs::create_dir(directory.path().join("settings.json"))
        .await
        .unwrap();
    let display = DisplaySettings {
        show_coordinates: false,
        ..DisplaySettings::default()
    };
    assert!(matches!(
        app.set_display_settings(display).await,
        Err(AppError::Settings(SettingsError::Io { .. }))
    ));
    assert_eq!(app.settings().await.unwrap(), original);
    app.shutdown().await.unwrap();
}
