use super::*;
use crate::coach::tests::{position, response};

fn policy(response: &mut AnalysisResponse, values: &[(usize, f64)]) {
    let mut weights = vec![0.0; 82];
    for (index, probability) in values {
        weights[*index] = *probability;
    }
    response.human_policy = Some(weights);
}

#[test]
fn rare_policy_temperature_preserves_common_odds_and_matches_half_life() {
    assert!((raw_temperature(0, 361) - 0.85).abs() < 1e-12);
    assert!((raw_temperature(80, 361) - 0.775).abs() < 1e-12);
    let mut response = response();
    policy(
        &mut response,
        &[(0, 0.6), (1, 0.39), (2, 0.009), (3, 0.001)],
    );
    let position = position("(;SZ[9])");
    let candidates = policy_candidates(&response, &position, false).unwrap();
    let weights = raw_weights(&candidates, 0.7);
    assert!((weights[0] / weights[1] - 0.6 / 0.39).abs() < 1e-12);
    assert!(weights[2] / weights[0] < 0.009 / 0.6);
    assert!(weights[3] / weights[2] < 0.001 / 0.009);
}

#[test]
fn seeded_raw_policy_rejects_occupied_and_suicide_instead_of_passing() {
    let position = position("(;SZ[9]AB[ii]AW[ba][ab][bc][cb]PL[B])");
    let mut response = response();
    // J1 is occupied; B8 has four white neighbours and is suicide. D9 is legal.
    policy(
        &mut response,
        &[(80, 0.7), (10, 0.29), (3, 0.01), (81, 1.0)],
    );
    let mut seed = 1;
    let first = sample_raw_policy(&response, &position, &mut seed).unwrap();
    assert_eq!(first, Move::Play(Color::Black, Point { x: 3, y: 0 }));
    let mut repeat = 1;
    assert_eq!(
        sample_raw_policy(&response, &position, &mut repeat).unwrap(),
        first
    );
    assert_eq!(repeat, seed);
    policy(&mut response, &[(80, 1.0)]);
    assert_eq!(
        sample_raw_policy(&response, &position, &mut seed),
        Err(CoachError::NoLegalMove)
    );
}

#[test]
fn high_rank_pool_caps_human_points_and_adds_search_and_policy_best() {
    let position = position("(;SZ[9])");
    let mut response = response();
    response.move_infos[0].coordinate = "H8".into();
    let weights: Vec<_> = (0..20).map(|i| (i, 1.0 / 20.0)).collect();
    policy(&mut response, &weights);
    let mut ordinary = vec![0.0; 82];
    ordinary[40] = 1.0;
    response.policy = Some(ordinary);
    let pool = candidate_pool(&response, &position).unwrap();
    assert_eq!(pool.len(), 14);
    assert!(pool.iter().any(|c| c.coordinate == "H8"));
    assert!(pool.iter().any(|c| c.coordinate == "E5"));
    assert!(!pool.iter().any(|c| matches!(c.mv, Move::Pass(_))));
    policy(&mut response, &[(0, 0.96), (1, 0.03), (2, 0.0001)]);
    response.policy = None;
    response.move_infos[0].coordinate = "A9".into();
    assert_eq!(candidate_pool(&response, &position).unwrap().len(), 1);
}

