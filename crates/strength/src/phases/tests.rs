use super::*;
use crate::MoveInput;

fn moves(last_move: u32) -> Vec<MoveInput> {
    (1..=last_move)
        .map(|move_number| MoveInput {
            move_number,
            is_black: move_number % 2 == 1,
            coordinate: "D4".into(),
            winrate_loss: (move_number % 7) as f64,
            score_loss: Some((move_number % 11) as f64 / 3.0),
            move_rank: (move_number % 6) as u8,
            candidates: Vec::new(),
        })
        .collect()
}

#[test]
fn boundaries_default_and_normalize_close_reversed_and_negative_inputs() {
    assert_eq!(normalize(None), [60, 150]);
    assert_eq!(normalize(Some([60, 150])), [60, 150]);
    assert_eq!(normalize(Some([2, 5])), [10, 20]);
    assert_eq!(normalize(Some([80, 85])), [80, 90]);
    assert_eq!(normalize(Some([150, 60])), [150, 160]);
    assert_eq!(normalize(Some([-100, -1])), [10, 20]);
    assert_eq!(normalize(Some([-1, 50])), [10, 50]);
    assert_eq!(
        normalize(Some([i32::MAX, i32::MIN])),
        [i32::MAX as u32, i32::MAX as u32 + 10]
    );
}

#[test]
fn phases_equal_independent_whole_game_estimates_without_renumbering() {
    let estimator = Estimator::bundled().unwrap();
    let moves = moves(200);
    let report = estimator.estimate(&moves);
    let unchanged = report.clone();
    for bounds in [None, Some([35, 113])] {
        let result = estimator.estimate_phases(&report, bounds, 200);
        for phase in &result.phases {
            let part: Vec<_> = moves
                .iter()
                .filter(|m| m.move_number >= phase.first && m.move_number <= phase.last)
                .cloned()
                .collect();
            let expected = estimator.estimate(&part);
            assert_eq!(phase.black, expected.black);
            assert_eq!(phase.white, expected.white);
            assert!(phase.played && phase.black_ranked && phase.white_ranked);
            assert_eq!(phase.black.samples.first().unwrap().move_number % 2, 1);
            assert_eq!(phase.white.samples.first().unwrap().move_number % 2, 0);
        }
    }
    let result = estimator.estimate_phases(&report, None, 200);
    assert_eq!(result.bounds, [60, 150]);
    assert_eq!(result.phases[0].black.sample_count, 30);
    assert_eq!(result.phases[0].white.sample_count, 30);
    assert_eq!(result.phases[1].black.sample_count, 45);
    assert_eq!(result.phases[1].white.sample_count, 45);
    assert_eq!(result.phases[2].black.sample_count, 25);
    assert_eq!(result.phases[2].white.sample_count, 25);
    // The model's existing phase cut remains 160, independently of presentation phases.
    assert_eq!(report.black.middlegame.sample_count, 50);
    assert_eq!(report.white.endgame.sample_count, 20);
    assert_eq!(report, unchanged);
}

#[test]
fn each_side_needs_six_samples_even_when_the_phase_was_played() {
    let estimator = Estimator::bundled().unwrap();
    let report = estimator.estimate(&moves(11));
    let result = estimator.estimate_phases(&report, None, 11);
    let phase = &result.phases[0];
    assert!(phase.played);
    assert_eq!(phase.black.sample_count, 6);
    assert_eq!(phase.white.sample_count, 5);
    assert!(phase.black_ranked);
    assert!(!phase.white_ranked);
    assert!(
        phase.white.rank_value.is_some(),
        "retain the original summary"
    );
    assert_eq!(
        result.black.best, None,
        "one ranked phase cannot be compared"
    );
    assert!(!result.black.worth_reviewing);
}

