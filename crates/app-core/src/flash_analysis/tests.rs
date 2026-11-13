#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use crate::Settings;

#[test]
fn missing_flash_settings_and_fields_use_java_defaults() {
    for json in ["{}", r#"{"flash_analysis":{}}"#] {
        let settings: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.flash_analysis, FlashAnalysisSettings::default());
    }
    let explicit: Settings = serde_json::from_str(r#"{"flash_analysis":{"visits":37}}"#).unwrap();
    assert_eq!(explicit.flash_analysis.visits, 37);
    assert!(!explicit.flash_analysis.always_override);
    let partial: Settings =
        serde_json::from_str(r#"{"flash_analysis":{"always_override":true}}"#).unwrap();
    assert_eq!(partial.flash_analysis.visits, 1);
    assert!(partial.flash_analysis.always_override);
}

#[tokio::test]
async fn flash_settings_round_trip_and_invalid_values_keep_old_settings() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = crate::spawn(directory.path()).await.unwrap();
    assert_eq!(
        core.flash_analysis().await.unwrap(),
        FlashAnalysisSettings::default()
    );
    let original_rules = core.settings().await.unwrap().engine_settings;
    let chosen = FlashAnalysisSettings {
        visits: 437,
        always_override: true,
    };
    assert_eq!(core.set_flash_analysis(chosen).await.unwrap(), chosen);
    assert_eq!(core.flash_analysis().await.unwrap(), chosen);
    assert_eq!(
        Settings::load(directory.path())
            .await
            .unwrap()
            .flash_analysis,
        chosen
    );
    assert_eq!(
        core.settings().await.unwrap().engine_settings,
        original_rules
    );
    let bytes = tokio::fs::read(directory.path().join("settings.json"))
        .await
        .unwrap();
    for visits in [0, 1_000_001, u64::MAX] {
        assert!(
            core.set_flash_analysis(FlashAnalysisSettings { visits, ..chosen })
                .await
                .is_err()
        );
        assert_eq!(core.flash_analysis().await.unwrap(), chosen);
        assert_eq!(
            tokio::fs::read(directory.path().join("settings.json"))
                .await
                .unwrap(),
            bytes
        );
    }
    core.shutdown().await.unwrap();
    let (core, _) = crate::spawn(directory.path()).await.unwrap();
    assert_eq!(core.flash_analysis().await.unwrap(), chosen);
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn failed_flash_save_preserves_memory_and_existing_target() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = crate::spawn(directory.path()).await.unwrap();
    tokio::fs::create_dir(directory.path().join("settings.json"))
        .await
        .unwrap();
    assert!(
        core.set_flash_analysis(FlashAnalysisSettings {
            visits: 12,
            always_override: true
        })
        .await
        .is_err()
    );
    assert_eq!(
        core.flash_analysis().await.unwrap(),
        FlashAnalysisSettings::default()
    );
    assert!(directory.path().join("settings.json").is_dir());
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn invalid_saved_flash_visits_are_rejected_on_load() {
    let directory = tempfile::tempdir().unwrap();
    tokio::fs::write(
        directory.path().join("settings.json"),
        br#"{"flash_analysis":{"visits":0}}"#,
    )
    .await
    .unwrap();
    assert!(Settings::load(directory.path()).await.is_err());
}
