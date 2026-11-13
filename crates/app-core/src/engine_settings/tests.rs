#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use serde_json::json;

#[test]
fn java_defaults_and_partial_settings_are_preserved() {
    let settings: EngineSettings = serde_json::from_value(json!({})).unwrap();
    assert_eq!(settings, EngineSettings::default());
    assert_eq!(settings.rules, EngineRules::FollowSgf);
    assert!(settings.read_komi && settings.auto_check_alive);
    assert_eq!(settings.default_komi, 7.5);
    assert_eq!(settings.report_interval_ms, 100);
    let partial: EngineSettings = serde_json::from_value(json!({"read_komi": false})).unwrap();
    assert!(!partial.read_komi);
    assert_eq!(partial.default_komi, 7.5);
    assert_eq!(partial.report_interval_ms, 100);
}

#[test]
fn every_java_named_rule_and_custom_editor_rule_becomes_a_query_rule() {
    for name in [
        "chinese",
        "japanese",
        "korean",
        "aga",
        "tromp-taylor",
        "new-zealand",
        "chinese-ogs",
        "chinese-kgs",
        "stone-scoring",
        "bga",
        "aga-button",
        "goe",
        "ancient-territory",
    ] {
        assert_eq!(
            EngineRules::from_java(name).unwrap().query_rules(),
            Some(katago_protocol::Rules::Named(name.into()))
        );
    }
    let custom = json!({"ko":"POSITIONAL","scoring":"AREA","tax":"ALL","suicide":true,"hasButton":false,"whiteHandicapBonus":"N","friendlyPassOk":true});
    let rules = EngineRules::from_java(&custom.to_string()).unwrap();
    assert_eq!(serde_json::to_value(rules.query_rules()).unwrap(), custom);
    assert!(EngineRules::from_java("unknown").is_err());
    assert!(EngineRules::from_java(r#"{"ko":"INVALID"}"#).is_err());
}

#[test]
fn settings_validation_and_evaluation_identity_separate_only_actual_overrides() {
    let mut settings = EngineSettings::default();
    let baseline = settings.evaluation_identity().unwrap();
    settings.default_komi = 6.5;
    settings.auto_check_alive = false;
    settings.report_interval_ms = 500;
    assert_eq!(settings.evaluation_identity().unwrap(), baseline);
    settings.read_komi = false;
    assert_ne!(settings.evaluation_identity().unwrap(), baseline);
    settings.default_komi = f64::NAN;
    assert!(settings.validate().is_err());
    settings.default_komi = 400.0;
    assert!(settings.validate().is_ok());
    settings.report_interval_ms = 0;
    assert!(settings.validate().is_err());
    settings.report_interval_ms = 86_400_001;
    assert!(settings.validate().is_err());
}
