use super::*;
use serde::Deserialize;

fn estimator() -> Estimator {
    Estimator::bundled().unwrap()
}
fn candidate(coordinate: &str, score: f64) -> CandidateInput {
    CandidateInput {
        coordinate: coordinate.into(),
        winrate: 40.0 + score,
        score_lead: Some(score),
        prior: 10.0,
        order: 0,
    }
}
fn input(number: u32, loss: f64) -> MoveInput {
    let candidates = if loss == 0.0 {
        vec![candidate("A9", 10.0), candidate("pass", 9.8)]
    } else {
        vec![candidate("pass", 10.0), candidate("A9", 10.0 - loss)]
    };
    MoveInput {
        move_number: number,
        is_black: number % 2 == 1,
        coordinate: "A9".into(),
        winrate_loss: loss,
        score_loss: Some(loss),
        move_rank: 0,
        candidates,
    }
}
fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() <= 1e-6, "{actual} != {expected}");
}

#[derive(Deserialize)]
struct Expected {
    features: [f64; 29],
    sample_count: usize,
    score_sample_count: usize,
    rank_value: f64,
    prediction_sigma: f64,
    quality_score: f64,
    average_winrate_loss: f64,
    average_score_loss: f64,
    median_score_loss: f64,
    weighted_score_loss: f64,
    average_score_equivalent_loss: f64,
    match_rate: f64,
    first_choice_rate: f64,
    top3_rate: f64,
    top5_rate: f64,
    good_move_rate: f64,
    move_rank_good_move_rate: f64,
    move_rank_counts: [u32; 6],
    bad_move_rate: f64,
    mistake_rate: f64,
    blunder_rate: f64,
    average_difficulty: f64,
    strength_band: Option<String>,
}
#[derive(Deserialize)]
struct Fixture {
    moves: Vec<MoveInput>,
    black: Expected,
    white: Expected,
    overall: Expected,
}
#[test]
fn java_candidate_review_features_and_statistics() {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../../fixtures/strength/java-candidate-review.json"
    ))
    .unwrap();
    let report = estimator().estimate(&fixture.moves);
    for (actual, expected) in [
        (&report.black, &fixture.black),
        (&report.white, &fixture.white),
        (&report.overall, &fixture.overall),
    ] {
        for (a, e) in actual.features.iter().zip(expected.features) {
            close(*a, e);
        }
        assert_eq!(actual.sample_count, expected.sample_count);
        assert_eq!(actual.score_sample_count, expected.score_sample_count);
        close(actual.rank_value.unwrap(), expected.rank_value);
        close(actual.prediction_sigma.unwrap(), expected.prediction_sigma);
        close(actual.quality_score, expected.quality_score);
        close(actual.average_winrate_loss, expected.average_winrate_loss);
        close(
            actual.average_score_loss.unwrap(),
            expected.average_score_loss,
        );
        close(
            actual.median_score_loss.unwrap(),
            expected.median_score_loss,
        );
        close(actual.weighted_score_loss, expected.weighted_score_loss);
        close(
            actual.average_score_equivalent_loss,
            expected.average_score_equivalent_loss,
        );
        close(actual.match_rate, expected.match_rate);
        close(actual.first_choice_rate, expected.first_choice_rate);
        close(actual.top3_rate, expected.top3_rate);
        close(actual.top5_rate, expected.top5_rate);
        close(actual.good_move_rate, expected.good_move_rate);
        close(
            actual.move_rank_good_move_rate,
            expected.move_rank_good_move_rate,
        );
        assert_eq!(actual.move_rank_counts, expected.move_rank_counts);
        close(actual.bad_move_rate, expected.bad_move_rate);
        close(actual.mistake_rate, expected.mistake_rate);
        close(actual.blunder_rate, expected.blunder_rate);
        close(actual.average_difficulty, expected.average_difficulty);
        assert_eq!(actual.confidence, Confidence::Low);
        if let Some(band) = &expected.strength_band {
            assert_eq!(&actual.strength_band, band);
        }
    }
}

