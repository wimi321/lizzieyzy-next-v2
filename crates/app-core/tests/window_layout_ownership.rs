mod common;
use app_core::{
    AppError, AppEvent, DisplaySettings, DisplaySettingsError, FloatingBoardKind,
    FloatingBoardsView, LayoutMode, Settings, SettingsError, WindowBounds, WindowKind,
    WindowLayout, WindowPlacement,
};
use common::within;

fn layout() -> WindowLayout {
    let bounds = WindowBounds {
        x: -1200,
        y: 50,
        width: 600,
        height: 600,
    };
    WindowLayout {
        main_window: Some(WindowPlacement {
            bounds,
            maximized: true,
        }),
        floating_main: Some(bounds),
        floating_sub: Some(WindowBounds {
            x: 40,
            width: 300,
            height: 300,
            ..bounds
        }),
        floating_boards: FloatingBoardsView {
            main: true,
            sub: true,
            main_on_top: true,
            sub_locked: true,
            ..Default::default()
        },
    }
}

#[tokio::test]
async fn display_setter_ignores_stale_or_invalid_window_fields_before_validation() {
    let directory = tempfile::tempdir().unwrap();
    let (app, _events) = within(app_core::spawn(directory.path())).await.unwrap();
    let stale = within(app.settings()).await.unwrap().display;
    within(app.save_window_layout(layout())).await.unwrap();
    for width in [800, 0, 100_001] {
        let stale_bounds = if width == 800 {
            WindowBounds {
                x: 100,
                y: 100,
                width,
                height: 400,
            }
        } else {
            WindowBounds {
                x: 100_001,
                y: -100_001,
                width,
                height: 0,
            }
        };
        let requested = DisplaySettings {
            layout_mode: LayoutMode::Min,
            board_position: 6,
            show_comment: false,
            main_window: Some(WindowPlacement {
                bounds: stale_bounds,
                maximized: false,
            }),
            floating_main: Some(stale_bounds),
            floating_sub: Some(stale_bounds),
            floating_boards: FloatingBoardsView {
                keep_main_board: true,
                main_locked: true,
                ..Default::default()
            },
            ..stale.clone()
        };
        let saved = within(app.set_display_settings(requested)).await.unwrap();
        assert_eq!(WindowLayout::from(&saved), layout());
        assert_eq!(saved.layout_mode, LayoutMode::Min);
        assert_eq!(saved.board_position, 6);
        assert!(!saved.show_comment);
        assert_eq!(
            within(Settings::load(directory.path()))
                .await
                .unwrap()
                .display,
            saved
        );
    }
    // An old UI omitting the fields deserializes them to defaults; those defaults
    // must not close windows or erase positions acquired after its snapshot.
    let old_ui: DisplaySettings = serde_json::from_str(r#"{"show_coordinates":false}"#).unwrap();
    let saved = within(app.set_display_settings(old_ui)).await.unwrap();
    assert_eq!(WindowLayout::from(&saved), layout());
    assert!(!saved.show_coordinates);
    assert_eq!(saved.layout_mode, LayoutMode::Normal);
    assert!(matches!(
        within(app.set_display_settings(DisplaySettings {
            board_position: 99,
            ..saved.clone()
        }))
        .await,
        Err(AppError::Settings(SettingsError::Display(
            DisplaySettingsError::OutOfRange {
                field: "board_position",
                ..
            }
        )))
    ));
    assert_eq!(within(app.settings()).await.unwrap().display, saved);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn window_layout_commit_replaces_four_fields_together_and_preserves_preferences() {
    let directory = tempfile::tempdir().unwrap();
    let (app, mut events) = within(app_core::spawn(directory.path())).await.unwrap();
    let preferences = DisplaySettings {
        layout_mode: LayoutMode::Thinking,
        large_sub_board: true,
        board_position: 8,
        show_comment: false,
        ..Default::default()
    };
    within(app.set_display_settings(preferences.clone()))
        .await
        .unwrap();
    let saved = within(app.save_window_layout(layout())).await.unwrap();
    assert_eq!(WindowLayout::from(&saved), layout());
    assert_eq!(saved.layout_mode, preferences.layout_mode);
    assert_eq!(saved.large_sub_board, preferences.large_sub_board);
    assert_eq!(saved.board_position, preferences.board_position);
    assert_eq!(saved.show_comment, preferences.show_comment);
    within(async {
        loop {
            if let AppEvent::DisplaySettingsChanged(value) = events.recv().await.unwrap() {
                assert_eq!(value, saved);
                break;
            }
        }
        loop {
            if let AppEvent::FloatingBoardsChanged(value) = events.recv().await.unwrap() {
                assert_eq!(value, layout().floating_boards);
                break;
            }
        }
    })
    .await;
    let cleared = within(app.save_window_layout(WindowLayout::default()))
        .await
        .unwrap();
    assert_eq!(cleared, preferences);
    assert_eq!(
        within(Settings::load(directory.path()))
            .await
            .unwrap()
            .display,
        cleared
    );
    // Native partial commands retain their write authority after the public
    // display setter starts protecting these fields.
    within(app.set_floating_board_open(FloatingBoardKind::Main, true))
        .await
        .unwrap();
    within(app.set_floating_board_options(FloatingBoardKind::Main, true, true))
        .await
        .unwrap();
    within(app.save_window_placement(WindowKind::Main, layout().main_window.unwrap()))
        .await
        .unwrap();
    let partial = within(app.settings()).await.unwrap().display;
    assert!(
        partial.floating_boards.main
            && partial.floating_boards.main_on_top
            && partial.floating_boards.main_locked
    );
    assert_eq!(partial.main_window, layout().main_window);
    assert_eq!(partial.layout_mode, preferences.layout_mode);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn window_layout_validation_and_save_failure_leave_all_fields_unchanged() {
    let directory = tempfile::tempdir().unwrap();
    let (app, _events) = within(app_core::spawn(directory.path())).await.unwrap();
    let original = within(app.save_window_layout(layout())).await.unwrap();
    let path = directory.path().join("settings.json");
    let bytes = within(tokio::fs::read(&path)).await.unwrap();
    let invalid = WindowBounds {
        x: 0,
        y: 0,
        width: 0,
        height: 400,
    };
    for candidate in [
        WindowLayout {
            main_window: Some(WindowPlacement {
                bounds: invalid,
                maximized: false,
            }),
            ..Default::default()
        },
        WindowLayout {
            floating_main: Some(invalid),
            ..Default::default()
        },
        WindowLayout {
            floating_sub: Some(invalid),
            ..Default::default()
        },
    ] {
        assert!(matches!(
            within(app.save_window_layout(candidate)).await,
            Err(AppError::Settings(SettingsError::Display(
                DisplaySettingsError::InvalidWindowBounds { .. }
            )))
        ));
        assert_eq!(within(app.settings()).await.unwrap().display, original);
        assert_eq!(within(tokio::fs::read(&path)).await.unwrap(), bytes);
    }
    within(tokio::fs::remove_file(&path)).await.unwrap();
    within(tokio::fs::create_dir(&path)).await.unwrap();
    assert!(
        within(app.save_window_layout(WindowLayout::default()))
            .await
            .is_err()
    );
    assert_eq!(within(app.settings()).await.unwrap().display, original);
    let preferences = DisplaySettings {
        show_comment: false,
        ..original.clone()
    };
    assert!(within(app.set_display_settings(preferences)).await.is_err());
    assert_eq!(within(app.settings()).await.unwrap().display, original);
    within(app.shutdown()).await.unwrap();
}
