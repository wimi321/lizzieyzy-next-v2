#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[test]
fn java_defaults_and_partial_settings_are_stable() {
    let defaults = TsumegoOptions::default();
    assert_eq!(defaults.to_play, TsumegoToPlay::Inherit);
    assert_eq!(defaults.attacker, TsumegoAttacker::Auto);
    assert_eq!(defaults.ko_threat, TsumegoKoThreat::Both);
    assert_eq!(defaults.wall_distance, 1);
    assert_eq!(
        serde_json::from_str::<TsumegoOptions>("{}").unwrap(),
        defaults
    );
    assert_eq!(
        serde_json::from_str::<TsumegoOptions>(r#"{"wall_distance":3}"#).unwrap(),
        TsumegoOptions {
            wall_distance: 3,
            ..defaults
        }
    );
}

#[test]
fn every_option_maps_to_the_pure_algorithm_without_changing_meaning() {
    use go_core::tsumego as core;
    for (value, expected) in [
        (TsumegoToPlay::Inherit, core::TsumegoToPlay::Inherit),
        (TsumegoToPlay::Black, core::TsumegoToPlay::Black),
        (TsumegoToPlay::White, core::TsumegoToPlay::White),
    ] {
        let options = core::TsumegoOptions::from(TsumegoOptions {
            to_play: value,
            ..Default::default()
        });
        assert_eq!(options.to_play, expected);
    }
    for (value, expected) in [
        (TsumegoAttacker::Auto, core::TsumegoAttacker::Auto),
        (TsumegoAttacker::Black, core::TsumegoAttacker::Black),
        (TsumegoAttacker::White, core::TsumegoAttacker::White),
    ] {
        let options = core::TsumegoOptions::from(TsumegoOptions {
            attacker: value,
            ..Default::default()
        });
        assert_eq!(options.attacker, expected);
    }
    for (value, expected) in [
        (TsumegoKoThreat::Both, core::TsumegoKoThreat::Both),
        (TsumegoKoThreat::Attacker, core::TsumegoKoThreat::Attacker),
        (TsumegoKoThreat::Defender, core::TsumegoKoThreat::Defender),
        (TsumegoKoThreat::None, core::TsumegoKoThreat::None),
    ] {
        let options = core::TsumegoOptions::from(TsumegoOptions {
            ko_threat: value,
            ..Default::default()
        });
        assert_eq!(options.ko_threat, expected);
    }
    assert!(matches!(
        TsumegoOptions {
            wall_distance: 0,
            ..Default::default()
        }
        .validate(),
        Err(TsumegoError::InvalidDistance)
    ));
    assert!(
        TsumegoOptions {
            wall_distance: u32::MAX,
            ..Default::default()
        }
        .validate()
        .is_ok()
    );
}

#[tokio::test]
async fn persisted_options_default_old_files_and_reject_invalid_values_atomically() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    for legacy in [
        r#"{}"#,
        r#"{"version":0}"#,
        r#"{"version":1,"tsumego":{"wall_distance":2}}"#,
    ] {
        std::fs::write(&path, legacy).unwrap();
        let settings = crate::Settings::load(directory.path()).await.unwrap();
        assert_eq!(
            settings.tsumego.wall_distance,
            if legacy.contains("distance") { 2 } else { 1 }
        );
        assert_eq!(settings.tsumego.attacker, TsumegoAttacker::Auto);
    }
    let settings = crate::Settings {
        tsumego: TsumegoOptions {
            to_play: TsumegoToPlay::White,
            attacker: TsumegoAttacker::Black,
            ko_threat: TsumegoKoThreat::Defender,
            wall_distance: 4,
        },
        ..Default::default()
    };
    settings.save(directory.path()).await.unwrap();
    assert_eq!(
        crate::Settings::load(directory.path()).await.unwrap(),
        settings
    );
    let saved = std::fs::read(&path).unwrap();
    let mut invalid = settings;
    invalid.tsumego.wall_distance = 0;
    assert!(matches!(
        invalid.save(directory.path()).await,
        Err(crate::SettingsError::Tsumego(TsumegoError::InvalidDistance))
    ));
    assert_eq!(std::fs::read(&path).unwrap(), saved);
    std::fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
    assert!(matches!(
        crate::Settings::load(directory.path()).await,
        Err(crate::SettingsError::Tsumego(TsumegoError::InvalidDistance))
    ));
}
