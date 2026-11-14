use super::*;
use crate::coach::tests::{position, response};
use go_core::Point;

#[test]
fn normal_distribution_and_missing_spread_match_reference_values() {
    assert!((rank_winrate(0.0, None) - 0.5).abs() < 1e-7);
    assert!((rank_winrate(12.0, None) - 0.841344746).abs() < 1e-7);
    assert!((rank_winrate(-12.0, None) - 0.158655254).abs() < 1e-7);
    assert!((rank_winrate(1.0, Some(0.2)) - 0.841344746).abs() < 1e-7);
    for value in [f64::NAN, f64::INFINITY, -1.0, 0.0] {
        assert_eq!(rank_winrate(12.0, Some(value)), rank_winrate(12.0, None));
    }
}

#[test]
fn four_ordinary_grades_round_percentages_and_decided_score_only_upgrades() {
    assert_eq!(verdict(0.5, 0.456, 0.0), CoachVerdict::Good);
    assert_eq!(verdict(0.5, 0.454, 0.0), CoachVerdict::Slow);
    assert_eq!(verdict(0.5, 0.404, 0.0), CoachVerdict::Problem);
    assert_eq!(verdict(0.5, 0.304, 0.0), CoachVerdict::Blunder);
    assert_eq!(verdict(0.99, 0.98, 10.0), CoachVerdict::Problem);
    assert_eq!(verdict(0.01, 0.009, 20.0), CoachVerdict::Blunder);
    assert_eq!(verdict(0.99, 0.60, 0.0), CoachVerdict::Blunder);
    assert_eq!(verdict(0.50, 0.49, 50.0), CoachVerdict::Good);
    assert_eq!(verdict(0.95, 0.94, 10.0), CoachVerdict::Good);
}

#[test]
fn sparse_candidates_use_after_root_with_black_perspective_for_both_colors() {
    let mut before = response();
    let position = position("(;SZ[9])");
    let mv = Move::Play(Color::Black, Point { x: 2, y: 0 });
    assert_eq!(reviewed_lead(&before, mv, position.board.size()), None);
    assert!(rate_move(&before, None, &position, mv, 3, 1).is_none());
    let mut after = response();
    after.root_info.score_lead = -12.0;
    after.root_info.current_player = katago_protocol::Player::White;
    let rated = rate_move(&before, Some(&after), &position, mv, 3, 1).unwrap();
    assert_eq!(rated.verdict, CoachVerdict::Blunder);
    assert_eq!(rated.points_lost, 12.0);
    assert!(rated.winrate_change < 0.0);
    before.move_infos[2].visits = 16;
    assert_eq!(
        reviewed_lead(&before, mv, position.board.size()),
        Some(-20.0)
    );
    before.move_infos[0].visits = 4000;
    assert_eq!(reviewed_lead(&before, mv, position.board.size()), None);

    let white = crate::coach::tests::position("(;SZ[9]PL[W])");
    after.root_info.score_lead = 12.0;
    let mv = Move::Play(Color::White, Point { x: 2, y: 0 });
    let rated = rate_move(&before, Some(&after), &white, mv, 5, 1).unwrap();
    assert_eq!(rated.verdict, CoachVerdict::Blunder);
    assert_eq!(rated.points_lost, 12.0);
    assert!(rated.winrate_change > 0.0);
    assert!(rated.winrate_after > 0.5);
}

#[test]
fn brilliant_is_best_rare_and_common_move_reliably_loses_five_percent() {
    let position = position("(;SZ[9])");
    let mut response = response();
    let mut policy = vec![0.0; 82];
    policy[0] = 0.05;
    policy[1] = 0.95;
    response.human_policy = Some(policy);
    let mv = Move::Play(Color::Black, Point { x: 0, y: 0 });
    let rated = rate_move(&response, None, &position, mv, 4, 1).unwrap();
    assert_eq!(rated.verdict, CoachVerdict::Brilliant);
    assert_eq!(rated.human_share, Some(0.05));
    assert_eq!(rated.best.mv, mv.into());
    response.move_infos[1].visits = 1;
    assert_eq!(
        rate_move(&response, None, &position, mv, 4, 1)
            .unwrap()
            .verdict,
        CoachVerdict::Good
    );
    response.move_infos[1].visits = 100;
    response.move_infos[1].score_lead = -1.0;
    assert_eq!(
        rate_move(&response, None, &position, mv, 4, 1)
            .unwrap()
            .verdict,
        CoachVerdict::Good
    );
    response.move_infos[1].score_lead = -12.0;
    response.human_policy.as_mut().unwrap()[0] = 0.10;
    assert_eq!(
        rate_move(&response, None, &position, mv, 4, 1)
            .unwrap()
            .verdict,
        CoachVerdict::Good
    );
}
