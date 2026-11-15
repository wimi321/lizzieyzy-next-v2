#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::{BoardSize, RulePreset};
use proptest::prelude::*;

const GOLDEN: [&str; 5] = [
    include_str!("../../../../fixtures/ladder/captured.txt"),
    include_str!("../../../../fixtures/ladder/breaker.txt"),
    include_str!("../../../../fixtures/ladder/countercapture.txt"),
    include_str!("../../../../fixtures/ladder/edge.txt"),
    include_str!("../../../../fixtures/ladder/not-ladder.txt"),
];

fn fixture(text: &str) -> (Position, LadderOutcome) {
    let mut lines = text.lines();
    let mut header = lines.next().unwrap().split_whitespace();
    let player = if header.next().unwrap() == "B" {
        Color::Black
    } else {
        Color::White
    };
    let expected = match header.next().unwrap() {
        "Captured" => LadderOutcome::Captured,
        "Escaped" => LadderOutcome::Escaped,
        _ => LadderOutcome::NotLadder,
    };
    let rows: Vec<_> = lines.collect();
    let size = BoardSize::new(rows[0].len() as u8, rows.len() as u8).unwrap();
    let mut stones = Vec::new();
    for (y, row) in rows.iter().enumerate() {
        for (x, value) in row.chars().enumerate() {
            let color = match value {
                'B' => Color::Black,
                'W' => Color::White,
                _ => continue,
            };
            stones.push((
                color,
                Point {
                    x: x as u8,
                    y: y as u8,
                },
            ));
        }
    }
    (
        Position::with_setup(size, RulePreset::Chinese.rules(), player, stones).unwrap(),
        expected,
    )
}

fn replay(position: &Position, result: &LadderReading) -> Position {
    let mut board = position.clone();
    for mv in &result.moves {
        assert_eq!(mv.color(), board.to_play());
        board.play(*mv).unwrap();
        assert_eq!(board.hash(), board.recompute_hash());
    }
    board
}

#[test]
fn golden_capture_breaker_countercapture_edge_and_not_ladder() {
    for text in GOLDEN {
        let (position, outcome) = fixture(text);
        let before = position.clone();
        let result = read_ladder(&position, position.to_play());
        assert_eq!(result.outcome, outcome, "{text}\n{result:?}");
        replay(&position, &result);
        assert_eq!(position, before);
    }
}

#[test]
fn capture_stops_at_last_atari_and_repeated_continuation_is_empty() {
    let (position, _) = fixture(GOLDEN[0]);
    let result = read_ladder(&position, position.to_play());
    assert!(result.moves.len() > 5);
    let after = replay(&position, &result);
    assert_eq!(after.to_play(), Color::White);
    assert_eq!(after.liberties_at(Point { x: 2, y: 2 }), Some(1));
    assert_eq!(
        read_ladder(&after, after.to_play()),
        LadderReading::terminal(LadderOutcome::Captured)
    );
}

#[test]
fn countercapture_is_preferred_over_losing_extension() {
    let (position, _) = fixture(GOLDEN[2]);
    let result = read_ladder(&position, position.to_play());
    assert_eq!(
        result.moves,
        vec![Move::Play(Color::White, Point { x: 0, y: 0 })]
    );
    let after = replay(&position, &result);
    assert_eq!(after.stone_at(Point { x: 0, y: 1 }), None);
}

#[test]
fn turn_mismatch_and_multiple_targets_are_not_guessed() {
    let (position, _) = fixture(GOLDEN[0]);
    assert_eq!(
        read_ladder(&position, Color::Black).outcome,
        LadderOutcome::NotLadder
    );
    let (position, _) = fixture("W NotLadder\n.B..\nBW..\n.B..\n..BW\n...B\n");
    assert_eq!(
        read_ladder(&position, Color::White).outcome,
        LadderOutcome::NotLadder
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn golden_symmetries_and_colors_preserve_outcome_and_legality(
        index in 0..GOLDEN.len(), rotation in 0u8..4, mirrored in any::<bool>(), swap in any::<bool>()
    ) {
        let (position, expected) = fixture(GOLDEN[index]);
        let size = position.size();
        let color = |c: Color| if swap { c.opponent() } else { c };
        let stones = position.stones().iter().enumerate().filter_map(|(i,c)| c.map(|c| {
            let mut p = at(&position, i);
            if mirrored { p.x = size.width - 1 - p.x; }
            for _ in 0..rotation { p = Point { x: size.width - 1 - p.y, y: p.x }; }
            (color(c),p)
        }));
        let transformed = Position::with_setup(size, position.rules(), color(position.to_play()), stones).unwrap();
        let result = read_ladder(&transformed, transformed.to_play());
        prop_assert_eq!(result.outcome, expected);
        replay(&transformed, &result);
    }

    #[test]
    fn arbitrary_legal_positions_only_produce_legal_bounded_continuations(
        size in 3u8..10,
        moves in prop::collection::vec((0u8..10, 0u8..10), 0..100)
    ) {
        let mut position = Position::new(BoardSize::new(size,size).unwrap(), RulePreset::Chinese.rules()).unwrap();
        for (x,y) in moves {
            let mv = Move::Play(position.to_play(), Point { x:x % size,y:y % size });
            let _ = position.play(mv);
        }
        let before = position.clone();
        let result = read_ladder(&position, position.to_play());
        prop_assert!(result.moves.len() <= position.size().area()*2);
        replay(&position, &result);
        prop_assert_eq!(position,before);
    }
}