#[test]
fn java_strength_profile_bands() {
    let profiles = [
        (
            vec![(26, 0.0), (19, 0.8), (4, 3.0), (2, 7.0)],
            "11d一线职业",
        ),
        (vec![(26, 0.0), (18, 0.6), (11, 1.21), (1, 7.0)], "9d"),
        (vec![(16, 0.0), (22, 0.8), (7, 3.0), (6, 7.0)], "3-4d"),
        (vec![(28, 0.0), (19, 0.8), (11, 2.0), (7, 7.0)], "7d"),
        (vec![(14, 0.0), (16, 0.8), (15, 3.0), (5, 7.0)], "1-2k"),
    ];
    for (groups, band) in profiles {
        let moves: Vec<_> = groups
            .iter()
            .flat_map(|&(n, v)| std::iter::repeat_n(v, n))
            .enumerate()
            .map(|(i, v)| input(i as u32 + 1, v))
            .collect();
        assert_eq!(estimator().estimate(&moves).overall.strength_band, band);
    }
}

#[test]
fn candidate_payload_precedes_fallback_and_missing_score_stays_missing() {
    let mut i = input(1, 5.0);
    i.winrate_loss = 90.0;
    i.score_loss = Some(90.0);
    let s = Sample::from_input(&i).unwrap();
    close(s.winrate_loss, 5.0);
    close(s.score_loss.unwrap(), 5.0);
    i.candidates.iter_mut().for_each(|c| c.score_lead = None);
    let s = Sample::from_input(&i).unwrap();
    assert_eq!(s.score_loss, None);
    close(s.score_equivalent_loss, 5.0 / 6.0);
    i.coordinate = "B9".into();
    let s = Sample::from_input(&i).unwrap();
    close(s.winrate_loss, 90.0);
    close(s.score_loss.unwrap(), 90.0);
    assert_eq!(s.ai_rank, 999);
}

#[test]
fn strict_first_choice_and_exclusive_tiers() {
    let mut i = input(1, 0.05);
    let report = estimator().estimate(&[i.clone()]);
    close(report.black.first_choice_rate, 0.0);
    assert_eq!(report.black.move_rank_counts, [1, 0, 0, 0, 0, 0]);
    assert_eq!(report.black.tier_counts, [0, 1, 0, 0, 0, 0]);
    i.coordinate = "PASS".into();
    let s = Sample::from_input(&i).unwrap();
    assert!(s.first_choice);
    close(s.score_equivalent_loss, 0.0);
}

#[test]
fn phase_boundaries_and_absent_phase_model_fallback() {
    let moves = vec![
        input(60, 0.0),
        input(61, 1.0),
        input(160, 3.0),
        input(161, 7.0),
    ];
    let report = estimator().estimate(&moves).overall;
    assert_eq!(report.opening.sample_count, 1);
    assert_eq!(report.middlegame.sample_count, 2);
    assert_eq!(report.endgame.sample_count, 1);
    close(report.opening.weighted_score_loss, 0.0);
    close(
        report.middlegame.weighted_score_loss,
        (1.0 * 0.5 + 3.0) / 1.5,
    );
    close(report.endgame.weighted_score_loss, 7.0);
    let only = estimator().estimate(&[input(1, 3.0)]).overall;
    close(
        only.middlegame.weighted_score_loss,
        only.weighted_score_loss,
    );
    close(only.middlegame.good_move_rate, only.good_move_rate);
}

#[test]
fn insufficient_data_and_java_confidence_boundaries() {
    let e = estimator();
    let empty = e.estimate(&[]);
    assert!(!empty.has_enough_data);
    assert_eq!(empty.black.rank_value, None);
    assert_eq!(empty.black.strength_band, "-");
    assert_eq!(empty.black.ai_likelihood, 0.0);
    for (count, confidence) in [
        (5, Confidence::Low),
        (6, Confidence::Low),
        (15, Confidence::Low),
        (16, Confidence::Medium),
        (39, Confidence::Medium),
        (40, Confidence::High),
    ] {
        let moves: Vec<_> = (1..=count).map(|n| input(n, 0.0)).collect();
        let report = e.estimate(&moves);
        assert_eq!(report.has_enough_data, count >= 6);
        assert_eq!(report.overall.confidence, confidence);
        assert!(report.overall.rank_value.is_some());
    }
    let mut samples: Vec<_> = (1..=40)
        .map(|n| Sample::from_input(&input(n, 0.0)).unwrap())
        .collect();
    samples
        .iter_mut()
        .take(13)
        .for_each(|s| s.score_loss = None);
    assert_eq!(e.summarize(&samples).confidence, Confidence::Medium);
    samples[12].score_loss = Some(0.0);
    assert_eq!(e.summarize(&samples).confidence, Confidence::High);
}

