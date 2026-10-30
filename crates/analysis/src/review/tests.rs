#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use document::{Command, GameDocument};
use go_core::{Color, Move, Point, Position};

fn positions(plies: u32) -> Vec<(NodeId, GamePosition)> {
    let mut document = GameDocument::new();
    let mut result = vec![(
        document.root(),
        document.position_at(document.root()).unwrap(),
    )];
    for _ in 0..plies {
        let position = document.position_at(document.current()).unwrap();
        document
            .apply(Command::Play(Move::Pass(position.board.to_play())))
            .unwrap();
        result.push((
            document.current(),
            document.position_at(document.current()).unwrap(),
        ));
    }
    result
}

#[test]
fn batches_the_root_and_sparse_unfinished_turns_at_lower_priority() {
    let positions: Vec<_> = positions(5)
        .into_iter()
        .filter(|(id, _)| matches!(id.0, 0 | 3 | 5))
        .collect();
    let mut batches = ReviewPlan::new(positions.clone(), 1000)
        .unwrap()
        .into_batches();
    assert_eq!(batches.len(), 1);
    let batch = batches.remove(0);
    assert_eq!(batch.request.analyze_turns, Some(vec![0, 3, 5]));
    assert_eq!(batch.request.priority, Some(REVIEW_PRIORITY));
    assert!(batch.request.priority.is_some_and(|priority| priority < 0));
    assert_eq!(batch.request.max_visits, Some(1000));
    assert_eq!(batch.request.moves.len(), 5);
    for (id, position) in positions {
        assert_eq!(batch.nodes[&id.0], vec![(id, position.analysis_key())]);
    }
}

#[test]
fn long_games_respect_session_turn_limit_without_losing_root_or_tail() {
    let batches = ReviewPlan::new(positions(300), 500).unwrap().into_batches();
    assert_eq!(batches.len(), 2);
    assert_eq!(
        batches[0].request.analyze_turns.as_ref().unwrap().len(),
        256
    );
    assert_eq!(batches[1].request.analyze_turns.as_ref().unwrap().len(), 45);
    let turns: Vec<_> = batches
        .iter()
        .flat_map(|batch| batch.request.analyze_turns.clone().unwrap())
        .collect();
    assert_eq!(turns, (0..=300).collect::<Vec<_>>());
}

#[test]
fn empty_nodes_alias_one_turn_and_setup_player_boundaries_get_separate_queries() {
    let mut line = positions(1);
    let mut setup = line[1].1.clone();
    line.push((NodeId(2), setup.clone())); // Comment-only alias after move 1.
    setup.initial_stones = vec![(Color::Black, Point { x: 3, y: 3 })];
    setup.initial_player = Color::White;
    setup.analysis_moves.clear();
    setup.board = Position::with_setup(
        setup.board.size(),
        setup.rules.rules(),
        setup.initial_player,
        setup.initial_stones.clone(),
    )
    .unwrap();
    line.push((NodeId(3), setup.clone()));
    let mv = Move::Pass(Color::White);
    setup.board.play_unchecked_ko(mv).unwrap();
    setup.analysis_moves.push(mv);
    setup.moves.push(mv);
    line.push((NodeId(4), setup.clone()));
    setup.initial_player = Color::White;
    setup.analysis_moves.clear();
    setup.board = Position::with_setup(
        setup.board.size(),
        setup.rules.rules(),
        Color::White,
        setup.initial_stones.clone(),
    )
    .unwrap();
    line.push((NodeId(5), setup));
    let batches = ReviewPlan::new(line, 3000).unwrap().into_batches();
    assert_eq!(batches.len(), 3);
    assert_eq!(batches[0].nodes[&1].len(), 2);
    assert_eq!(batches[1].request.analyze_turns, Some(vec![0, 1]));
    assert_eq!(batches[1].request.moves.len(), 1);
    assert_eq!(batches[1].request.initial_stones.as_ref().unwrap().len(), 1);
    assert_eq!(batches[2].request.analyze_turns, Some(vec![0]));
    assert_eq!(batches[2].nodes[&0][0].0, NodeId(5));
}

#[test]
fn invalid_target_rejects_before_submitting_anything() {
    for target in [0, 499, 1_000_001, u64::MAX] {
        assert!(matches!(
            ReviewPlan::new(positions(0), target),
            Err(AnalysisError::InvalidSettings(_))
        ));
    }
    for target in [500, 1000, 3000, 10_000, 1_000_000] {
        assert!(ReviewPlan::new(positions(0), target).is_ok());
    }
}
