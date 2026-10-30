#![allow(clippy::expect_used)]

use super::*;

fn settings(mode: MoveRankEvaluationMode) -> MoveRankSettings {
    MoveRankSettings {
        mode,
        ..MoveRankSettings::default()
    }
}

#[test]
fn java_move_rank_definition_test_cases() {
    // Direct positive-loss counterparts of MoveRankDefinitionTest's cases.
    let cases = [
        (MoveRankEvaluationMode::Auto, 6.0, Some(0.4), MoveRank::Best),
        (
            MoveRankEvaluationMode::Auto,
            20.0,
            Some(0.1),
            MoveRank::Mistake,
        ),
        (MoveRankEvaluationMode::Auto, 24.0, None, MoveRank::Blunder),
        (
            MoveRankEvaluationMode::Combined,
            24.0,
            Some(0.4),
            MoveRank::Blunder,
        ),
        (MoveRankEvaluationMode::Score, 24.0, None, MoveRank::Best),
        (
            MoveRankEvaluationMode::Winrate,
            0.5,
            Some(12.0),
            MoveRank::Best,
        ),
    ];
    for (mode, winrate, score, expected) in cases {
        assert_eq!(settings(mode).classify_losses(winrate, score), expected);
    }
}

#[test]
fn all_five_threshold_boundaries_are_inclusive_for_each_explicit_mode() {
    let ranks = [
        MoveRank::Best,
        MoveRank::Good,
        MoveRank::Normal,
        MoveRank::Inaccuracy,
        MoveRank::Mistake,
        MoveRank::Blunder,
    ];
    for mode in [
        MoveRankEvaluationMode::Winrate,
        MoveRankEvaluationMode::Score,
        MoveRankEvaluationMode::Combined,
    ] {
        let settings = settings(mode);
        for index in 0..5 {
            if mode != MoveRankEvaluationMode::Score {
                let threshold = settings.winrate_thresholds[index];
                assert_eq!(
                    settings.classify_losses(threshold - 1e-9, Some(0.0)),
                    ranks[index]
                );
                assert_eq!(
                    settings.classify_losses(threshold, Some(0.0)),
                    ranks[index + 1]
                );
                assert_eq!(
                    settings.classify_losses(threshold + 1e-9, Some(0.0)),
                    ranks[index + 1]
                );
            }
            if mode != MoveRankEvaluationMode::Winrate {
                let threshold = settings.score_thresholds[index];
                assert_eq!(
                    settings.classify_losses(0.0, Some(threshold - 1e-9)),
                    ranks[index]
                );
                assert_eq!(
                    settings.classify_losses(0.0, Some(threshold)),
                    ranks[index + 1]
                );
                assert_eq!(
                    settings.classify_losses(0.0, Some(threshold + 1e-9)),
                    ranks[index + 1]
                );
            }
        }
    }
}

#[test]
fn auto_score_boundaries_and_winrate_safety_guard_match_java() {
    let settings = settings(MoveRankEvaluationMode::Auto);
    for (score, expected) in [
        (0.5, MoveRank::Good),
        (1.5, MoveRank::Normal),
        (3.0, MoveRank::Inaccuracy),
        (6.0, MoveRank::Mistake),
        (12.0, MoveRank::Blunder),
    ] {
        assert_eq!(settings.classify_losses(0.0, Some(score)), expected);
    }
    assert_eq!(
        settings.classify_losses(12.0 - 1e-9, Some(0.0)),
        MoveRank::Best
    );
    assert_eq!(settings.classify_losses(12.0, Some(0.0)), MoveRank::Mistake);
    assert_eq!(
        settings.classify_losses(24.0 - 1e-9, Some(0.0)),
        MoveRank::Mistake
    );
    assert_eq!(settings.classify_losses(24.0, Some(0.0)), MoveRank::Blunder);
    assert_eq!(settings.classify_losses(3.0, None), MoveRank::Normal);
}

