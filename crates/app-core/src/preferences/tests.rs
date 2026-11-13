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
        UiLanguage::ZhTw
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
    for language in [
        UiLanguage::ZhCn,
        UiLanguage::ZhTw,
        UiLanguage::Ja,
        UiLanguage::Ko,
        UiLanguage::Th,
        UiLanguage::En,
        UiLanguage::System,
    ] {
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
    assert!(core.set_always_on_top(true).await.is_err());
    assert!(core.set_ui_scale(1.5).await.is_err());
    assert!(core.set_play_sound(false).await.is_err());
    assert!(core.claim_daily_update_check(100).await.is_err());
    assert_eq!(core.settings().await.unwrap(), Settings::default());
    core.shutdown().await.unwrap();
}

#[test]
fn system_locale_resolves_supported_languages_and_exact_language_subtags() {
    for (locale, expected) in [
        ("zh-TW", UiLanguage::ZhTw),
        ("zh-HK", UiLanguage::ZhTw),
        ("zh-Hant", UiLanguage::ZhTw),
        ("ZH_hant_tw.UTF-8", UiLanguage::ZhTw),
        ("zh_CN.UTF-8", UiLanguage::ZhCn),
        ("zh-Hans", UiLanguage::ZhCn),
        ("zh-SG", UiLanguage::ZhCn),
        ("zh", UiLanguage::ZhCn),
        ("ja", UiLanguage::Ja),
        ("ja-JP", UiLanguage::Ja),
        ("ko_KR", UiLanguage::Ko),
        ("th-TH", UiLanguage::Th),
        ("fr-FR", UiLanguage::En),
        ("zhongwen", UiLanguage::En),
        ("japanese", UiLanguage::En),
        ("", UiLanguage::En),
    ] {
        assert_eq!(UiLanguage::System.resolve_locale(Some(locale)), expected);
        assert_eq!(UiLanguage::Ko.resolve_locale(Some(locale)), UiLanguage::Ko);
    }
}

#[tokio::test]
async fn everyday_preferences_default_round_trip_and_survive_restart() {
    let defaults: Settings = serde_json::from_str("{}").unwrap();
    assert!(!defaults.always_on_top);
    assert_eq!(defaults.ui_scale, 1.0);
    assert!(defaults.play_sound);
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = crate::spawn(directory.path()).await.unwrap();
    let initial = core.settings().await.unwrap();
    assert!(!core.always_on_top().await.unwrap());
    assert_eq!(core.ui_scale().await.unwrap(), 1.0);
    assert!(core.play_sound().await.unwrap());

    assert!(core.set_always_on_top(true).await.unwrap());
    assert_eq!(core.set_ui_scale(1.5).await.unwrap(), 1.5);
    assert!(!core.set_play_sound(false).await.unwrap());
    let expected = Settings {
        always_on_top: true,
        ui_scale: 1.5,
        play_sound: false,
        ..initial
    };
    assert_eq!(core.settings().await.unwrap(), expected);
    assert_eq!(Settings::load(directory.path()).await.unwrap(), expected);
    core.shutdown().await.unwrap();
    let (core, _) = crate::spawn(directory.path()).await.unwrap();
    assert!(core.always_on_top().await.unwrap());
    assert_eq!(core.ui_scale().await.unwrap(), 1.5);
    assert!(!core.play_sound().await.unwrap());
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn ui_scale_validation_preserves_old_memory_and_disk_values() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = crate::spawn(directory.path()).await.unwrap();
    for scale in [0.5, 1.0, 4.0 / 3.0, 5.0 / 3.0, 3.0] {
        assert_eq!(core.set_ui_scale(scale).await.unwrap(), scale);
        assert_eq!(
            Settings::load(directory.path()).await.unwrap().ui_scale,
            scale
        );
    }
    let previous = std::fs::read(directory.path().join("settings.json")).unwrap();
    for scale in [0.49, 3.01, 0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(matches!(
            core.set_ui_scale(scale).await,
            Err(AppError::Settings(crate::SettingsError::InvalidUiScale))
        ));
        assert_eq!(core.ui_scale().await.unwrap(), 3.0);
        assert_eq!(
            std::fs::read(directory.path().join("settings.json")).unwrap(),
            previous
        );
    }
    core.shutdown().await.unwrap();
    std::fs::write(
        directory.path().join("settings.json"),
        br#"{"ui_scale":3.01}"#,
    )
    .unwrap();
    assert!(matches!(
        Settings::load(directory.path()).await,
        Err(crate::SettingsError::InvalidUiScale)
    ));
}

#[test]
fn new_languages_use_java_catalog_names_and_leave_technical_names_unchanged() {
    // Expected values from the three AutoSetup.transformer*Model Java resource keys.
    for (language, names) in [
        (
            UiLanguage::ZhTw,
            [
                "Transformer 10B 輕量版",
                "Transformer 10B 均衡版",
                "Transformer 11B 旗艦版",
            ],
        ),
        (
            UiLanguage::Ja,
            [
                "Transformer 10B 軽量版",
                "Transformer 10B バランス版",
                "Transformer 11B フラッグシップ",
            ],
        ),
        (
            UiLanguage::Ko,
            [
                "Transformer 10B 경량형",
                "Transformer 10B 균형형",
                "Transformer 11B 플래그십",
            ],
        ),
        (
            UiLanguage::Th,
            [
                "Transformer 10B รุ่นเบา",
                "Transformer 10B รุ่นสมดุล",
                "Transformer 11B รุ่นเรือธง",
            ],
        ),
        (
            UiLanguage::En,
            [
                "Transformer 10B Lightweight",
                "Transformer 10B Balanced",
                "Transformer 11B Flagship",
            ],
        ),
    ] {
        for (id, name) in ["b10-lightweight", "b10-balanced", "b11-flagship"]
            .into_iter()
            .zip(names)
        {
            assert_eq!(
                localize_engine_name(EngineDisplayName::Catalog(id.into()), language),
                EngineDisplayName::Text(name.into())
            );
        }
        let text = EngineDisplayName::Text("Transformer B11 · 2026-06-15".into());
        assert_eq!(localize_engine_name(text.clone(), language), text);
    }
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