#[test]
fn searched_weights_filter_rank_loss_and_respect_black_perspective() {
    let position = position("(;SZ[9])");
    let mut response = response();
    policy(&mut response, &[(0, 0.05), (1, 0.90), (2, 0.05)]);
    response.move_infos[1].score_lead = -3.5;
    response.move_infos[2].score_lead = -5.0;
    let pool = candidate_pool(&response, &position).unwrap();
    for seed in 0..100 {
        let mv = select_searched_move(
            &pool,
            &response,
            &position,
            CoachRank::Dan(9),
            &mut seed.clone(),
        )
        .unwrap();
        assert_eq!(mv, Move::Play(Color::Black, Point { x: 0, y: 0 }));
    }
    assert!(needs_deeper(
        &pool,
        &response,
        Color::Black,
        CoachRank::Dan(6)
    ));
    for info in &mut response.move_infos {
        info.score_lead = 0.0;
    }
    assert!(!needs_deeper(
        &pool,
        &response,
        Color::Black,
        CoachRank::Dan(6)
    ));
    assert!(needs_deeper(
        &pool,
        &response,
        Color::Black,
        CoachRank::Dan(7)
    ));
    let mut count_b = 0;
    let mut seed = 9;
    for _ in 0..1000 {
        if select_searched_move(&pool, &response, &position, CoachRank::Dan(6), &mut seed).unwrap()
            == Move::Play(Color::Black, Point { x: 1, y: 0 })
        {
            count_b += 1;
        }
    }
    assert!((750..950).contains(&count_b));
    let mut white = position.clone();
    white.board =
        go_core::Position::with_setup(white.board.size(), white.board.rules(), Color::White, [])
            .unwrap();
    for info in &mut response.move_infos {
        info.score_lead = -info.score_lead;
    }
    let pool = candidate_pool(&response, &white).unwrap();
    assert!(select_searched_move(&pool, &response, &white, CoachRank::Dan(6), &mut seed).is_ok());
}

#[test]
fn pass_requires_ordinary_search_and_respects_one_point_tolerance() {
    let early = position("(;SZ[9])");
    let late = position("(;SZ[9];B[])");
    let mut response = response();
    assert!(!pass_check_needed(&early));
    assert!(pass_check_needed(&late));
    response.move_infos[1].coordinate = "pass".into();
    response.move_infos[1].score_lead = 1.0;
    assert_eq!(
        pass_override(&response, &late, 0.0),
        Some(Move::Pass(Color::White))
    );
    response.move_infos[1].score_lead = 1.01;
    assert_eq!(pass_override(&response, &late, 0.0), None);
    assert_eq!(
        pass_override(&response, &late, 0.5),
        Some(Move::Play(Color::White, Point { x: 0, y: 0 }))
    );
    response.move_infos[0].coordinate = "pass".into();
    assert_eq!(
        pass_override(&response, &late, 0.0),
        Some(Move::Pass(Color::White))
    );
    assert_eq!(pass_override(&response, &early, 1.0), None);
}

#[test]
fn searched_selection_never_chooses_pass_by_weight_or_zero_mass_fallback() {
    let position = position("(;SZ[9];B[])");
    let mut response = response();
    response.move_infos.truncate(2);
    response.move_infos[1].coordinate = "pass".into();
    response.move_infos[1].score_lead = 2.0;
    policy(&mut response, &[(0, 0.6), (81, 0.4)]);
    assert_eq!(pass_override(&response, &position, 0.4), None);
    let pool = candidate_pool(&response, &position).unwrap();
    assert!(
        pool.iter()
            .any(|candidate| matches!(candidate.mv, Move::Pass(_)))
    );
    let expected = Move::Play(Color::White, Point { x: 0, y: 0 });
    for rank in [CoachRank::Dan(6), CoachRank::Dan(9)] {
        for mut seed in 0..100 {
            assert_eq!(
                select_searched_move(&pool, &response, &position, rank, &mut seed),
                Ok(expected)
            );
        }
    }

    // Search can add an ordinary point without human mass. Even if verification
    // ranks pass first, its zero-mass fallback must still use the ordinary point.
    policy(&mut response, &[(81, 1.0)]);
    let pool = candidate_pool(&response, &position).unwrap();
    response.move_infos[1].order = 0;
    response.move_infos[0].order = 1;
    assert_eq!(
        select_searched_move(&pool, &response, &position, CoachRank::Dan(9), &mut 1),
        Ok(expected)
    );
    let pass_only: Vec<_> = pool
        .into_iter()
        .filter(|candidate| matches!(candidate.mv, Move::Pass(_)))
        .collect();
    assert_eq!(
        select_searched_move(&pass_only, &response, &position, CoachRank::Dan(9), &mut 1),
        Err(CoachError::NoLegalMove)
    );
}
