#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{
    BoardSize, Color, IllegalMove, KoRule, Move, Point, Position, RulePreset, Rules, ScoringRule,
};
use proptest::prelude::*;
use std::collections::{HashSet, VecDeque};
use std::time::Instant;

const BLACK: Color = Color::Black;
const WHITE: Color = Color::White;
const PRESETS: [RulePreset; 6] = [
    RulePreset::Chinese,
    RulePreset::Japanese,
    RulePreset::Korean,
    RulePreset::Aga,
    RulePreset::TrompTaylor,
    RulePreset::NewZealand,
];

fn point(x: u8, y: u8) -> Point {
    Point { x, y }
}

fn rules(ko: KoRule, allow_multi_stone_suicide: bool) -> Rules {
    Rules {
        ko,
        allow_multi_stone_suicide,
        scoring: ScoringRule::Area,
    }
}

fn empty(width: u8, height: u8, ko: KoRule, allow_multi_stone_suicide: bool) -> Position {
    Position::new(
        BoardSize::new(width, height).unwrap(),
        rules(ko, allow_multi_stone_suicide),
    )
    .unwrap()
}

fn setup(
    width: u8,
    height: u8,
    ko: KoRule,
    allow_multi_stone_suicide: bool,
    black: &[(u8, u8)],
    white: &[(u8, u8)],
) -> Position {
    Position::with_setup(
        BoardSize::new(width, height).unwrap(),
        rules(ko, allow_multi_stone_suicide),
        BLACK,
        black
            .iter()
            .map(|&(x, y)| (BLACK, point(x, y)))
            .chain(white.iter().map(|&(x, y)| (WHITE, point(x, y)))),
    )
    .unwrap()
}

fn assert_points(actual: &[Point], expected: &[(u8, u8)]) {
    let mut actual: Vec<_> = actual.iter().map(|p| (p.x, p.y)).collect();
    let mut expected = expected.to_vec();
    actual.sort_unstable();
    expected.sort_unstable();
    assert_eq!(actual, expected);
}

fn stone_count(position: &Position) -> usize {
    position.stones().iter().flatten().count()
}

fn ko_position(ko: KoRule) -> Position {
    setup(
        5,
        5,
        ko,
        false,
        &[(0, 2), (1, 1), (1, 3)],
        &[(1, 2), (2, 1), (2, 3), (3, 2)],
    )
}

fn assert_rejected_without_mutation(position: &mut Position, mv: Move, expected: IllegalMove) {
    let before = position.clone();
    assert_eq!(position.play(mv), Err(expected));
    assert_eq!(*position, before);
    assert_eq!(position.hash(), position.recompute_hash());
}

#[test]
fn captures_single_connected_group() {
    let mut position = setup(
        5,
        5,
        KoRule::Simple,
        false,
        &[(0, 1), (1, 0), (2, 0), (1, 2), (2, 2)],
        &[(1, 1), (2, 1)],
    );
    let outcome = position.play(Move::Play(BLACK, point(3, 1))).unwrap();
    assert_points(&outcome.captured, &[(1, 1), (2, 1)]);
    assert!(outcome.self_captured.is_empty());
    assert_eq!(stone_count(&position), 6);
    assert_eq!(position.stone_at(point(3, 1)), Some(BLACK));
    assert_eq!(position.stone_at(point(1, 1)), None);
    assert_eq!(position.stone_at(point(2, 1)), None);
}

#[test]
fn captures_four_disconnected_groups_at_once() {
    let mut position = setup(
        5,
        5,
        KoRule::Simple,
        false,
        &[
            (0, 2),
            (1, 1),
            (1, 3),
            (4, 2),
            (3, 1),
            (3, 3),
            (2, 0),
            (2, 4),
        ],
        &[(1, 2), (3, 2), (2, 1), (2, 3)],
    );
    let outcome = position.play(Move::Play(BLACK, point(2, 2))).unwrap();
    assert_points(&outcome.captured, &[(1, 2), (3, 2), (2, 1), (2, 3)]);
    assert!(outcome.self_captured.is_empty());
    assert_eq!(position.liberties_at(point(2, 2)), Some(4));
    assert_eq!(stone_count(&position), 9);
}

