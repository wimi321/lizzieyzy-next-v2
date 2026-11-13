use super::*;

#[test]
fn java_depth_presets_and_custom_limits() {
    assert_eq!(WholeGameOptions::default().visits, 1000);
    for (preset, visits) in [
        (WholeGamePreset::Lightning, 1),
        (WholeGamePreset::Quick, 500),
        (WholeGamePreset::Standard, 1000),
        (WholeGamePreset::Deep, 3000),
        (WholeGamePreset::Professional, 10000),
    ] {
        assert_eq!(WholeGameOptions::from(preset).visits, visits);
        WholeGameOptions::from(preset).validate().expect("preset");
    }
    for visits in [1, 2, 499, 500, 501, 1_000_000] {
        WholeGameOptions {
            visits,
            ..Default::default()
        }
        .validate()
        .expect("valid custom");
    }
    for visits in [0, 1_000_001, u64::MAX] {
        assert!(
            WholeGameOptions {
                visits,
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
    assert_eq!(
        serde_json::from_str::<WholeGameOptions>("{}").expect("default JSON"),
        WholeGameOptions::default()
    );
}

#[test]
fn old_options_keep_the_selected_line_and_do_not_force() {
    let options: WholeGameOptions = serde_json::from_str(r#"{"visits":7}"#).unwrap();
    assert_eq!(options.scope, WholeGameScope::Line);
    assert!(!options.force);
    assert_eq!(options.visits, 7);
    let options: WholeGameOptions =
        serde_json::from_str(r#"{"scope":{"type":"Range","data":{}}}"#).unwrap();
    assert_eq!(
        options.scope,
        WholeGameScope::Range {
            first_move: None,
            last_move: None
        }
    );
}
