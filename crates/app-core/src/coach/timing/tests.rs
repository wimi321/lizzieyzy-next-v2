use super::*;
use crate::coach::tests::{position, response};
use go_core::{Color, Move};

#[tokio::test(start_paused = true)]
async fn pace_ranges_and_minimum_delivery_use_tokio_time() {
    for (pace, min, max) in [
        (CoachPace::Natural, 0.4, 8.0),
        (CoachPace::Fast, 0.25, 1.8),
        (CoachPace::Slow, 0.8, 14.0),
    ] {
        let mut seed = 28;
        for move_number in [0, 8, 100, 1000] {
            for top in [0.0, 0.5, 1.0, f64::NAN] {
                let delay = think_delay(pace, top, move_number, 81, &mut seed);
                assert!((min..=max).contains(&delay.as_secs_f64()));
            }
        }
        let delay = think_delay(pace, 0.5, 25, 81, &mut seed);
        let at = tokio::time::Instant::now() + delay;
        tokio::time::advance(delay - Duration::from_millis(1)).await;
        assert!(tokio::time::Instant::now() < at);
        tokio::time::advance(Duration::from_millis(1)).await;
        assert_eq!(tokio::time::Instant::now(), at);
    }
}

#[test]
fn resignation_waits_for_board_share_and_rank_specific_consecutive_evidence() {
    let mut position = position("(;SZ[9])");
    let mut response = response();
    response.root_info.winrate = 0.005;
    response.root_info.score_lead = -40.0;
    let mut count = 0;
    for _ in 0..30 {
        assert!(!should_resign(
            CoachRank::Kyu(1),
            &response,
            &position,
            &mut count
        ));
    }
    assert_eq!(count, 0);
    position.moves = vec![Move::Pass(Color::Black); 33];
    for _ in 0..19 {
        assert!(!should_resign(
            CoachRank::Dan(1),
            &response,
            &position,
            &mut count
        ));
    }
    assert!(should_resign(
        CoachRank::Dan(1),
        &response,
        &position,
        &mut count
    ));
    count = 0;
    response.root_info.winrate = 0.01;
    response.root_info.score_lead = -20.0;
    for _ in 0..9 {
        assert!(!should_resign(
            CoachRank::Dan(9),
            &response,
            &position,
            &mut count
        ));
    }
    response.root_info.score_lead = -19.9;
    assert!(!should_resign(
        CoachRank::Dan(9),
        &response,
        &position,
        &mut count
    ));
    assert_eq!(count, 0);
    response.root_info.score_lead = -20.0;
    for _ in 0..9 {
        assert!(!should_resign(
            CoachRank::Dan(9),
            &response,
            &position,
            &mut count
        ));
    }
    assert!(should_resign(
        CoachRank::Dan(9),
        &response,
        &position,
        &mut count
    ));
    count = 0;
    response.root_info.winrate = 0.0075;
    response.root_info.score_lead = -30.0;
    for _ in 0..14 {
        assert!(!should_resign(
            CoachRank::Dan(5),
            &response,
            &position,
            &mut count
        ));
    }
    assert!(should_resign(
        CoachRank::Dan(5),
        &response,
        &position,
        &mut count
    ));
    response.root_info.winrate = f64::NAN;
    assert!(!should_resign(
        CoachRank::Dan(5),
        &response,
        &position,
        &mut count
    ));
    assert_eq!(count, 0);
}

#[test]
fn adaptive_verification_obeys_time_reserve_minimum_gain_and_visit_ceiling() {
    assert_eq!(
        adaptive_visits(64, Duration::from_secs(1), Duration::from_secs(10)),
        519
    );
    assert_eq!(
        adaptive_visits(64, Duration::from_secs(1), Duration::from_secs(1)),
        64
    );
    assert_eq!(
        adaptive_visits(64, Duration::ZERO, Duration::from_secs(10)),
        64
    );
    assert_eq!(
        adaptive_visits(64, Duration::from_millis(1), Duration::from_secs(10)),
        4096
    );
    assert_eq!(
        adaptive_visits(4096, Duration::from_millis(1), Duration::from_secs(10)),
        4096
    );
}