#[test]
fn a_group_touching_the_move_twice_is_captured_once() {
    let mut position = setup(
        4,
        4,
        KoRule::Simple,
        false,
        &[(0, 1), (1, 0), (0, 2), (1, 3), (2, 0), (3, 1)],
        &[(1, 1), (1, 2), (2, 1)],
    );
    let outcome = position.play(Move::Play(BLACK, point(2, 2))).unwrap();
    assert_points(&outcome.captured, &[(1, 1), (1, 2), (2, 1)]);
    assert_eq!(stone_count(&position), 7);
}

#[test]
fn snapback_recapture_is_legal_and_captures_the_larger_group() {
    for ko in [
        KoRule::Simple,
        KoRule::PositionalSuperko,
        KoRule::SituationalSuperko,
    ] {
        let mut position = setup(
            5,
            5,
            ko,
            false,
            &[(0, 2), (1, 1), (1, 3), (3, 2)],
            &[(1, 2), (2, 1), (2, 3), (3, 1), (3, 3), (4, 2)],
        );
        let capture = position.play(Move::Play(BLACK, point(2, 2))).unwrap();
        assert_points(&capture.captured, &[(1, 2)]);
        assert_eq!(position.liberties_at(point(2, 2)), Some(1));
        let snapback = position.play(Move::Play(WHITE, point(1, 2))).unwrap();
        assert_points(&snapback.captured, &[(2, 2), (3, 2)]);
        assert_eq!(position.stone_at(point(1, 2)), Some(WHITE));
    }
}

#[test]
fn capture_is_processed_before_the_suicide_check() {
    let mut position = ko_position(KoRule::Simple);
    // All four neighbors are occupied before the move, but capturing grants a liberty.
    let outcome = position.play(Move::Play(BLACK, point(2, 2))).unwrap();
    assert_points(&outcome.captured, &[(1, 2)]);
    assert_eq!(position.liberties_at(point(2, 2)), Some(1));
}

#[test]
fn single_stone_suicide_is_rejected_by_all_presets_and_replay() {
    for preset in PRESETS {
        for color in [BLACK, WHITE] {
            for unchecked_ko in [false, true] {
                let mut position = Position::with_setup(
                    BoardSize::new(5, 5).unwrap(),
                    preset.rules(),
                    color,
                    [(1, 2), (3, 2), (2, 1), (2, 3)].map(|(x, y)| (color.opponent(), point(x, y))),
                )
                .unwrap();
                let before = position.clone();
                let mv = Move::Play(color, point(2, 2));
                let result = if unchecked_ko {
                    position.play_unchecked_ko(mv)
                } else {
                    position.play(mv)
                };
                assert_eq!(
                    result,
                    Err(IllegalMove::Suicide { point: point(2, 2) }),
                    "{preset:?}, {color:?}, unchecked_ko={unchecked_ko}"
                );
                // Includes stones, to-play, incremental hash and repetition history.
                assert_eq!(position, before);
                assert_eq!(position.hash(), position.recompute_hash());
            }
        }
    }
}

