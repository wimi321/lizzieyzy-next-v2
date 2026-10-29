#![allow(clippy::unwrap_used)]

use super::*;

#[test]
fn named_presets_map_to_katago_mechanics_and_strings() {
    let expected = [
        (
            "chinese",
            RulePreset::Chinese,
            KoRule::Simple,
            false,
            ScoringRule::Area,
        ),
        (
            "japanese",
            RulePreset::Japanese,
            KoRule::Simple,
            false,
            ScoringRule::Territory,
        ),
        (
            "korean",
            RulePreset::Korean,
            KoRule::Simple,
            false,
            ScoringRule::Territory,
        ),
        (
            "aga",
            RulePreset::Aga,
            KoRule::SituationalSuperko,
            false,
            ScoringRule::Area,
        ),
        (
            "tromp-taylor",
            RulePreset::TrompTaylor,
            KoRule::PositionalSuperko,
            true,
            ScoringRule::Area,
        ),
        (
            "new-zealand",
            RulePreset::NewZealand,
            KoRule::SituationalSuperko,
            true,
            ScoringRule::Area,
        ),
    ];
    for (name, preset, ko, allow_multi_stone_suicide, scoring) in expected {
        assert_eq!(RulePreset::from_name(name).unwrap(), preset);
        assert_eq!(preset.name(), name);
        assert_eq!(preset.as_katago_str(), name);
        assert_eq!(
            preset.rules(),
            Rules {
                ko,
                allow_multi_stone_suicide,
                scoring
            }
        );
        assert_eq!(Rules::from_name(name).unwrap(), preset.rules());
    }
    assert_ne!(RulePreset::Japanese, RulePreset::Korean);
    assert_eq!(RulePreset::Japanese.rules(), RulePreset::Korean.rules());
}

#[test]
fn unsupported_names_fail_instead_of_falling_back() {
    for name in [
        "",
        "unknown",
        "Chinese",
        " chinese",
        "chinese ",
        "nz",
        "new_zealand",
    ] {
        assert_eq!(RulePreset::from_name(name), Err(UnknownRules(name.into())));
        assert_eq!(Rules::from_name(name), Err(UnknownRules(name.into())));
        assert!(
            Rules::from_name(name)
                .unwrap_err()
                .to_string()
                .contains("unsupported")
        );
    }
}
