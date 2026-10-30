#![allow(clippy::expect_used)]

use super::*;
use katago_protocol::MoveInfo;
use serde_json::json;

fn size() -> BoardSize {
    BoardSize::new(19, 19).expect("board size")
}

fn played(color: Color, coordinate: &str) -> Move {
    Move::from_gtp(color, coordinate, size()).expect("valid move")
}

fn candidate(coordinate: &str, order: u32, winrate: f64, score_lead: f64) -> MoveInfo {
    serde_json::from_value(json!({
        "move": coordinate,
        "order": order,
        "visits": 123,
        "winrate": winrate,
        "scoreLead": score_lead,
        "prior": 0.2,
        "pv": [coordinate],
    }))
    .expect("candidate")
}

fn response(visits: u64, winrate: f64, score_lead: f64) -> AnalysisResponse {
    serde_json::from_value(json!({
        "id": "stored",
        "turnNumber": 7,
        "isDuringSearch": false,
        "moveInfos": [],
        "rootInfo": {
            "visits": visits,
            "winrate": winrate,
            "scoreLead": score_lead,
            "utility": 0.0,
            "currentPlayer": "B",
        },
    }))
    .expect("response")
}

fn before() -> AnalysisResponse {
    // A deliberately misleading root and wire-array order ensure that loss
    // uses the engine's order-zero candidate, not the root or first array item.
    let mut response = response(345, 0.01, -100.0);
    response.move_infos = vec![
        candidate("C3", 5, 0.99, 99.0),
        candidate("E5", 2, 0.60, 2.0),
        candidate("D4", 0, 0.75, 4.0),
    ];
    response
}

fn loss(
    color: Color,
    coordinate: &str,
    before: &AnalysisResponse,
    after: &AnalysisResponse,
) -> MoveLoss {
    MoveLoss::between(
        played(color, coordinate),
        size(),
        before,
        after,
        &MoveRankSettings::default(),
    )
    .expect("available loss")
}

#[test]
fn black_loss_uses_best_candidate_before_and_root_after_in_percentage_points() {
    let before = before();
    let mut after = response(678, 0.50, -2.0);
    // The after-position candidate value must not replace its root evaluation.
    after.move_infos = vec![candidate("F6", 0, 0.10, -50.0)];
    let actual = loss(Color::Black, "E5", &before, &after);
    assert_eq!(actual.winrate_loss, 25.0);
    assert_eq!(actual.score_loss, 6.0);
    assert_eq!(actual.rank, MoveRank::Blunder);
    assert!(!actual.is_best);
    assert_eq!(actual.candidate_rank, Some(3));
    assert_eq!(actual.visits, 678);
    assert_eq!(actual.before_visits, 345);
}

#[test]
fn white_loss_reverses_black_perspective_differences() {
    let mut before = before();
    before.move_infos[2].winrate = 0.25;
    before.move_infos[2].score_lead = -4.0;
    let after = response(678, 0.50, 2.0);
    let actual = loss(Color::White, "E5", &before, &after);
    assert_eq!(actual.winrate_loss, 25.0);
    assert_eq!(actual.score_loss, 6.0);
    assert_eq!(actual.rank, MoveRank::Blunder);
    assert!(!actual.is_best);
    assert_eq!(actual.candidate_rank, Some(3));
}

#[test]
fn first_choice_has_zero_loss_even_when_subsequent_evaluation_deteriorates() {
    // Java NodeInfo.getWinrateDiff / getScoreMeanDiff override isBest to zero.
    for (color, after_winrate, after_score) in
        [(Color::Black, 0.01, -100.0), (Color::White, 0.99, 100.0)]
    {
        let actual = loss(
            color,
            "D4",
            &before(),
            &response(678, after_winrate, after_score),
        );
        assert_eq!(actual.winrate_loss, 0.0);
        assert_eq!(actual.score_loss, 0.0);
        assert_eq!(actual.rank, MoveRank::Best);
        assert!(actual.is_best);
        assert_eq!(actual.candidate_rank, Some(1));
    }
}

#[test]
fn absent_candidate_retains_loss_and_explicitly_has_no_rank() {
    let actual = loss(Color::Black, "Q16", &before(), &response(678, 0.50, -2.0));
    assert_eq!(actual.winrate_loss, 25.0);
    assert_eq!(actual.score_loss, 6.0);
    assert_eq!(actual.rank, MoveRank::Blunder);
    assert!(!actual.is_best);
    assert_eq!(actual.candidate_rank, None);
}