#[test]
fn multi_stone_suicide_follows_each_preset_in_checked_play_and_replay() {
    for preset in PRESETS {
        for color in [BLACK, WHITE] {
            for unchecked_ko in [false, true] {
                let opponents = [(0, 1), (1, 0), (1, 2), (2, 0), (3, 1), (2, 2)];
                let mut position = Position::with_setup(
                    BoardSize::new(5, 4).unwrap(),
                    preset.rules(),
                    color,
                    [(color, point(1, 1))]
                        .into_iter()
                        .chain(opponents.map(|(x, y)| (color.opponent(), point(x, y)))),
                )
                .unwrap();
                let before = position.clone();
                let mv = Move::Play(color, point(2, 1));
                let result = if unchecked_ko {
                    position.play_unchecked_ko(mv)
                } else {
                    position.play(mv)
                };
                if matches!(preset, RulePreset::TrompTaylor | RulePreset::NewZealand) {
                    let outcome = result.unwrap();
                    assert!(outcome.captured.is_empty());
                    assert_points(&outcome.self_captured, &[(1, 1), (2, 1)]);
                    assert_eq!(position.stone_at(point(1, 1)), None);
                    assert_eq!(position.stone_at(point(2, 1)), None);
                    for (x, y) in opponents {
                        assert_eq!(position.stone_at(point(x, y)), Some(color.opponent()));
                    }
                    assert_eq!(
                        stone_count(&before) + 1
                            - outcome.captured.len()
                            - outcome.self_captured.len(),
                        stone_count(&position)
                    );
                    assert_eq!(stone_count(&position), 6);
                    assert_eq!(position.to_play(), color.opponent());
                    assert_ne!(position.hash(), before.hash());
                } else {
                    assert_eq!(
                        result,
                        Err(IllegalMove::Suicide { point: point(2, 1) }),
                        "{preset:?}, {color:?}, unchecked_ko={unchecked_ko}"
                    );
                    assert_eq!(position, before);
                }
                assert_eq!(position.hash(), position.recompute_hash());
            }
        }
    }
}

#[test]
fn simple_ko_forbids_immediate_recapture_then_allows_it_after_a_threat() {
    let mut position = ko_position(KoRule::Simple);
    position.play(Move::Play(BLACK, point(2, 2))).unwrap();
    assert_rejected_without_mutation(
        &mut position,
        Move::Play(WHITE, point(1, 2)),
        IllegalMove::Ko {
            point: point(1, 2),
            rule: KoRule::Simple,
        },
    );
    position.play(Move::Play(WHITE, point(4, 4))).unwrap();
    position.play(Move::Play(BLACK, point(0, 4))).unwrap();
    let outcome = position.play(Move::Play(WHITE, point(1, 2))).unwrap();
    assert_points(&outcome.captured, &[(2, 2)]);
}

fn triple_ko_position(ko: KoRule) -> Position {
    let mut stones = Vec::new();
    for (x, center_color) in [(2, WHITE), (7, BLACK), (12, WHITE)] {
        let capturer = center_color.opponent();
        stones.push((center_color, point(x, 2)));
        for p in [point(x - 1, 2), point(x, 1), point(x, 3)] {
            stones.push((capturer, p));
        }
        for p in [point(x + 1, 1), point(x + 1, 3), point(x + 2, 2)] {
            stones.push((center_color, p));
        }
    }
    Position::with_setup(
        BoardSize::new(17, 5).unwrap(),
        rules(ko, false),
        BLACK,
        stones,
    )
    .unwrap()
}

fn triple_ko_cycle() -> [Move; 6] {
    [
        Move::Play(BLACK, point(3, 2)),
        Move::Play(WHITE, point(8, 2)),
        Move::Play(BLACK, point(13, 2)),
        Move::Play(WHITE, point(2, 2)),
        Move::Play(BLACK, point(7, 2)),
        Move::Play(WHITE, point(12, 2)),
    ]
}

#[test]
fn triple_ko_cycles_under_simple_ko_but_not_either_superko_rule() {
    for ko in [
        KoRule::Simple,
        KoRule::PositionalSuperko,
        KoRule::SituationalSuperko,
    ] {
        let mut position = triple_ko_position(ko);
        let initial_stones = position.stones().to_vec();
        let initial_hash = position.hash();
        let moves = triple_ko_cycle();
        for mv in &moves[..5] {
            let outcome = position.play(*mv).unwrap();
            assert_eq!(outcome.captured.len(), 1);
            assert_eq!(position.hash(), position.recompute_hash());
        }
        if ko == KoRule::Simple {
            position.play(moves[5]).unwrap();
            assert_eq!(position.stones(), initial_stones);
            assert_eq!(position.to_play(), BLACK);
            assert_eq!(position.hash(), initial_hash);
            for mv in moves {
                position.play(mv).unwrap();
            }
            assert_eq!(position.hash(), initial_hash);
        } else {
            assert_rejected_without_mutation(
                &mut position,
                moves[5],
                IllegalMove::Ko {
                    point: point(12, 2),
                    rule: ko,
                },
            );
        }
    }
}

