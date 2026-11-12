use super::*;
use crate::{Command, NodeId};

fn load(text: &str) -> GameDocument {
    GameDocument::from_sgf(&sgf::parse(text.as_bytes()).unwrap().games[0]).unwrap()
}

#[test]
fn root_setup_replaces_erases_and_preserves_turn_metadata_with_undo() {
    let mut doc = load("(;SZ[9]PB[Black]C[root]HA[2]AB[aa:ab]XX[keep])");
    let original = doc.snapshot();
    let point = Point { x: 0, y: 0 };
    doc.apply(Command::SetSetupStone {
        point,
        color: Some(Color::White),
    })
    .unwrap();
    let state = doc.position_at(doc.root()).unwrap();
    assert_eq!(state.board.stone_at(point), Some(Color::White));
    assert_eq!(state.board.to_play(), Color::White);
    assert!(state.moves.is_empty());
    let revision = doc.revision();
    doc.apply(Command::SetSetupStone {
        point,
        color: Some(Color::White),
    })
    .unwrap();
    assert_eq!(doc.revision(), revision);
    doc.undo().unwrap();
    assert_eq!(doc.snapshot(), original);
    doc.redo().unwrap();
    doc.apply(Command::SetSetupStone { point, color: None })
        .unwrap();
    assert_eq!(
        doc.position_at(doc.root()).unwrap().board.stone_at(point),
        None
    );
    doc.apply(Command::SetFirstPlayer(Color::Black)).unwrap();
    assert_eq!(
        doc.position_at(doc.root()).unwrap().board.to_play(),
        Color::Black
    );
    doc.undo().unwrap();
    assert_eq!(
        doc.position_at(doc.root()).unwrap().board.to_play(),
        Color::White
    );
    doc.redo().unwrap();
    doc.apply(Command::ClearSetup).unwrap();
    let empty = doc.position_at(doc.root()).unwrap();
    assert!(empty.board.stones().iter().all(Option::is_none));
    assert_eq!(empty.board.to_play(), Color::Black);
    assert_eq!(
        doc.game_info().unwrap().black_name.as_deref(),
        Some("Black")
    );
    doc.undo().unwrap();
    assert_eq!(doc.position_at(doc.root()).unwrap().initial_stones.len(), 1);
    doc.redo().unwrap();
    assert!(
        doc.position_at(doc.root())
            .unwrap()
            .initial_stones
            .is_empty()
    );
}

#[test]
fn java_setup_allows_zero_liberty_groups_but_replay_refuses_until_repaired() {
    // BoardRootSetupSeamTest.setupPlacementIgnoresCaptureAndSuicide: no capture
    // and no suicide validation during setup, including a fully occupied board.
    let mut doc = load("(;SZ[3])");
    for y in 0..3 {
        for x in 0..3 {
            let color = if x == 1 && y == 1 {
                Color::White
            } else {
                Color::Black
            };
            doc.apply(Command::SetSetupStone {
                point: Point { x, y },
                color: Some(color),
            })
            .unwrap();
        }
    }
    assert!(doc.position_at(doc.root()).is_err());
    let raw = doc.to_sgf();
    assert_eq!(
        raw.sequence[0]
            .properties
            .iter()
            .find(|p| p.identifier == "AB")
            .unwrap()
            .values
            .len(),
        8
    );
    assert_eq!(
        raw.sequence[0]
            .properties
            .iter()
            .find(|p| p.identifier == "AW")
            .unwrap()
            .values,
        ["bb"]
    );
    doc.apply(Command::ClearSetup).unwrap();
    assert!(
        doc.position_at(doc.root())
            .unwrap()
            .initial_stones
            .is_empty()
    );
    doc.undo().unwrap();
    assert!(doc.position_at(doc.root()).is_err());
    doc.apply(Command::SetSetupStone {
        point: Point { x: 1, y: 1 },
        color: None,
    })
    .unwrap();
    assert_eq!(doc.position_at(doc.root()).unwrap().initial_stones.len(), 8);
}

#[test]
fn setup_rejects_history_and_outside_points_atomically() {
    for sgf in ["(;SZ[9];B[aa])", "(;SZ[9];C[empty])", "(;SZ[9]B[aa])"] {
        let mut doc = load(sgf);
        let original = doc.snapshot();
        for command in [
            Command::ClearSetup,
            Command::SetFirstPlayer(Color::White),
            Command::SetSetupStone {
                point: Point { x: 1, y: 1 },
                color: Some(Color::Black),
            },
        ] {
            assert_eq!(doc.apply(command), Err(DocError::SetupRequiresRoot));
            assert_eq!(doc.snapshot(), original);
        }
    }
    let mut doc = load("(;SZ[9])");
    let original = doc.snapshot();
    assert!(
        doc.apply(Command::SetSetupStone {
            point: Point { x: 9, y: 0 },
            color: None
        })
        .is_err()
    );
    assert_eq!(doc.snapshot(), original);
    assert!(!doc.can_undo());
}

