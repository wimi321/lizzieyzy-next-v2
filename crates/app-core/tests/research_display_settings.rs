mod common;

use app_core::{
    AppError, ControlPanelBounds, DisplaySettings, DisplaySettingsError, PaintingColor, Settings,
    SettingsError,
};
use common::within;

#[tokio::test]
async fn research_display_settings_persist_and_reject_invalid_values_without_changes() {
    let directory = tempfile::tempdir().unwrap();
    let (app, _events) = within(app_core::spawn(directory.path())).await.unwrap();
    let original = DisplaySettings {
        autoplay_interval_ms: 60_000,
        painting_color: PaintingColor::Green,
        control_panel_bounds: Some(ControlPanelBounds {
            x: -1_920,
            y: 50,
            width: 640,
            height: 480,
        }),
        ..DisplaySettings::default()
    };
    assert_eq!(
        within(app.set_display_settings(original.clone()))
            .await
            .unwrap(),
        original
    );
    let path = directory.path().join("settings.json");
    let before = std::fs::read(&path).unwrap();
    for autoplay_interval_ms in [99, 60_001] {
        let invalid = DisplaySettings {
            autoplay_interval_ms,
            ..original.clone()
        };
        assert!(matches!(
            within(app.set_display_settings(invalid)).await,
            Err(AppError::Settings(SettingsError::Display(
                DisplaySettingsError::OutOfRange {
                    field: "autoplay_interval_ms",
                    ..
                }
            )))
        ));
        assert_eq!(within(app.settings()).await.unwrap().display, original);
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    for control_panel_bounds in [
        ControlPanelBounds {
            x: 100_001,
            y: 0,
            width: 640,
            height: 480,
        },
        ControlPanelBounds {
            x: 0,
            y: -100_001,
            width: 640,
            height: 480,
        },
        ControlPanelBounds {
            x: 0,
            y: 0,
            width: 0,
            height: 480,
        },
        ControlPanelBounds {
            x: 0,
            y: 0,
            width: 640,
            height: 100_001,
        },
    ] {
        let invalid = DisplaySettings {
            control_panel_bounds: Some(control_panel_bounds),
            ..original.clone()
        };
        assert!(matches!(
            within(app.set_display_settings(invalid)).await,
            Err(AppError::Settings(SettingsError::Display(
                DisplaySettingsError::InvalidControlPanelBounds { .. }
            )))
        ));
        assert_eq!(within(app.settings()).await.unwrap().display, original);
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    within(app.shutdown()).await.unwrap();
    let (app, _events) = within(app_core::spawn(directory.path())).await.unwrap();
    assert_eq!(within(app.settings()).await.unwrap().display, original);
    let cleared = DisplaySettings {
        control_panel_bounds: None,
        ..original
    };
    assert_eq!(
        within(app.set_display_settings(cleared.clone()))
            .await
            .unwrap(),
        cleared
    );
    within(app.shutdown()).await.unwrap();
    assert_eq!(
        within(Settings::load(directory.path()))
            .await
            .unwrap()
            .display,
        cleared
    );
}

#[tokio::test]
async fn invalid_research_display_settings_are_rejected_on_load() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    for display in [
        serde_json::json!({"autoplay_interval_ms": 0}),
        serde_json::json!({"control_panel_bounds": {"x": 0, "y": 0, "width": 0, "height": 400}}),
    ] {
        std::fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({"display": display})).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            within(Settings::load(directory.path())).await,
            Err(SettingsError::Display(_))
        ));
    }
    std::fs::write(&path, br#"{"display":{"painting_color":"Black"}}"#).unwrap();
    assert!(matches!(
        within(Settings::load(directory.path())).await,
        Err(SettingsError::Json(_))
    ));
}