#[test]
fn positional_and_situational_superko_distinguish_the_next_player() {
    for ko in [KoRule::PositionalSuperko, KoRule::SituationalSuperko] {
        let mut position = setup(
            5,
            4,
            ko,
            true,
            &[],
            &[(0, 1), (1, 0), (1, 2), (2, 0), (3, 1), (2, 2)],
        );
        let initial_stones = position.stones().to_vec();
        let initial_hash = position.hash();
        let first_move = Move::Play(BLACK, point(1, 1));
        position.play(first_move).unwrap();
        assert_eq!(position.liberties_at(point(1, 1)), Some(1));
        position.play(Move::Pass(WHITE)).unwrap();
        let suicide = Move::Play(BLACK, point(2, 1));
        if ko == KoRule::SituationalSuperko {
            // The two-stone suicide restores the initial stones, but with White
            // to play rather than Black, so only positional superko forbids it.
            let outcome = position.play(suicide).unwrap();
            assert!(outcome.captured.is_empty());
            assert_points(&outcome.self_captured, &[(1, 1), (2, 1)]);
            assert_eq!(position.stones(), initial_stones);
            assert_eq!(position.to_play(), WHITE);
            assert_ne!(position.hash(), initial_hash);
            assert_eq!(position.hash(), position.recompute_hash());
            position.play(Move::Pass(WHITE)).unwrap();
            assert_eq!(position.hash(), initial_hash);
            assert_rejected_without_mutation(
                &mut position,
                first_move,
                IllegalMove::Ko {
                    point: point(1, 1),
                    rule: ko,
                },
            );
        } else {
            assert_rejected_without_mutation(
                &mut position,
                suicide,
                IllegalMove::Ko {
                    point: point(2, 1),
                    rule: ko,
                },
            );
        }
    }
}

#[test]
fn passes_are_always_legal_even_when_they_repeat_a_superko_position() {
    for ko in [
        KoRule::Simple,
        KoRule::PositionalSuperko,
        KoRule::SituationalSuperko,
    ] {
        let mut position = empty(9, 13, ko, false);
        let initial_hash = position.hash();
        for _ in 0..3 {
            for color in [BLACK, WHITE] {
                let outcome = position.play(Move::Pass(color)).unwrap();
                assert!(outcome.captured.is_empty());
                assert!(outcome.self_captured.is_empty());
                assert_eq!(position.to_play(), color.opponent());
                assert_eq!(position.hash(), position.recompute_hash());
                assert_eq!(stone_count(&position), 0);
            }
            assert_eq!(position.hash(), initial_hash);
        }
    }
}

#[test]
fn passes_release_simple_ko_but_do_not_erase_superko_history() {
    for ko in [
        KoRule::Simple,
        KoRule::PositionalSuperko,
        KoRule::SituationalSuperko,
    ] {
        let mut position = ko_position(ko);
        position.play(Move::Play(BLACK, point(2, 2))).unwrap();
        position.play(Move::Pass(WHITE)).unwrap();
        position.play(Move::Pass(BLACK)).unwrap();
        let mv = Move::Play(WHITE, point(1, 2));
        if ko == KoRule::Simple {
            assert_points(&position.play(mv).unwrap().captured, &[(2, 2)]);
        } else {
            assert_rejected_without_mutation(
                &mut position,
                mv,
                IllegalMove::Ko {
                    point: point(1, 2),
                    rule: ko,
                },
            );
        }
    }
}

#[test]
fn unchecked_ko_replays_captures_and_records_history_for_future_checked_moves() {
    for ko in [KoRule::PositionalSuperko, KoRule::SituationalSuperko] {
        let mut position = triple_ko_position(ko);
        let initial_hash = position.hash();
        for mv in triple_ko_cycle() {
            let outcome = position.play_unchecked_ko(mv).unwrap();
            assert_eq!(outcome.captured.len(), 1);
            assert_eq!(position.hash(), position.recompute_hash());
        }
        assert_eq!(position.hash(), initial_hash);
        assert_rejected_without_mutation(
            &mut position,
            triple_ko_cycle()[0],
            IllegalMove::Ko {
                point: point(3, 2),
                rule: ko,
            },
        );
    }
    let mut position = ko_position(KoRule::Simple);
    position.play(Move::Play(BLACK, point(2, 2))).unwrap();
    let outcome = position
        .play_unchecked_ko(Move::Play(WHITE, point(1, 2)))
        .unwrap();
    assert_points(&outcome.captured, &[(2, 2)]);
    assert_rejected_without_mutation(
        &mut position,
        Move::Play(BLACK, point(2, 2)),
        IllegalMove::Ko {
            point: point(2, 2),
            rule: KoRule::Simple,
        },
    );
}