#[test]
fn conversion_materializes_current_board_but_keeps_root_metadata_and_comment() {
    let mut doc = load(
        "(;SZ[9]PB[Black]RU[japanese]KM[6.50]C[root]XX[keep]TR[bb]AB[aa](;W[cc]C[current];B[dd])(;W[ee]))",
    );
    doc.apply(Command::GoTo(NodeId(1))).unwrap();
    let original = doc.snapshot();
    let converted = doc.to_start_position().unwrap();
    assert_eq!(doc.snapshot(), original);
    assert_eq!(converted.snapshot().nodes.len(), 1);
    assert!(!converted.can_undo());
    let position = converted.position_at(converted.root()).unwrap();
    assert_eq!(
        position.board.stones(),
        doc.position_at(doc.current()).unwrap().board.stones()
    );
    assert_eq!(position.board.to_play(), Color::Black);
    assert!(position.moves.is_empty());
    assert_eq!(position.captured_by, crate::Captures::default());
    assert_eq!(
        converted.node(converted.root()).unwrap().comment(),
        Some("root")
    );
    assert_eq!(converted.game_info(), doc.game_info());
    assert_eq!(converted.markup_at(converted.root()).unwrap().len(), 1);
    let reloaded = GameDocument::from_sgf(&converted.to_sgf()).unwrap();
    assert_eq!(reloaded.position_at(reloaded.root()).unwrap(), position);
}

#[test]
fn toggle_on_move_adds_player_node_without_a_pass_and_undo_restores_focus() {
    let mut doc = load("(;SZ[9];B[aa];W[bb])");
    doc.apply(Command::GoTo(NodeId(1))).unwrap();
    let original = doc.snapshot();
    let key = doc.position_at(doc.current()).unwrap().analysis_key();
    doc.apply(Command::ToggleToPlay).unwrap();
    let changed = doc.snapshot();
    let position = doc.position_at(doc.current()).unwrap();
    assert_eq!(position.board.to_play(), Color::Black);
    assert_ne!(position.analysis_key(), key);
    assert_eq!(position.moves.len(), 1);
    assert!(position.analysis_moves.is_empty());
    assert_eq!(
        doc.node(doc.current()).unwrap().properties()[0].identifier,
        "PL"
    );
    assert_eq!(
        doc.node(NodeId(1)).unwrap().children(),
        &[NodeId(2), NodeId(3)]
    );
    doc.undo().unwrap();
    assert_eq!(doc.snapshot(), original);
    doc.redo().unwrap();
    assert_eq!(doc.snapshot(), changed);
    doc.apply(Command::ToggleToPlay).unwrap();
    assert_eq!(doc.current(), NodeId(3));
    assert_eq!(
        doc.position_at(doc.current()).unwrap().board.to_play(),
        Color::White
    );
    doc.undo().unwrap();
    assert_eq!(doc.snapshot(), changed);
}

#[test]
fn root_toggle_is_undoable_and_invalid_positions_do_not_change() {
    let mut doc = load("(;SZ[9]PL[b]PL[W])");
    let original = doc.snapshot();
    doc.apply(Command::ToggleToPlay).unwrap();
    assert_eq!(
        doc.position_at(doc.root()).unwrap().board.to_play(),
        Color::White
    );
    doc.undo().unwrap();
    assert_eq!(doc.snapshot(), original);
    doc.redo().unwrap();
    assert_eq!(
        doc.position_at(doc.root()).unwrap().board.to_play(),
        Color::White
    );
    let mut invalid = load("(;SZ[9];B[zz])");
    invalid.apply(Command::GoTo(NodeId(1))).unwrap();
    let original = invalid.snapshot();
    assert!(invalid.apply(Command::ToggleToPlay).is_err());
    assert!(invalid.to_start_position().is_err());
    assert_eq!(invalid.snapshot(), original);
}

proptest::proptest! {
    #[test]
    fn arbitrary_setup_edits_roundtrip_including_temporarily_illegal_groups(
        operations in proptest::collection::vec((0u8..3, 0u8..3, 0u8..5), 0..60)
    ) {
        let mut doc = load("(;SZ[3]C[unchanged])");
        let mut history = vec![doc.snapshot()];
        for (x, y, action) in operations {
            let before = doc.revision();
            let command = match action {
                0 => Command::ClearSetup,
                1 => Command::SetFirstPlayer(Color::White),
                _ => Command::SetSetupStone {
                    point: Point { x, y },
                    color: match action { 2 => Some(Color::Black), 3 => Some(Color::White), _ => None },
                },
            };
            doc.apply(command).unwrap();
            if doc.revision() != before { history.push(doc.snapshot()); }
        }
        let encoded = sgf::serialize(&sgf::Collection {
            games: vec![doc.to_sgf()], encoding: "UTF-8".into(), warnings: vec![],
        });
        let reloaded = load(&encoded);
        proptest::prop_assert_eq!(doc.position_at(doc.root()), reloaded.position_at(reloaded.root()));
        for expected in history.iter().rev().skip(1) {
            doc.undo().unwrap();
            proptest::prop_assert_eq!(&doc.snapshot(), expected);
        }
        for expected in history.iter().skip(1) {
            doc.redo().unwrap();
            proptest::prop_assert_eq!(&doc.snapshot(), expected);
        }
    }
}
