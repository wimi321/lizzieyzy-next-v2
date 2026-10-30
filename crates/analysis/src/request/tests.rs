#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use document::{GameDocument, GamePosition};
use engine_session::AnalysisRequest;
use go_core::{BoardSize, Color, CoordinateError, Move, Point, Position, RulePreset};
use katago_protocol::{Player, Rules};

use super::build;
use crate::{AnalysisError, LiveSettings};

fn empty() -> GamePosition {
    let document = GameDocument::new();
    document.position_at(document.root()).unwrap()
}

fn color(player: Player) -> Color {
    match player {
        Player::Black => Color::Black,
        Player::White => Color::White,
    }
}

/// Execute the emitted query's setup and moves with the domain board. This
/// verifies the replay contract, rather than just comparing request fields.
fn replay(request: &AnalysisRequest, rules: RulePreset) -> Position {
    let size = BoardSize::new(request.board_x_size, request.board_y_size).unwrap();
    let stones = request
        .initial_stones
        .as_ref()
        .unwrap()
        .iter()
        .map(|(player, point)| (color(*player), Point::from_gtp(point, size).unwrap()));
    let mut board = Position::with_setup(
        size,
        rules.rules(),
        color(request.initial_player.unwrap()),
        stones,
    )
    .unwrap();
    for (player, coordinate) in &request.moves {
        let mv = Move::from_gtp(color(*player), coordinate, size).unwrap();
        board.play_unchecked_ko(mv).unwrap();
    }
    board
}

#[test]
fn defaults_describe_the_current_empty_position_and_live_reporting() {
    let position = empty();
    let request = build(&position, &LiveSettings::default()).unwrap();
    assert_eq!(request.moves, []);
    assert_eq!(request.initial_stones, Some(vec![]));
    assert_eq!(request.initial_player, Some(Player::Black));
    assert_eq!((request.board_x_size, request.board_y_size), (19, 19));
    assert_eq!(request.rules, Rules::Named("chinese".to_owned()));
    assert_eq!(request.komi, Some(7.5));
    assert_eq!(request.max_visits, Some(u64::from(u32::MAX)));
    assert_eq!(request.report_during_search_every, Some(0.15));
    assert_eq!(request.first_report_during_search_after, Some(0.15));
    assert_eq!(request.include_ownership, Some(true));
    assert_eq!(request.include_policy, Some(false));
    assert_eq!(request.analyze_turns, None);
    assert_eq!(request.priority, Some(0));
    assert_eq!(request.priorities, None);
    assert_eq!(request.override_settings, None);
    assert!(request.extensions.is_empty());
    assert_eq!(replay(&request, position.rules), position.board);
}

#[test]
fn rectangular_handicap_replays_from_initial_white_not_current_black() {
    let size = BoardSize::new(13, 9).unwrap();
    let mut position = empty();
    position.rules = RulePreset::Japanese;
    position.rules_raw = Some("untrusted original spelling".to_owned());
    position.komi = -0.5;
    position.handicap = 2;
    position.initial_stones = vec![
        (Color::Black, Point { x: 3, y: 2 }),
        (Color::Black, Point { x: 9, y: 6 }),
    ];
    position.initial_player = Color::White;
    position.board = Position::with_setup(
        size,
        position.rules.rules(),
        position.initial_player,
        position.initial_stones.clone(),
    )
    .unwrap();
    let mv = Move::Pass(Color::White);
    position.board.play_unchecked_ko(mv).unwrap();
    position.moves.push(mv);
    position.analysis_moves.push(mv);
    let settings = LiveSettings {
        max_visits: 1234,
        report_interval: Duration::from_millis(10),
        include_ownership: false,
        include_policy: true,
    };
    let request = build(&position, &settings).unwrap();
    assert_eq!(request.initial_player, Some(Player::White));
    assert_eq!(position.board.to_play(), Color::Black);
    assert_eq!(request.moves, [(Player::White, "pass".to_owned())]);
    assert_eq!(
        request.initial_stones,
        Some(vec![
            (Player::Black, "D7".to_owned()),
            (Player::Black, "K3".to_owned()),
        ])
    );
    assert_eq!((request.board_x_size, request.board_y_size), (13, 9));
    assert_eq!(request.rules, Rules::Named("japanese".to_owned()));
    assert_eq!(request.komi, Some(-0.5));
    assert_eq!(request.max_visits, Some(1234));
    assert_eq!(request.report_during_search_every, Some(0.01));
    assert_eq!(request.first_report_during_search_after, Some(0.01));
    assert_eq!(request.include_ownership, Some(false));
    assert_eq!(request.include_policy, Some(true));
    assert_eq!(replay(&request, position.rules), position.board);
}

