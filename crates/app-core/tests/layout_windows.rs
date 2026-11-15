mod common;
use app_core::{
    AppEvent, CustomLayout, DisplaySettings, DisplaySettingsError, FloatingBoardKind,
    FloatingBoardsView, LayoutMode, Settings, SettingsError, WindowBounds, WindowKind,
    WindowPlacement,
};
use common::within;
use serde_json::json;

fn placement() -> WindowPlacement {
    WindowPlacement {
        bounds: WindowBounds {
            x: -1800,
            y: -200,
            width: 600,
            height: 500,
        },
        maximized: true,
    }
}

#[tokio::test]
async fn window_updates_are_partial_persisted_and_emit_ordered_events() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = within(app_core::spawn(directory.path())).await.unwrap();
    let display = DisplaySettings {
        layout_mode: LayoutMode::Thinking,
        board_position: 8,
        show_comment: false,
        confirm_new_board: false,
        custom_layouts: [Some(CustomLayout::default()), None],
        ..Default::default()
    };
    within(app.set_display_settings(display.clone()))
        .await
        .unwrap();
    for mask in 0..8 {
        let (main, keep, sub) = (mask & 1 != 0, mask & 2 != 0, mask & 4 != 0);
        let view = within(app.set_floating_layout(main, keep, sub))
            .await
            .unwrap();
        assert_eq!(
            (view.main, view.keep_main_board, view.sub),
            (main, keep, sub)
        );
    }
    let view = within(app.set_floating_board_options(FloatingBoardKind::Main, true, true))
        .await
        .unwrap();
    assert!(view.main && view.keep_main_board && view.sub && view.main_on_top && view.main_locked);
    let view = within(app.set_floating_board_options(FloatingBoardKind::Sub, true, true))
        .await
        .unwrap();
    assert!(view.sub_on_top && view.sub_locked);
    let view = within(app.set_floating_board_open(FloatingBoardKind::Main, false))
        .await
        .unwrap();
    assert!(!view.main && view.keep_main_board && view.sub && view.main_locked);
    let mut saw_display = false;
    within(async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::DisplaySettingsChanged(value) => {
                    assert_eq!(value.floating_boards, view);
                    saw_display = true;
                }
                AppEvent::FloatingBoardsChanged(value) => {
                    assert!(saw_display);
                    assert_eq!(value, view);
                    break;
                }
                _ => {}
            }
        }
    })
    .await;
    for kind in [
        WindowKind::Main,
        WindowKind::FloatingMain,
        WindowKind::FloatingSub,
    ] {
        within(app.save_window_placement(kind, placement()))
            .await
            .unwrap();
    }
    let saved = within(app.settings()).await.unwrap().display;
    assert_eq!(saved.main_window, Some(placement()));
    assert_eq!(saved.floating_main, Some(placement().bounds));
    assert_eq!(saved.floating_sub, Some(placement().bounds));
    assert_eq!(saved.layout_mode, display.layout_mode);
    assert_eq!(saved.custom_layouts, display.custom_layouts);
    assert!(!saved.show_comment && !saved.confirm_new_board);
    assert_eq!(
        within(Settings::load(directory.path()))
            .await
            .unwrap()
            .display,
        saved
    );
    within(app.shutdown()).await.unwrap();
    let (app, _events) = within(app_core::spawn(directory.path())).await.unwrap();
    assert_eq!(within(app.settings()).await.unwrap().display, saved);
    let reset = within(app.reset_window_positions()).await.unwrap();
    assert_eq!(
        reset,
        DisplaySettings {
            main_window: None,
            floating_main: None,
            floating_sub: None,
            board_position: 4,
            ..saved
        }
    );
    assert_eq!(
        within(Settings::load(directory.path()))
            .await
            .unwrap()
            .display,
        reset
    );
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn invalid_layout_updates_and_storage_failures_preserve_state_and_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let (app, _events) = within(app_core::spawn(directory.path())).await.unwrap();
    within(app.set_display_settings(DisplaySettings::default()))
        .await
        .unwrap();
    let path = directory.path().join("settings.json");
    let bytes = within(tokio::fs::read(&path)).await.unwrap();
    for display in [
        DisplaySettings {
            board_position: 9,
            ..Default::default()
        },
        DisplaySettings {
            large_sub_board: true,
            large_winrate_graph: true,
            ..Default::default()
        },
        DisplaySettings {
            custom_layouts: [
                Some(CustomLayout {
                    board_position: 20,
                    ..Default::default()
                }),
                None,
            ],
            ..Default::default()
        },
    ] {
        assert!(matches!(
            within(app.set_display_settings(display)).await,
            Err(app_core::AppError::Settings(SettingsError::Display(_)))
        ));
        assert_eq!(within(tokio::fs::read(&path)).await.unwrap(), bytes);
    }
    let invalid = WindowPlacement {
        bounds: WindowBounds {
            width: 0,
            ..placement().bounds
        },
        ..placement()
    };
    assert!(matches!(
        within(app.save_window_placement(WindowKind::Main, invalid)).await,
        Err(app_core::AppError::Settings(SettingsError::Display(
            DisplaySettingsError::InvalidWindowBounds { .. }
        )))
    ));
    assert_eq!(
        within(app.settings()).await.unwrap().display,
        DisplaySettings::default()
    );
    within(tokio::fs::remove_file(&path)).await.unwrap();
    within(tokio::fs::create_dir(&path)).await.unwrap();
    assert!(
        within(app.set_floating_layout(true, true, true))
            .await
            .is_err()
    );
    assert_eq!(
        within(app.settings())
            .await
            .unwrap()
            .display
            .floating_boards,
        FloatingBoardsView::default()
    );
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn damaged_optional_positions_recover_without_losing_other_settings() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    for invalid in [
        json!("corrupt"),
        json!({"x": 0}),
        json!({"x": -100001, "y": 0, "width": 600, "height": 600}),
        json!({"x": 0, "y": 0, "width": 0, "height": 600}),
        json!({"x": 0, "y": 0, "width": 3.5, "height": 600}),
    ] {
        let value = json!({"pondering": false, "display": {
            "layout_mode": "Min", "show_comment": false,
            "main_window": {"bounds": invalid, "maximized": true},
            "floating_main": invalid, "floating_sub": invalid,
            "custom_layouts": [{"board_position": 8, "main_window": {"bounds": invalid}, "floating_main": invalid, "floating_sub": invalid}, null]
        }});
        let bytes = serde_json::to_vec(&value).unwrap();
        within(tokio::fs::write(&path, &bytes)).await.unwrap();
        let settings = within(Settings::load(directory.path())).await.unwrap();
        assert!(!settings.pondering && !settings.display.show_comment);
        assert_eq!(settings.display.layout_mode, LayoutMode::Min);
        assert_eq!(settings.display.main_window, None);
        assert_eq!(settings.display.floating_main, None);
        assert_eq!(settings.display.floating_sub, None);
        let custom = settings.display.custom_layouts[0].as_ref().unwrap();
        assert_eq!(custom.board_position, 8);
        assert_eq!(custom.main_window, WindowPlacement::default());
        assert_eq!((custom.floating_main, custom.floating_sub), (None, None));
        assert_eq!(within(tokio::fs::read(&path)).await.unwrap(), bytes);
        // Actual application startup uses the same tolerant loading path.
        let (app, _events) = within(app_core::spawn(directory.path())).await.unwrap();
        assert_eq!(
            within(app.settings()).await.unwrap().display,
            settings.display
        );
        within(app.shutdown()).await.unwrap();
    }
}

#[tokio::test]
async fn old_files_get_defaults_and_new_layout_snapshots_round_trip() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    for value in [
        json!({"version": 0}),
        json!({"display": {"show_comment": true}}),
    ] {
        within(tokio::fs::write(&path, serde_json::to_vec(&value).unwrap()))
            .await
            .unwrap();
        assert_eq!(
            within(Settings::load(directory.path()))
                .await
                .unwrap()
                .display,
            DisplaySettings::default()
        );
    }
    let display = DisplaySettings {
        custom_layouts: [
            Some(CustomLayout {
                layout_mode: LayoutMode::MultiView,
                main_window: placement(),
                floating_main: Some(placement().bounds),
                floating_sub: Some(placement().bounds),
                board_position: 0,
                large_sub_board: true,
                ..Default::default()
            }),
            Some(CustomLayout {
                layout_mode: LayoutMode::Min,
                large_winrate_graph: true,
                ..Default::default()
            }),
        ],
        ..Default::default()
    };
    let settings = Settings {
        display,
        ..Default::default()
    };
    within(settings.save(directory.path())).await.unwrap();
    assert_eq!(
        within(Settings::load(directory.path())).await.unwrap(),
        settings
    );
}