#[test]
fn unchecked_ko_still_checks_occupation_bounds_and_suicide() {
    let mut position = setup(
        5,
        5,
        KoRule::Simple,
        false,
        &[],
        &[(1, 2), (3, 2), (2, 1), (2, 3)],
    );
    for (mv, error) in [
        (
            Move::Play(BLACK, point(1, 2)),
            IllegalMove::Occupied { point: point(1, 2) },
        ),
        (
            Move::Play(BLACK, point(5, 0)),
            IllegalMove::OutOfBounds {
                point: point(5, 0),
                size: position.size(),
            },
        ),
        (
            Move::Play(BLACK, point(2, 2)),
            IllegalMove::Suicide { point: point(2, 2) },
        ),
    ] {
        let before = position.clone();
        assert_eq!(position.play_unchecked_ko(mv), Err(error));
        assert_eq!(position, before);
    }
}

#[test]
fn corners_edges_and_connected_groups_have_unique_liberties() {
    for (p, expected) in [(point(0, 0), 2), (point(0, 2), 3), (point(2, 2), 4)] {
        let mut position = empty(5, 5, KoRule::Simple, false);
        position.play(Move::Play(BLACK, p)).unwrap();
        assert_eq!(position.liberties_at(p), Some(expected));
    }
    let position = setup(5, 5, KoRule::Simple, false, &[(1, 1), (1, 2), (2, 1)], &[]);
    for p in [point(1, 1), point(1, 2), point(2, 1)] {
        // The empty point (2, 2) touches the group twice but counts once.
        assert_eq!(position.liberties_at(p), Some(7));
    }
    assert_eq!(position.liberties_at(point(0, 0)), None);
    assert_eq!(position.liberties_at(point(255, 255)), None);
}

#[test]
fn rectangular_board_indexes_rows_and_checks_each_dimension() {
    for (width, height) in [(2, 25), (25, 2), (9, 13)] {
        let mut position = empty(width, height, KoRule::Simple, false);
        let corner = point(width - 1, height - 1);
        position.play(Move::Play(BLACK, corner)).unwrap();
        assert_eq!(
            position.stones().len(),
            usize::from(width) * usize::from(height)
        );
        assert_eq!(position.stones().last(), Some(&Some(BLACK)));
        assert_eq!(position.stone_at(corner), Some(BLACK));
        assert_eq!(position.liberties_at(corner), Some(2));
        for p in [point(width, 0), point(0, height), point(255, 255)] {
            let size = position.size();
            assert_rejected_without_mutation(
                &mut position,
                Move::Play(WHITE, p),
                IllegalMove::OutOfBounds { point: p, size },
            );
            assert_eq!(position.stone_at(p), None);
        }
        assert_rejected_without_mutation(
            &mut position,
            Move::Play(WHITE, corner),
            IllegalMove::Occupied { point: corner },
        );
    }
}

#[test]
fn setup_rejects_invalid_sizes_duplicates_out_of_bounds_and_dead_groups() {
    let rules = rules(KoRule::Simple, false);
    for (width, height) in [(0, 9), (1, 9), (9, 1), (26, 19), (19, 26)] {
        assert!(Position::new(BoardSize { width, height }, rules).is_err());
    }
    let size = BoardSize::new(3, 3).unwrap();
    for stones in [
        vec![(BLACK, point(3, 0))],
        vec![(BLACK, point(0, 3))],
        vec![(BLACK, point(0, 0)), (BLACK, point(0, 0))],
        vec![(BLACK, point(0, 0)), (WHITE, point(0, 0))],
        vec![
            (BLACK, point(0, 0)),
            (WHITE, point(1, 0)),
            (WHITE, point(0, 1)),
        ],
    ] {
        assert!(Position::with_setup(size, rules, BLACK, stones).is_err());
    }
}