#[test]
fn unreached_phases_are_distinct_from_played_but_unanalyzed_phases() {
    let estimator = Estimator::bundled().unwrap();
    let report = estimator.estimate(&moves(120));
    let result = estimator.estimate_phases(&report, None, 120);
    assert_eq!((result.phases[1].first, result.phases[1].last), (61, 120));
    assert!(result.phases[1].played);
    assert_eq!((result.phases[2].first, result.phases[2].last), (151, 120));
    assert!(!result.phases[2].played);
    assert!(!result.phases[2].black_ranked && !result.phases[2].white_ranked);
    assert_eq!(result.phases[2].black.sample_count, 0);
    assert_eq!(result.phases[2].white.rank_value, None);

    let empty_report = estimator.estimate(&[]);
    let unanalyzed = estimator.estimate_phases(&empty_report, None, 200);
    assert!(unanalyzed.phases.iter().all(|phase| phase.played));
    assert!(
        unanalyzed
            .phases
            .iter()
            .all(|phase| !phase.black_ranked && !phase.white_ranked)
    );
    let empty_game = estimator.estimate_phases(&empty_report, None, 0);
    assert!(empty_game.phases.iter().all(|phase| !phase.played));
    let early = estimator.estimate_phases(&empty_report, None, 60);
    assert!(!early.phases[1].played);
    let on_boundary = estimator.estimate_phases(&empty_report, None, 150);
    assert!(!on_boundary.phases[2].played);
}

fn ranked_phases(black: [f64; 3], white: [f64; 3]) -> [PhaseEstimate; 3] {
    let estimator = Estimator::bundled().unwrap();
    let report = estimator.estimate(&moves(200));
    let mut phases = estimator.estimate_phases(&report, None, 200).phases;
    for (index, phase) in phases.iter_mut().enumerate() {
        phase.black.rank_value = Some(black[index]);
        phase.white.rank_value = Some(white[index]);
    }
    phases
}

#[test]
fn extremes_are_per_side_and_even_phases_have_no_markers() {
    let phases = ranked_phases([4.0, 7.0, 3.0], [6.0, 6.1, 6.2]);
    assert_eq!(
        extremes(&phases, true),
        PhaseExtremes {
            best: Some(1),
            worst: Some(2),
            worth_reviewing: true,
        }
    );
    assert_eq!(
        extremes(&phases, false),
        PhaseExtremes {
            best: None,
            worst: None,
            worth_reviewing: false,
        }
    );
    assert_eq!(extremes(&phases[..1], true).best, None);
}

#[test]
fn extremes_include_exact_thresholds_and_preserve_first_ties() {
    let phases = ranked_phases([0.0, 0.3, 0.3], [0.0, 0.6, 0.6]);
    assert_eq!(
        extremes(&phases, true),
        PhaseExtremes {
            best: Some(1),
            worst: Some(0),
            worth_reviewing: false,
        }
    );
    assert_eq!(
        extremes(&phases, false),
        PhaseExtremes {
            best: Some(1),
            worst: Some(0),
            worth_reviewing: true,
        }
    );
    let below = ranked_phases([0.0, 0.299, 0.1], [0.0, 0.599, 0.2]);
    assert_eq!(extremes(&below, true).best, None);
    assert!(!extremes(&below, false).worth_reviewing);
}

#[test]
fn extremes_ignore_insufficient_missing_and_nonfinite_ranks() {
    let mut phases = ranked_phases([0.0, 5.0, 8.0], [0.0, f64::NAN, f64::INFINITY]);
    phases[1].black.sample_count = MIN_SAMPLES - 1;
    phases[2].black.rank_value = None;
    assert!(!ranked(&phases[1].black));
    assert!(!ranked(&phases[2].black));
    assert!(!ranked(&phases[1].white));
    assert!(!ranked(&phases[2].white));
    for is_black in [true, false] {
        assert_eq!(
            extremes(&phases, is_black),
            PhaseExtremes {
                best: None,
                worst: None,
                worth_reviewing: false,
            }
        );
    }
}
