use app_core::{
    AppError, DisplaySettings, MoveNumberMode, MoveRankMarkMode, NextMoveHint, Settings,
    SettingsError, WinrateGraphMode,
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