#[test]
fn midgame_setup_uses_analysis_moves_instead_of_unreplayable_full_history() {
    let mut position = empty();
    let old_move = Move::Play(Color::Black, Point { x: 3, y: 3 });
    position.initial_stones = vec![
        (Color::Black, Point { x: 3, y: 3 }),
        (Color::White, Point { x: 6, y: 6 }),
    ];
    position.initial_player = Color::White;
    position.board = Position::with_setup(
        position.board.size(),
        position.rules.rules(),
        position.initial_player,
        position.initial_stones.clone(),
    )
    .unwrap();
    // The document retained an earlier move already present in the new setup.
    // Replaying the full history from this setup would play on an occupied point.
    assert!(position.board.play_unchecked_ko(old_move).is_err());
    let after_setup = Move::Play(Color::White, Point { x: 15, y: 3 });
    position.moves = vec![old_move, after_setup];
    position.analysis_moves = vec![after_setup];
    position.board.play_unchecked_ko(after_setup).unwrap();
    let request = build(&position, &LiveSettings::default()).unwrap();
    assert_eq!(request.moves, [(Player::White, "Q16".to_owned())]);
    assert_eq!(request.initial_player, Some(Player::White));
    assert_eq!(replay(&request, position.rules), position.board);
}

#[test]
fn empty_replay_after_player_reset_keeps_white_to_play() {
    let mut position = empty();
    position.initial_player = Color::White;
    position.board = Position::with_setup(
        position.board.size(),
        position.rules.rules(),
        Color::White,
        [],
    )
    .unwrap();
    position.moves = vec![Move::Pass(Color::Black), Move::Pass(Color::White)];
    let request = build(&position, &LiveSettings::default()).unwrap();
    assert_eq!(request.initial_player, Some(Player::White));
    assert!(request.moves.is_empty());
    assert_eq!(replay(&request, position.rules), position.board);
}

#[test]
fn all_preset_identities_are_passed_to_katago_without_remapping() {
    for (preset, name) in [
        (RulePreset::Chinese, "chinese"),
        (RulePreset::Japanese, "japanese"),
        (RulePreset::Korean, "korean"),
        (RulePreset::Aga, "aga"),
        (RulePreset::TrompTaylor, "tromp-taylor"),
        (RulePreset::NewZealand, "new-zealand"),
    ] {
        let mut position = empty();
        position.rules = preset;
        let request = build(&position, &LiveSettings::default()).unwrap();
        assert_eq!(request.rules, Rules::Named(name.to_owned()));
    }
}

#[test]
fn malformed_public_position_fields_return_typed_errors() {
    let mut position = empty();
    for komi in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        position.komi = komi;
        assert!(matches!(
            build(&position, &LiveSettings::default()),
            Err(AnalysisError::InvalidPosition(_))
        ));
    }
    position.komi = 7.5;
    let outside = Point { x: 19, y: 0 };
    position.initial_stones = vec![(Color::Black, outside)];
    assert!(matches!(
        build(&position, &LiveSettings::default()),
        Err(AnalysisError::Coordinate(
            CoordinateError::OutOfBounds { .. }
        ))
    ));
    position.initial_stones.clear();
    position.analysis_moves = vec![Move::Play(Color::White, outside)];
    assert!(matches!(
        build(&position, &LiveSettings::default()),
        Err(AnalysisError::Coordinate(
            CoordinateError::OutOfBounds { .. }
        ))
    ));
}