#[test]
fn explicit_move_color_supports_non_alternating_recorded_moves() {
    let mut position = empty(9, 9, KoRule::Simple, false);
    position.play(Move::Play(WHITE, point(2, 2))).unwrap();
    position.play(Move::Play(WHITE, point(3, 2))).unwrap();
    assert_eq!(position.to_play(), BLACK);
    assert_eq!(position.stone_at(point(2, 2)), Some(WHITE));
    assert_eq!(position.stone_at(point(3, 2)), Some(WHITE));
    assert_eq!(position.hash(), position.recompute_hash());
}

#[test]
fn simple_ko_only_restricts_the_recapturing_color() {
    let mut position = ko_position(KoRule::Simple);
    position.play(Move::Play(BLACK, point(2, 2))).unwrap();
    // A recorded same-color move can fill the ko point instead of recapturing.
    let outcome = position.play(Move::Play(BLACK, point(1, 2))).unwrap();
    assert!(outcome.captured.is_empty());
    assert!(outcome.self_captured.is_empty());
    assert_eq!(position.stone_at(point(1, 2)), Some(BLACK));
    assert_eq!(position.to_play(), WHITE);
}

#[test]
fn setup_history_starts_at_setup_instead_of_the_empty_board() {
    let mut position = setup(
        2,
        2,
        KoRule::PositionalSuperko,
        true,
        &[(0, 0), (1, 0), (0, 1)],
        &[],
    );
    // Removing this group first reaches an empty board; it is not a repetition.
    let outcome = position.play(Move::Play(BLACK, point(1, 1))).unwrap();
    assert_eq!(outcome.self_captured.len(), 4);
    assert_eq!(stone_count(&position), 0);
    assert_eq!(position.hash(), position.recompute_hash());
}

#[test]
fn zobrist_is_deterministic_and_depends_on_color_location_size_and_turn() {
    let baseline = empty(9, 13, KoRule::Simple, false);
    assert_eq!(baseline.hash(), empty(9, 13, KoRule::Simple, false).hash());
    assert_ne!(baseline.hash(), empty(13, 9, KoRule::Simple, false).hash());
    let mut hashes = HashSet::new();
    hashes.insert(baseline.hash());
    for color in [BLACK, WHITE] {
        for p in [point(0, 0), point(8, 12)] {
            let mut position = baseline.clone();
            position.play(Move::Play(color, p)).unwrap();
            assert!(hashes.insert(position.hash()));
            let reconstructed = Position::with_setup(
                position.size(),
                position.rules(),
                color.opponent(),
                [(color, p)],
            )
            .unwrap();
            assert_eq!(position.hash(), reconstructed.hash());
            let before = position.hash();
            position.play(Move::Pass(position.to_play())).unwrap();
            assert_ne!(position.hash(), before);
            assert_eq!(position.hash(), position.recompute_hash());
        }
    }
}