#[test]
fn negative_losses_are_improvements_and_custom_thresholds_are_used() {
    for mode in [
        MoveRankEvaluationMode::Auto,
        MoveRankEvaluationMode::Winrate,
        MoveRankEvaluationMode::Score,
        MoveRankEvaluationMode::Combined,
    ] {
        let mut settings = settings(mode);
        assert_eq!(settings.classify_losses(-24.0, Some(-12.0)), MoveRank::Best);
        assert_eq!(settings.classify_losses(0.0, Some(0.0)), MoveRank::Best);
        settings.winrate_thresholds = [10.0, 20.0, 30.0, 40.0, 50.0];
        settings.score_thresholds = [2.0, 4.0, 8.0, 16.0, 32.0];
        settings.validate().expect("custom thresholds");
        assert_eq!(settings.classify_losses(20.0, Some(4.0)), MoveRank::Normal);
        assert_eq!(
            settings.classify_losses(50.0, Some(32.0)),
            MoveRank::Blunder
        );
    }
}

#[test]
fn defaults_and_partial_json_match_java_defaults() {
    let defaults = MoveRankSettings::default();
    assert_eq!(defaults.mode, MoveRankEvaluationMode::Auto);
    assert_eq!(defaults.winrate_thresholds, [1.0, 3.0, 6.0, 12.0, 24.0]);
    assert_eq!(defaults.score_thresholds, [0.5, 1.5, 3.0, 6.0, 12.0]);
    assert_eq!(
        serde_json::from_str::<MoveRankSettings>("{}").expect("defaults"),
        defaults
    );
    assert_eq!(
        serde_json::from_str::<MoveRankSettings>(r#"{"mode":"Combined"}"#)
            .expect("partial settings"),
        settings(MoveRankEvaluationMode::Combined)
    );
    let json = serde_json::to_string(&defaults).expect("serialize");
    assert_eq!(
        serde_json::from_str::<MoveRankSettings>(&json).expect("deserialize"),
        defaults
    );
}

#[test]
fn threshold_validation_rejects_nonfinite_nonpositive_or_unordered_values() {
    for invalid in [
        [0.0, 3.0, 6.0, 12.0, 24.0],
        [-1.0, 3.0, 6.0, 12.0, 24.0],
        [1.0, 3.0, 3.0, 12.0, 24.0],
        [1.0, 6.0, 3.0, 12.0, 24.0],
        [1.0, 3.0, 6.0, 12.0, f64::INFINITY],
        [1.0, f64::NAN, 6.0, 12.0, 24.0],
    ] {
        let settings = MoveRankSettings {
            winrate_thresholds: invalid,
            ..MoveRankSettings::default()
        };
        assert_eq!(
            settings.validate(),
            Err(MoveRankError::InvalidWinrateThresholds)
        );
        let settings = MoveRankSettings {
            score_thresholds: invalid,
            ..MoveRankSettings::default()
        };
        assert_eq!(
            settings.validate(),
            Err(MoveRankError::InvalidScoreThresholds)
        );
    }
    let mut settings = MoveRankSettings::default();
    settings.winrate_thresholds[4] = 100.0;
    settings.validate().expect("inclusive upper limit");
    settings.winrate_thresholds[4] = 100.1;
    assert_eq!(
        settings.validate(),
        Err(MoveRankError::InvalidWinrateThresholds)
    );
}

#[test]
fn severity_levels_and_order_match_java_rank_enum() {
    let ranks = [
        MoveRank::Best,
        MoveRank::Good,
        MoveRank::Normal,
        MoveRank::Inaccuracy,
        MoveRank::Mistake,
        MoveRank::Blunder,
    ];
    for (level, rank) in ranks.iter().enumerate() {
        assert_eq!(usize::from(rank.severity_level()), level);
    }
    assert!(ranks.windows(2).all(|pair| pair[0] < pair[1]));
}
