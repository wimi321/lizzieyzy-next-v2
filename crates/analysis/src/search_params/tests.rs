#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;

#[test]
fn official_search_parameter_bounds_are_finite_and_inclusive() {
    for (pda, noise, temperature) in [(-3.0, 0.0, 0.01), (3.0, 5.0, 100.0)] {
        SearchParams {
            playout_doubling_advantage: Some(pda),
            wide_root_noise: Some(noise),
            root_policy_temperature: Some(temperature),
        }
        .validate()
        .unwrap();
    }
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -3.001, 3.001] {
        assert!(
            SearchParams {
                playout_doubling_advantage: Some(invalid),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
    for invalid in [f64::NAN, -0.01, 5.01] {
        assert!(
            SearchParams {
                wide_root_noise: Some(invalid),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
    for invalid in [f64::NAN, 0.0, 0.009, 100.01] {
        assert!(
            SearchParams {
                root_policy_temperature: Some(invalid),
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
    assert!(SearchParams::default().overrides().is_empty());
}
