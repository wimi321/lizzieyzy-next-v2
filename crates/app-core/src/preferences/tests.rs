#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use crate::Settings;

#[test]
fn old_settings_keep_defaults_and_explicit_languages_resolve() {
    let settings: Settings = serde_json::from_str("{}").unwrap();
    assert_eq!(settings.language, UiLanguage::System);
    assert!(settings.auto_check_updates);
    assert_eq!(settings.last_update_check_unix_ms, None);
    assert_eq!(
        UiLanguage::System.resolve_locale(Some("zh-Hant-TW")),
        UiLanguage::ZhCn
    );
    assert_eq!(
        UiLanguage::System.resolve_locale(Some("en-US")),
        UiLanguage::En
    );
    assert_eq!(UiLanguage::System.resolve_locale(None), UiLanguage::En);
    assert_eq!(UiLanguage::En.resolve_locale(Some("zh-CN")), UiLanguage::En);
    assert!(serde_json::from_str::<Settings>(r#"{"language":"unknown"}"#).is_err());
}

#[tokio::test]
async fn language_and_update_preference_round_trip_without_changing_engine_or_analysis() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = crate::spawn(directory.path()).await.unwrap();
    let baseline = core.settings().await.unwrap();
    for language in [UiLanguage::ZhCn, UiLanguage::En, UiLanguage::System] {
        assert_eq!(core.set_language(language).await.unwrap(), language);
        assert_eq!(
            Settings::load(directory.path()).await.unwrap().language,
            language
        );
        let current = core.settings().await.unwrap();
        assert_eq!(current.engine, baseline.engine);
        assert_eq!(current.pondering, baseline.pondering);
    }
    core.set_auto_check_updates(false).await.unwrap();
    assert!(
        !Settings::load(directory.path())
            .await
            .unwrap()
            .auto_check_updates
    );
    assert!(!core.claim_daily_update_check(100).await.unwrap());
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn daily_check_claim_is_persisted_before_network_and_survives_restarts() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = crate::spawn(directory.path()).await.unwrap();
    const DAY: u64 = 86_400_000;
    assert!(core.claim_daily_update_check(DAY).await.unwrap());
    assert!(!core.claim_daily_update_check(DAY + 1).await.unwrap());
    assert!(!core.claim_daily_update_check(DAY - 1).await.unwrap());
    core.shutdown().await.unwrap();
    let (core, _) = crate::spawn(directory.path()).await.unwrap();
    assert!(!core.claim_daily_update_check(2 * DAY - 1).await.unwrap());
    let (first, second) = tokio::join!(
        core.claim_daily_update_check(2 * DAY),
        core.claim_daily_update_check(2 * DAY)
    );
    assert_ne!(first.unwrap(), second.unwrap());
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn failed_preference_save_keeps_previous_state_and_check_claim() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = crate::spawn(directory.path()).await.unwrap();
    std::fs::create_dir(directory.path().join("settings.json")).unwrap();
    assert!(core.set_language(UiLanguage::En).await.is_err());
    assert!(core.set_auto_check_updates(false).await.is_err());
    assert!(core.claim_daily_update_check(100).await.is_err());
    assert_eq!(core.settings().await.unwrap(), Settings::default());
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn explicit_language_localizes_catalog_names_and_system_keeps_legacy_contract() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = crate::spawn(directory.path()).await.unwrap();
    let profile = EngineProfile {
        model: "b11c768h12nbt3tflrs-fson-silu.bin.gz".into(),
        ..Default::default()
    };
    assert_eq!(
        core.localized_engine_display_name(&profile).await.unwrap(),
        EngineDisplayName::Catalog("b11-flagship".into())
    );
    core.set_language(UiLanguage::ZhCn).await.unwrap();
    assert_eq!(
        core.localized_engine_display_name(&profile).await.unwrap(),
        EngineDisplayName::Text("Transformer 11B 旗舰版".into())
    );
    core.set_language(UiLanguage::En).await.unwrap();
    assert_eq!(
        core.localized_engine_display_name(&profile).await.unwrap(),
        EngineDisplayName::Text("Transformer 11B Flagship".into())
    );
    core.shutdown().await.unwrap();
}