#[test]
fn pass_can_be_first_choice_a_ranked_candidate_or_absent() {
    let after = response(678, 0.50, -2.0);
    for color in [Color::Black, Color::White] {
        let mut before = before();
        before.move_infos[2].coordinate = "pass".into();
        let first = loss(color, "pass", &before, &after);
        assert!(first.is_best);
        assert_eq!(first.candidate_rank, Some(1));
        assert_eq!((first.winrate_loss, first.score_loss), (0.0, 0.0));

        before.move_infos[2].coordinate = "D4".into();
        before.move_infos[1].coordinate = "pass".into();
        let ranked = loss(color, "pass", &before, &after);
        assert!(!ranked.is_best);
        assert_eq!(ranked.candidate_rank, Some(3));

        before.move_infos[1].coordinate = "E5".into();
        let absent = loss(color, "pass", &before, &after);
        assert!(!absent.is_best);
        assert_eq!(absent.candidate_rank, None);
    }
}

#[test]
fn non_first_choice_improvements_clamp_each_loss_to_zero() {
    for (color, after_winrate, after_score) in
        [(Color::Black, 0.90, 10.0), (Color::White, 0.10, -10.0)]
    {
        let actual = loss(
            color,
            "E5",
            &before(),
            &response(678, after_winrate, after_score),
        );
        assert!(!actual.is_best);
        assert_eq!((actual.winrate_loss, actual.score_loss), (0.0, 0.0));
        assert_eq!(actual.rank, MoveRank::Best);
    }
    let mixed = loss(Color::Black, "E5", &before(), &response(678, 0.50, 10.0));
    assert_eq!((mixed.winrate_loss, mixed.score_loss), (25.0, 0.0));
    let mixed = loss(Color::White, "E5", &before(), &response(678, 0.50, 10.0));
    assert_eq!((mixed.winrate_loss, mixed.score_loss), (0.0, 6.0));
}

fn unavailable(before: &AnalysisResponse, after: &AnalysisResponse) {
    for coordinate in ["E5", "D4"] {
        assert_eq!(
            MoveLoss::between(
                played(Color::Black, coordinate),
                size(),
                before,
                after,
                &MoveRankSettings::default(),
            ),
            None,
            "unavailable evaluations must not become zero loss for {coordinate}"
        );
    }
}

#[test]
fn missing_candidates_or_zero_computation_make_loss_unavailable() {
    let after = response(678, 0.50, -2.0);
    let mut missing = before();
    missing.move_infos.clear();
    unavailable(&missing, &after);

    let mut zero_before = before();
    zero_before.root_info.visits = 0;
    unavailable(&zero_before, &after);
    let mut zero_best = before();
    zero_best.move_infos[2].visits = 0;
    unavailable(&zero_best, &after);
    unavailable(&before(), &response(0, 0.50, -2.0));

    // The task uses the after-position root; it needs no after candidates.
    assert!(after.move_infos.is_empty());
    assert_eq!(
        loss(Color::Black, "E5", &before(), &after).winrate_loss,
        25.0
    );
}

#[test]
fn nonfinite_or_out_of_range_used_evaluations_make_loss_unavailable() {
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.01, 1.01] {
        let mut before = before();
        before.move_infos[2].winrate = invalid;
        unavailable(&before, &response(678, 0.50, -2.0));
    }
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.01, 1.01] {
        let mut after = response(678, 0.50, -2.0);
        after.root_info.winrate = invalid;
        unavailable(&before(), &after);
    }
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut before = before();
        before.move_infos[2].score_lead = invalid;
        unavailable(&before, &response(678, 0.50, -2.0));
    }
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut after = response(678, 0.50, -2.0);
        after.root_info.score_lead = invalid;
        unavailable(&before(), &after);
    }
}

#[test]
fn invalid_best_coordinate_is_unavailable_and_score_overflow_is_rejected() {
    let mut invalid = before();
    invalid.move_infos[2].coordinate = "not-a-point".into();
    unavailable(&invalid, &response(678, 0.50, -2.0));

    let mut before = before();
    before.move_infos[2].score_lead = f64::MAX;
    let mut after = response(678, 0.50, -2.0);
    after.root_info.score_lead = -f64::MAX;
    assert!(
        MoveLoss::between(
            played(Color::Black, "E5"),
            size(),
            &before,
            &after,
            &MoveRankSettings::default(),
        )
        .is_none()
    );
}
