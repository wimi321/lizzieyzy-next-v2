#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::{Move, Rules};
use proptest::prelude::*;

fn options(rule: ScoringRule) -> ScoreOptions {
    ScoreOptions {
        rule,
        preset: RulePreset::Chinese,
        komi: 0.5,
        handicap: 0,
        captured_by: [2, 4],
    }
}

fn position() -> Position {
    Position::with_setup(
        BoardSize::new(3, 3).unwrap(),
        Rules::from_name("chinese").unwrap(),
        Color::Black,
        [
            (Color::Black, Point { x: 0, y: 0 }),
            (Color::White, Point { x: 1, y: 1 }),
        ],
    )
    .unwrap()
}

#[test]
fn area_and_territory_count_living_dead_and_previous_captures() {
    let position = position();
    let original = position.clone();
    let dead = HashSet::from([Point { x: 1, y: 1 }]);
    let area = score(
        &position,
        &dead,
        &HashSet::new(),
        options(ScoringRule::Area),
    )
    .unwrap();
    assert_eq!(area.black.living_stones, 1);
    assert_eq!(area.black.territory, 8);
    assert_eq!(area.white.dead_stones, 1);
    assert_eq!(area.black.captures, 2);
    assert_eq!(area.white.captures, 4);
    assert_eq!(area.black.total, 9.0);
    assert_eq!(area.white.total, 0.5);
    assert_eq!(area.result, "B+8.5");
    assert_eq!(area.ownership, vec![Some(Color::Black); 9]);
    let territory = score(
        &position,
        &dead,
        &HashSet::new(),
        options(ScoringRule::Territory),
    )
    .unwrap();
    assert_eq!(territory.black.total, 11.0);
    assert_eq!(territory.white.total, 4.5);
    assert_eq!(territory.result, "B+6.5");
    assert_eq!(position, original);
}

#[test]
fn shared_empty_regions_and_marked_neutral_points_do_not_score() {
    let position = position();
    let shared = score(
        &position,
        &HashSet::new(),
        &HashSet::new(),
        options(ScoringRule::Area),
    )
    .unwrap();
    assert_eq!(shared.black.territory, 0);
    assert_eq!(shared.white.territory, 0);
    assert_eq!(shared.result, "W+0.5");
    let result = score(
        &position,
        &HashSet::from([Point { x: 1, y: 1 }]),
        &HashSet::from([Point { x: 1, y: 0 }]),
        options(ScoringRule::Area),
    )
    .unwrap();
    assert_eq!(result.ownership[1], None);
    assert_eq!(result.black.territory, 7);
    assert_eq!(result.result, "B+7.5");
}

#[test]
fn handicap_follows_preset_and_only_applies_to_area() {
    let position =
        Position::new(BoardSize::new(9, 9).unwrap(), RulePreset::Chinese.rules()).unwrap();
    for (preset, expected) in [
        (RulePreset::Chinese, 3),
        (RulePreset::Aga, 2),
        (RulePreset::NewZealand, 0),
        (RulePreset::Japanese, 0),
    ] {
        let mut config = options(ScoringRule::Area);
        config.preset = preset;
        config.handicap = 3;
        let result = score(&position, &HashSet::new(), &HashSet::new(), config).unwrap();
        assert_eq!(result.white.handicap_compensation, expected);
        assert_eq!(result.white.total, f64::from(expected) + 0.5);
        config.rule = ScoringRule::Territory;
        let result = score(&position, &HashSet::new(), &HashSet::new(), config).unwrap();
        assert_eq!(result.white.handicap_compensation, 0);
    }
}

#[test]
fn draw_and_decimal_results_are_standard_sgf() {
    let position = position();
    let mut config = options(ScoringRule::Area);
    config.komi = 0.0;
    assert_eq!(
        score(&position, &HashSet::new(), &HashSet::new(), config)
            .unwrap()
            .result,
        "0"
    );
    config.komi = 0.1;
    assert_eq!(
        score(&position, &HashSet::new(), &HashSet::new(), config)
            .unwrap()
            .result,
        "W+0.1"
    );
    config.komi = -0.1;
    assert_eq!(
        score(&position, &HashSet::new(), &HashSet::new(), config)
            .unwrap()
            .result,
        "B+0.1"
    );
}

#[test]
fn group_is_only_orthogonally_connected_same_color() {
    let position = Position::with_setup(
        BoardSize::new(5, 3).unwrap(),
        RulePreset::Chinese.rules(),
        Color::Black,
        [
            (Color::Black, Point { x: 0, y: 0 }),
            (Color::Black, Point { x: 1, y: 0 }),
            (Color::Black, Point { x: 2, y: 1 }),
            (Color::White, Point { x: 0, y: 1 }),
        ],
    )
    .unwrap();
    assert_eq!(
        stone_group(&position, Point { x: 1, y: 0 }),
        vec![Point { x: 0, y: 0 }, Point { x: 1, y: 0 }]
    );
    assert!(stone_group(&position, Point { x: 4, y: 2 }).is_empty());
}

#[test]
fn invalid_inputs_are_rejected_without_mutation() {
    let position = position();
    assert!(matches!(
        score(
            &position,
            &HashSet::from([Point { x: 3, y: 0 }]),
            &HashSet::new(),
            options(ScoringRule::Area)
        ),
        Err(ScoringError::OutOfBounds(_))
    ));
    assert!(matches!(
        score(
            &position,
            &HashSet::from([Point { x: 2, y: 0 }]),
            &HashSet::new(),
            options(ScoringRule::Area)
        ),
        Err(ScoringError::EmptyDeadPoint(_))
    ));
    assert!(matches!(
        score(
            &position,
            &HashSet::new(),
            &HashSet::from([Point { x: 0, y: 0 }]),
            options(ScoringRule::Area)
        ),
        Err(ScoringError::OccupiedNeutralPoint(_))
    ));
    let mut config = options(ScoringRule::Area);
    config.komi = f64::NAN;
    assert_eq!(
        score(&position, &HashSet::new(), &HashSet::new(), config),
        Err(ScoringError::InvalidKomi)
    );
}

proptest! {
    #[test]
    fn legal_positions_partition_area_and_scoring_never_changes_them(
        moves in prop::collection::vec((0u8..5, 0u8..5), 0..100),
        komi in -40i32..40,
    ) {
        let mut position = Position::new(BoardSize::new(5,5).unwrap(), RulePreset::Chinese.rules()).unwrap();
        for (x,y) in moves {
            let _ = position.play(Move::Play(position.to_play(), Point { x, y }));
        }
        let original = position.clone();
        let mut config = options(ScoringRule::Area);
        config.komi = f64::from(komi) / 2.0;
        let result = score(&position, &HashSet::new(), &HashSet::new(), config).unwrap();
        prop_assert_eq!(result.ownership.len(), 25);
        prop_assert!(result.black.living_stones + result.white.living_stones + result.black.territory + result.white.territory <= 25);
        prop_assert_eq!(result.black.total + result.white.total - config.komi, result.ownership.iter().filter(|owner| owner.is_some()).count() as f64);
        prop_assert_eq!(position, original);
    }
}
