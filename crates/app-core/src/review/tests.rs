use super::*;

#[test]
fn java_depth_presets_and_custom_limits() {
    assert_eq!(WholeGameOptions::default().visits, 1000);
    for (preset, visits) in [
        (WholeGamePreset::Quick, 500),
        (WholeGamePreset::Standard, 1000),
        (WholeGamePreset::Deep, 3000),
        (WholeGamePreset::Professional, 10000),
    ] {
        assert_eq!(WholeGameOptions::from(preset).visits, visits);
        WholeGameOptions::from(preset).validate().expect("preset");
    }
    for visits in [500, 501, 1_000_000] {
        WholeGameOptions { visits }
            .validate()
            .expect("valid custom");
    }
    for visits in [0, 499, 1_000_001, u64::MAX] {
        assert!(WholeGameOptions { visits }.validate().is_err());
    }
    assert_eq!(
        serde_json::from_str::<WholeGameOptions>("{}").expect("default JSON"),
        WholeGameOptions::default()
    );
}