// This independent flood fill deliberately uses only the rendered board, not the
// core's group/liberty helpers, so the property can catch a bug in those helpers.
fn every_group_has_a_liberty(position: &Position) -> bool {
    let width = usize::from(position.size().width);
    let height = usize::from(position.size().height);
    let stones = position.stones();
    let mut visited = vec![false; stones.len()];
    for start in 0..stones.len() {
        let Some(color) = stones[start] else {
            continue;
        };
        if visited[start] {
            continue;
        }
        let mut queue = VecDeque::from([start]);
        visited[start] = true;
        let mut liberty_found = false;
        while let Some(index) = queue.pop_front() {
            let x = index % width;
            let y = index / width;
            let candidates = [
                x.checked_sub(1).map(|nx| y * width + nx),
                (x + 1 < width).then_some(y * width + x + 1),
                y.checked_sub(1).map(|ny| ny * width + x),
                (y + 1 < height).then_some((y + 1) * width + x),
            ];
            for next in candidates.into_iter().flatten() {
                match stones[next] {
                    None => liberty_found = true,
                    Some(neighbor) if neighbor == color && !visited[next] => {
                        visited[next] = true;
                        queue.push_back(next);
                    }
                    Some(_) => {}
                }
            }
        }
        if !liberty_found {
            return false;
        }
    }
    true
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn random_legal_sequences_preserve_liberties_incremental_hash_and_stone_accounting(
        width in prop_oneof![3 => 2u8..=12, 1 => 13u8..=25],
        height in prop_oneof![3 => 2u8..=12, 1 => 13u8..=25],
        choices in prop::collection::vec(any::<u16>(), 1..180),
    ) {
        for ko in [KoRule::Simple, KoRule::PositionalSuperko, KoRule::SituationalSuperko] {
            for allow_multi_stone_suicide in [false, true] {
                let mut position = empty(width, height, ko, allow_multi_stone_suicide);
                let area = usize::from(width) * usize::from(height);
                let mut placed = 0usize;
                let mut removed = 0usize;
                for choice in &choices {
                    let index = usize::from(*choice) % (area + 1);
                    let color = position.to_play();
                    let mv = if index == area {
                        Move::Pass(color)
                    } else {
                        Move::Play(color, point((index % usize::from(width)) as u8, (index / usize::from(width)) as u8))
                    };
                    let before = position.clone();
                    match position.play(mv) {
                        Ok(outcome) => {
                            placed += usize::from(matches!(mv, Move::Play(_, _)));
                            removed += outcome.captured.len() + outcome.self_captured.len();
                        }
                        Err(_) => {
                            prop_assert_eq!(&position, &before);
                            // An illegal candidate is replaced by a legal pass, so the
                            // executed sequence contains only legal moves.
                            position.play(Move::Pass(color)).unwrap();
                        }
                    }
                    prop_assert!(every_group_has_a_liberty(&position));
                    prop_assert_eq!(position.hash(), position.recompute_hash());
                    prop_assert!(placed >= removed);
                    prop_assert_eq!(placed - removed, stone_count(&position));
                }
            }
        }
    }
}

fn random_u64(state: &mut u64) -> u64 {
    // Fixed seed and arithmetic make the replay independent of rand's versions.
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut value = *state;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[test]
#[ignore = "manual release timing; no CI performance threshold"]
fn replay_10000_random_legal_moves() {
    // 50 fresh 19x19 games, 200 legal moves each. Generation and legality search
    // are excluded; timing includes fresh positions, captures and superko history.
    let mut random = 0x706c_6179_3130_3030;
    let mut games = Vec::with_capacity(50);
    let mut play_count = 0;
    for _ in 0..50 {
        let mut position = empty(19, 19, KoRule::SituationalSuperko, false);
        let mut moves = Vec::with_capacity(200);
        for _ in 0..200 {
            let color = position.to_play();
            let start = (random_u64(&mut random) % 362) as usize;
            let mut chosen = Move::Pass(color);
            if start < 361 {
                for offset in 0..361 {
                    let index = (start + offset) % 361;
                    let mv = Move::Play(color, point((index % 19) as u8, (index / 19) as u8));
                    if position.play(mv).is_ok() {
                        chosen = mv;
                        play_count += 1;
                        break;
                    }
                }
            }
            if matches!(chosen, Move::Pass(_)) {
                position.play(chosen).unwrap();
            }
            moves.push(chosen);
        }
        games.push((moves, position.hash()));
    }
    assert!(play_count > 9_000);
    let started = Instant::now();
    let mut replayed = 0;
    for (moves, expected_hash) in games {
        let mut position = empty(19, 19, KoRule::SituationalSuperko, false);
        for mv in moves {
            position.play(mv).unwrap();
            replayed += 1;
        }
        assert_eq!(position.hash(), expected_hash);
    }
    let elapsed = started.elapsed();
    assert_eq!(replayed, 10_000);
    println!(
        "replay_10000_random_legal_moves: {elapsed:?}; 50 games, 19x19, \
         situational superko, {play_count} plays, {} passes; generation excluded",
        10_000 - play_count
    );
}