#[test]
fn finite_input_and_policy_difficulty_guards() {
    let mut i = input(1, 1.0);
    i.winrate_loss = f64::NAN;
    assert!(Sample::from_input(&i).is_none());
    i.winrate_loss = 1.0;
    i.score_loss = Some(f64::INFINITY);
    assert!(Sample::from_input(&i).is_none());
    i.score_loss = Some(1.0);
    i.candidates.iter_mut().for_each(|c| c.prior = 0.0);
    close(Sample::from_input(&i).unwrap().complexity, 1.0 / 1.2);
    i.candidates[1].prior = 10.0;
    i.candidates[1].order = 999;
    close(Sample::from_input(&i).unwrap().complexity, 1.0 / 1.2);
}

#[test]
fn java_rank_formatting_and_scale() {
    for (value, zh, en) in [
        (0.6339170252703988, "业余1.4级", "1.4 kyu"),
        (0.462_926_218_772_769_96, "业余1.5级", "1.5 kyu"),
        (0.999, "业余1.0级", "1.0 kyu"),
        (1.0, "业余1.0段", "1.0 dan"),
        (9.94, "业余9.9段", "9.9 dan"),
        (10.0, "10.0 职业", "10.0 Professional"),
        (11.0, "11.0 一线职业", "11.0 Top professional"),
        (12.0, "12.0 半神/AI", "12.0 Semi-god/AI"),
    ] {
        assert_eq!(format_rank(value, RankLocale::ZhCn), zh);
        assert_eq!(format_rank(value, RankLocale::En), en);
    }
    assert_eq!(format_rank(f64::NAN, RankLocale::En), "-");
    assert_eq!(format_rank(f64::INFINITY, RankLocale::En), "-");
    assert_eq!(format_rank(-30.0, RankLocale::En), "20.0 kyu");
    close(scale_position(-30.0), 0.0);
    close(scale_position(20.0), 1.0);
    assert_eq!(scale_tier(1.0), 1);
    assert_eq!(scale_tier(0.99), 0);
    assert_eq!(scale_tier(12.5), 4);
    assert!((scale_position(2.1) - scale_position(0.7)) * 360.0 >= 18.0);
    let positions: Vec<_> = (-80..=56).map(|n| scale_position(n as f64 / 4.0)).collect();
    assert!(positions.windows(2).all(|p| p[0] <= p[1]));
}

#[test]
fn original_java_model_predictions_for_review_feature_vectors() {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../../fixtures/strength/java-candidate-review.json"
    ))
    .unwrap();
    let oracle: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/strength/java-model-predictions.json"
    ))
    .unwrap();
    let report = estimator().estimate(&fixture.moves);
    for (name, actual) in [
        ("black", report.black),
        ("white", report.white),
        ("overall", report.overall),
    ] {
        let expected = oracle["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == name)
            .unwrap();
        close(
            actual.rank_value.unwrap(),
            expected["rank_value"].as_f64().unwrap(),
        );
        for (a, e) in actual
            .features
            .iter()
            .zip(expected["features"].as_array().unwrap())
        {
            close(*a, e.as_f64().unwrap());
        }
    }
}

#[test]
fn java_formatter_half_up_decimal_boundaries() {
    for (value, expected) in [
        (1.25, "1.3 dan"),
        (1.05, "1.1 dan"),
        (9.95, "10.0 dan"),
        (-1.25, "3.3 kyu"),
        (10.05, "10.1 Professional"),
    ] {
        assert_eq!(format_rank(value, RankLocale::En), expected);
    }
}

#[test]
fn finite_candidate_values_that_overflow_difficulty_do_not_escape_as_nan() {
    let mut i = input(1, 7.0);
    i.candidates.iter_mut().for_each(|c| c.prior = f64::MAX);
    assert!(Sample::from_input(&i).is_none());
    let report = estimator().estimate(&[i]);
    assert_eq!(report.overall.sample_count, 0);
    assert!(serde_json::to_string(&report).is_ok());
}

#[test]
fn normal_display_rank_is_not_a_java_good_move() {
    let mut normal = input(1, 1.0);
    normal.move_rank = 2;
    let report = estimator().estimate(&[normal]).black;
    close(report.good_move_rate, 1.0);
    close(report.move_rank_good_move_rate, 0.0);
    assert_eq!(report.move_rank_counts, [0, 0, 1, 0, 0, 0]);
}
