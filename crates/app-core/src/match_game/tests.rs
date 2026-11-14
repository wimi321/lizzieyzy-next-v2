use super::*;
#[test]
fn elo_and_two_sigma_probability_interval() {
    assert_eq!(EloEstimate::from_results(0, 0, 0), None);
    assert_eq!(EloEstimate::from_results(10, 0, 0), None);
    assert_eq!(EloEstimate::from_results(0, 10, 0), None);
    let balanced = EloEstimate::from_results(50, 50, 0).unwrap();
    assert_eq!(balanced.elo, 0.0);
    assert!((balanced.lower.unwrap() + 70.4365036222725).abs() < 1e-8);
    assert!((balanced.upper.unwrap() - 70.4365036222725).abs() < 1e-8);
    let draw = EloEstimate::from_results(0, 0, 1).unwrap();
    assert_eq!(draw.elo, 0.0);
    assert_eq!(draw.lower, None);
    assert_eq!(draw.upper, None);
    let favorable = EloEstimate::from_results(75, 25, 0).unwrap();
    assert!((favorable.elo - 190.84850188786498).abs() < 1e-8);
}
#[test]
fn standings_track_colors_openings_draws_and_unscored_reasons() {
    let mut view = MatchView::new(6);
    view.current_opening = Some("opening.sgf".into());
    view.record(Some("B+R"), "");
    view.a_is_black = false;
    view.record(Some("W+2.5"), "");
    view.record(Some("B+R"), "");
    view.record(Some("0"), "");
    view.record(None, "超手数");
    view.record(None, "未数子");
    assert_eq!(view.completed, 6);
    assert_eq!(
        view.a,
        MatchWins {
            total: 2,
            black: 1,
            white: 1
        }
    );
    assert_eq!(
        view.b,
        MatchWins {
            total: 1,
            black: 1,
            white: 0
        }
    );
    assert_eq!(view.draws, 1);
    assert_eq!(view.unscored["超手数"], 1);
    assert_eq!(view.unscored["未数子"], 1);
    assert_eq!(view.openings[0].a, view.a);
    assert_eq!(view.openings[0].completed, 6);
    assert!(view.elo.is_some());
}
#[test]
fn defaults_and_input_validation() {
    let mut setup = MatchSetup::default();
    // Defaults are represented independently for the two catalog entries.
    setup.a = serde_json::from_value(serde_json::json!({"type":"KataGo","data":{"engine_id":"a"}}))
        .unwrap();
    setup.b = setup.a.clone();
    setup.validate().unwrap();
    let MatchPlayer::KataGo { play, .. } = &setup.a else {
        panic!()
    };
    assert_eq!(
        play.resign,
        Some(ResignRule {
            after_moves: 0,
            consecutive: 2,
            below_winrate: 0.1
        })
    );
    assert!(setup.swap_colors);
    assert!(setup.auto_save);
    assert_eq!(setup.games, 1);
    setup.games = 0;
    assert!(setup.validate().is_err());
    assert!(validate_params(f64::NAN, 0.0).is_err());
    assert!(validate_params(1.0, f64::INFINITY).is_err());
}
#[test]
fn filenames_have_portable_components() {
    assert_eq!(
        stats::match_file_name(2, "A/B", "B:*", Some("W+R"), ""),
        "2_A_B_vs_B___W+R.sgf"
    );
    assert_eq!(stats::file_component("..."), "Unknown");
    assert!(stats::match_file_name(1, "A", "B", None, "超手数").ends_with("_超手数.sgf"));
}
