#![allow(clippy::unwrap_used, clippy::expect_used)]

use go_core::{Color, Move, Point};

use crate::{AnalysisKey, Command, GameDocument, GamePosition};

fn position_from_sgf(text: &str) -> GamePosition {
    let collection = sgf::parse(text.as_bytes()).unwrap();
    let document = GameDocument::from_sgf(&collection.games[0]).unwrap();
    let mut current = document.root();
    while let Some(child) = document.node(current).unwrap().children().first() {
        current = *child;
    }
    document.position_at(current).unwrap()
}

fn key_hex(key: AnalysisKey) -> String {
    key.as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn identical_analysis_inputs_in_independent_documents_share_key() {
    let collection = sgf::parse(b"(;SZ[9]KM[7.5]RU[Chinese])").unwrap();
    let mut first = GameDocument::from_sgf(&collection.games[0]).unwrap();
    first
        .apply(Command::Play(Move::Play(
            Color::Black,
            Point { x: 2, y: 2 },
        )))
        .unwrap();
    first
        .apply(Command::SetComment("This does not change analysis.".into()))
        .unwrap();
    let first_position = first.position_at(first.current()).unwrap();
    let second_position = position_from_sgf("(;SZ[9]KM[7.5]RU[Chinese]PB[Other];B[cc]LZ[opaque])");
    assert_eq!(
        first_position.analysis_key(),
        second_position.analysis_key()
    );
}

#[test]
fn different_move_orders_with_identical_stones_have_different_keys() {
    let first = position_from_sgf("(;SZ[9]KM[7.5];B[aa];W[bb];B[cc];W[dd])");
    let second = position_from_sgf("(;SZ[9]KM[7.5];B[cc];W[dd];B[aa];W[bb])");
    assert_eq!(first.board.stones(), second.board.stones());
    assert_eq!(first.board.to_play(), second.board.to_play());
    assert_ne!(first.analysis_key(), second.analysis_key());
}

#[test]
fn komi_dimensions_named_rules_and_turn_affect_keys() {
    let chinese = position_from_sgf("(;SZ[9]KM[7.5]RU[Chinese])");
    let komi = position_from_sgf("(;SZ[9]KM[6.5]RU[Chinese])");
    let dimensions = position_from_sgf("(;SZ[9:13]KM[7.5]RU[Chinese])");
    let japanese = position_from_sgf("(;SZ[9]KM[7.5]RU[Japanese])");
    let korean = position_from_sgf("(;SZ[9]KM[7.5]RU[Korean])");
    let white = position_from_sgf("(;SZ[9]KM[7.5]RU[Chinese]PL[W])");
    assert_ne!(chinese.analysis_key(), komi.analysis_key());
    assert_ne!(chinese.analysis_key(), dimensions.analysis_key());
    assert_ne!(chinese.analysis_key(), japanese.analysis_key());
    assert_eq!(japanese.rules.rules(), korean.rules.rules());
    assert_ne!(japanese.analysis_key(), korean.analysis_key());
    assert_ne!(chinese.analysis_key(), white.analysis_key());
}

#[test]
fn setup_order_unknown_rule_spelling_and_negative_zero_do_not_change_key() {
    let first = position_from_sgf("(;SZ[9]KM[0]RU[mystery rules]AB[aa][cc]AW[bb]PL[W])");
    let second = position_from_sgf("(;SZ[9]KM[-0]RU[Chinese]AW[bb]AB[cc][aa]PL[W])");
    assert_eq!(first.analysis_key(), second.analysis_key());
    let mut reversed = second.clone();
    reversed.initial_stones.reverse();
    assert_eq!(second.analysis_key(), reversed.analysis_key());
}

#[test]
fn initial_setup_and_midgame_setup_are_part_of_key() {
    let first = position_from_sgf("(;SZ[9]KM[7.5]AB[aa];W[bb])");
    let other_setup = position_from_sgf("(;SZ[9]KM[7.5]AB[cc];W[bb])");
    assert_ne!(first.analysis_key(), other_setup.analysis_key());

    let reset = position_from_sgf("(;SZ[9]KM[7.5];B[aa];W[bb];AB[cc];B[dd])");
    let uninterrupted = position_from_sgf("(;SZ[9]KM[7.5]AB[cc];B[aa];W[bb];B[dd])");
    assert_eq!(reset.board.stones(), uninterrupted.board.stones());
    assert_eq!(reset.moves, uninterrupted.moves);
    assert_ne!(reset.analysis_key(), uninterrupted.analysis_key());
}

#[test]
fn passes_and_history_before_setup_remain_distinct() {
    let empty = position_from_sgf("(;SZ[9]KM[7.5])");
    let passes = position_from_sgf("(;SZ[9]KM[7.5];B[];W[])");
    assert_eq!(empty.board.stones(), passes.board.stones());
    assert_eq!(empty.board.to_play(), passes.board.to_play());
    assert_ne!(empty.analysis_key(), passes.analysis_key());

    let first = position_from_sgf("(;SZ[9]KM[7.5];B[aa];W[bb];AE[aa][bb]PL[B])");
    let second = position_from_sgf("(;SZ[9]KM[7.5];B[cc];W[dd];AE[cc][dd]PL[B])");
    assert_eq!(first.initial_stones, second.initial_stones);
    assert_eq!(first.analysis_moves, second.analysis_moves);
    assert_ne!(first.analysis_key(), second.analysis_key());
}

#[test]
fn analysis_key_v1_has_fixed_test_vectors() {
    let empty = position_from_sgf("(;SZ[19]KM[7.5]RU[Chinese])");
    let setup = position_from_sgf("(;SZ[9:13]KM[6.5]RU[Japanese]AB[aa][cc]AW[bb]PL[W];W[dd];B[])");
    assert_eq!(
        key_hex(empty.analysis_key()),
        "95d1819b4e7fa4a73303c9108265b5788a2a5f539fa08a4feba20270d6707cb9"
    );
    assert_eq!(
        key_hex(setup.analysis_key()),
        "04ae86b27dd0ef2778d94e9e3acd4153d0aac2a6551ab278ec0bdf712bcf40eb"
    );
}
