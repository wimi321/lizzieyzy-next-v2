#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use go_core::{Color, IllegalMove, Point, RulePreset};
use serde_json::Value;

use crate::{Command, DocError, GameDocument, GamePosition, NodeId};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/sgf")
}

fn fixture_paths() -> Vec<PathBuf> {
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(fixture_dir().join("manifest.json")).unwrap())
            .unwrap();
    manifest["fixtures"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| fixture_dir().join(entry["file"].as_str().unwrap()))
        .collect()
}

fn parse_document(input: &[u8]) -> GameDocument {
    GameDocument::from_sgf(&sgf::parse(input).unwrap().games[0]).unwrap()
}

fn mainline(document: &GameDocument) -> Vec<NodeId> {
    let mut result = vec![document.root()];
    let mut current = document.root();
    while let Some(child) = document.node(current).unwrap().children().first() {
        current = *child;
        result.push(current);
    }
    result
}

fn all_nodes(document: &GameDocument) -> Vec<NodeId> {
    let mut pending = vec![document.root()];
    let mut result = vec![];
    while let Some(id) = pending.pop() {
        result.push(id);
        pending.extend(document.node(id).unwrap().children().iter().rev());
    }
    result
}

fn board_rows(position: &GamePosition) -> Vec<String> {
    position
        .board
        .stones()
        .chunks(usize::from(position.board.size().width))
        .map(|row| {
            row.iter()
                .map(|stone| match stone {
                    Some(Color::Black) => 'X',
                    Some(Color::White) => 'O',
                    None => '.',
                })
                .collect()
        })
        .collect()
}

#[test]
fn every_fixture_mainline_matches_independent_board_and_capture_expectations() {
    let paths = fixture_paths();
    assert!(paths.len() >= 42);
    for path in paths {
        let parsed = sgf::parse(&std::fs::read(&path).unwrap()).unwrap();
        let expected: Value =
            serde_json::from_slice(&std::fs::read(path.with_extension("expected.json")).unwrap())
                .unwrap();
        let games = expected["games"].as_array().unwrap();
        assert_eq!(parsed.games.len(), games.len(), "{}", path.display());
        for (game, expected) in parsed.games.iter().zip(games) {
            let document = GameDocument::from_sgf(game).unwrap();
            let nodes = all_nodes(&document);
            let line = mainline(&document);
            assert_eq!(nodes.len() as u64, expected["node_count"].as_u64().unwrap());
            assert_eq!(
                document.node(document.root()).unwrap().children().len() as u64,
                expected["root_branch_count"].as_u64().unwrap()
            );
            let comment_count = nodes
                .iter()
                .filter(|id| document.node(**id).unwrap().comment().is_some())
                .count();
            assert_eq!(
                comment_count as u64,
                expected["comment_count"].as_u64().unwrap()
            );
            let move_count = line
                .iter()
                .filter(|id| {
                    document
                        .node(**id)
                        .unwrap()
                        .properties()
                        .iter()
                        .any(|property| matches!(property.identifier.as_str(), "B" | "W"))
                })
                .count();
            assert_eq!(
                move_count as u64,
                expected["mainline_move_count"].as_u64().unwrap()
            );

            let invalid_indices: Vec<_> = expected["invalid_mainline_nodes"]
                .as_array()
                .map(|indices| {
                    indices
                        .iter()
                        .map(|index| index.as_u64().unwrap() as usize)
                        .collect()
                })
                .unwrap_or_default();
            let first_invalid = invalid_indices.first().copied();
            for (index, &id) in line.iter().enumerate() {
                if first_invalid.is_some_and(|first| index >= first) {
                    let origin = line[first_invalid.unwrap()];
                    assert!(
                        matches!(
                            document.node_error(id).unwrap(),
                            Some(DocError::IllegalMove { node, reason: IllegalMove::Occupied { .. } })
                                if node == origin
                        ),
                        "{} illegal node {index}",
                        path.display()
                    );
                    assert!(document.position_at(id).is_err());
                } else {
                    assert!(
                        document.node_error(id).unwrap().is_none(),
                        "{} node {index}",
                        path.display()
                    );
                }
            }
            // The occupied-move oracle explicitly records the preceding legal
            // board. The invalid node itself must remain an error, not a position.
            let endpoint = first_invalid.map_or(*line.last().unwrap(), |index| line[index - 1]);
            let position = document.position_at(endpoint).unwrap();
            let expected_rows: Vec<_> = expected["final_board"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row.as_str().unwrap().to_owned())
                .collect();
            assert_eq!(
                board_rows(&position),
                expected_rows,
                "{} board",
                path.display()
            );
            assert_eq!(
                position.captured_by.black as u64,
                expected["captured_by"]["black"].as_u64().unwrap(),
                "{} black captures",
                path.display()
            );
            assert_eq!(
                position.captured_by.white as u64,
                expected["captured_by"]["white"].as_u64().unwrap(),
                "{} white captures",
                path.display()
            );
            assert_eq!(
                u64::from(position.board.size().width),
                expected["board_size"]["width"].as_u64().unwrap()
            );
            assert_eq!(
                u64::from(position.board.size().height),
                expected["board_size"]["height"].as_u64().unwrap()
            );
            assert_eq!(
                u64::from(position.handicap),
                expected["handicap"].as_u64().unwrap()
            );
            assert_eq!(
                position.rules_raw.as_deref(),
                expected["rules_raw"].as_str()
            );
        }
    }
}

// A one-child variation and a sequence have the same document tree. Compare
// each node at its child-index path without requiring an SGF grouping style.
fn tree_signature(tree: &sgf::GameTree) -> Vec<(Vec<usize>, Vec<sgf::Property>)> {
    let mut result = vec![];
    let mut pending = vec![(tree, vec![])];
    while let Some((tree, mut path)) = pending.pop() {
        for (index, node) in tree.sequence.iter().enumerate() {
            if index > 0 {
                path.push(0);
            }
            result.push((path.clone(), node.properties.clone()));
        }
        for (index, child) in tree.children.iter().enumerate().rev() {
            let mut child_path = path.clone();
            child_path.push(index);
            pending.push((child, child_path));
        }
    }
    result
}

fn as_collection(game: sgf::GameTree) -> sgf::Collection {
    sgf::Collection {
        games: vec![game],
        encoding: "UTF-8".to_owned(),
        warnings: vec![],
    }
}

#[test]
fn every_fixture_round_trip_preserves_all_node_properties_including_unknown_and_lz() {
    for path in fixture_paths() {
        let collection = sgf::parse(&std::fs::read(&path).unwrap()).unwrap();
        for game in collection.games {
            let document = GameDocument::from_sgf(&game).unwrap();
            let output = document.to_sgf();
            assert_eq!(
                tree_signature(&output),
                tree_signature(&game),
                "{}",
                path.display()
            );
            // Round-trip actual text too: sgf owns the unavoidable root CA
            // normalization, while document must retain every other value.
            let expected = sgf::parse(sgf::serialize(&as_collection(game)).as_bytes()).unwrap();
            let reparsed = sgf::parse(sgf::serialize(&as_collection(output)).as_bytes()).unwrap();
            assert_eq!(
                tree_signature(&reparsed.games[0]),
                tree_signature(&expected.games[0]),
                "{} serialized",
                path.display()
            );
            let reloaded = GameDocument::from_sgf(&reparsed.games[0]).unwrap();
            assert_eq!(
                tree_signature(&reloaded.to_sgf()),
                tree_signature(&reparsed.games[0])
            );
        }
    }
}

#[test]
fn fox_fixture_uses_normalized_komi_without_rewriting_its_source_property() {
    let bytes = std::fs::read(fixture_dir().join("fox-export-shape-synthetic.sgf")).unwrap();
    let document = parse_document(&bytes);
    let position = document.position_at(document.root()).unwrap();
    assert_eq!(position.komi, 7.5);
    assert_eq!(position.rules, RulePreset::Chinese);
    let km = document
        .node(document.root())
        .unwrap()
        .properties()
        .iter()
        .find(|property| property.identifier == "KM")
        .unwrap();
    assert_eq!(km.values, ["375"]);
    let round_trip = GameDocument::from_sgf(&document.to_sgf()).unwrap();
    assert_eq!(round_trip.position_at(round_trip.root()).unwrap().komi, 7.5);
}

#[test]
fn java_lz_payload_stays_byte_exact_through_document_round_trip() {
    let bytes = std::fs::read(fixture_dir().join("java-saved-lz.sgf")).unwrap();
    let document = parse_document(&bytes);
    let move_id = document.node(document.root()).unwrap().children()[0];
    let lz = document
        .node(move_id)
        .unwrap()
        .properties()
        .iter()
        .find(|property| property.identifier == "LZ")
        .unwrap();
    let expected = b"KataGo-fixture 41.5 100 2.5 3.0 0.0 rootVisits=100 wrn=0.04\n\
        move G3 visits 80 winrate 5850 prior 3000 scoreMean 2.50 order 0 pv G3 G7";
    assert_eq!(lz.values[0].as_bytes(), expected);
    let reloaded = parse_document(sgf::serialize(&as_collection(document.to_sgf())).as_bytes());
    let reloaded_move = reloaded.node(reloaded.root()).unwrap().children()[0];
    assert_eq!(
        reloaded.node(reloaded_move).unwrap().properties(),
        document.node(move_id).unwrap().properties()
    );
}

#[test]
fn properties_stay_with_their_nodes_after_edit_promote_delete_and_undo() {
    let mut document = parse_document(
        b"(;SZ[9]XX[root];B[aa]XX[first]LZ[first payload]C[before]\
          (;W[bb]XX[main]LZ[main payload])(;W[cc]XX[variation]LZ[variation payload]))",
    );
    let first = document.node(document.root()).unwrap().children()[0];
    let main = document.node(first).unwrap().children()[0];
    let variation = document.node(first).unwrap().children()[1];
    let main_properties = document.node(main).unwrap().properties().to_vec();
    let variation_properties = document.node(variation).unwrap().properties().to_vec();
    let original = document.to_sgf();

    document.apply(Command::GoTo(first)).unwrap();
    document
        .apply(Command::SetComment("after ] \\ 中文".into()))
        .unwrap();
    assert_eq!(
        document.node(first).unwrap().comment(),
        Some("after ] \\ 中文")
    );
    let untouched: Vec<_> = document
        .node(first)
        .unwrap()
        .properties()
        .iter()
        .filter(|property| property.identifier != "C")
        .cloned()
        .collect();
    let original_untouched: Vec<_> = original.sequence[1]
        .properties
        .iter()
        .filter(|property| property.identifier != "C")
        .cloned()
        .collect();
    assert_eq!(untouched, original_untouched);

    document
        .apply(Command::PromoteVariation(variation))
        .unwrap();
    assert_eq!(document.node(first).unwrap().children(), &[variation, main]);
    assert_eq!(document.node(main).unwrap().properties(), main_properties);
    assert_eq!(
        document.node(variation).unwrap().properties(),
        variation_properties
    );
    document.apply(Command::DeleteNode(variation)).unwrap();
    assert_eq!(document.node(first).unwrap().children(), &[main]);
    assert!(document.node(variation).is_err());
    document.undo().unwrap();
    assert_eq!(document.node(first).unwrap().children(), &[variation, main]);
    assert_eq!(
        document.node(variation).unwrap().properties(),
        variation_properties
    );
    document.undo().unwrap();
    document.undo().unwrap();
    assert_eq!(
        tree_signature(&document.to_sgf()),
        tree_signature(&original)
    );
    assert_eq!(document.current(), first);
    assert_eq!(document.undo(), Err(DocError::NothingToUndo));
}

#[test]
fn illegal_nodes_and_descendants_report_the_original_move_and_other_branch_stays_valid() {
    let document = parse_document(b"(;SZ[9];B[aa](;W[aa];B[bb])(;W[cc]))");
    let first = document.node(document.root()).unwrap().children()[0];
    let bad = document.node(first).unwrap().children()[0];
    let descendant = document.node(bad).unwrap().children()[0];
    let sibling = document.node(first).unwrap().children()[1];
    for id in [bad, descendant] {
        assert!(matches!(
            document.position_at(id),
            Err(DocError::IllegalMove { node, reason: IllegalMove::Occupied { point } })
                if node == bad && point == Point { x: 0, y: 0 }
        ));
    }
    assert!(document.node_error(sibling).unwrap().is_none());
    let valid = document.position_at(sibling).unwrap();
    assert_eq!(
        valid.board.stone_at(Point { x: 0, y: 0 }),
        Some(Color::Black)
    );
    assert_eq!(
        valid.board.stone_at(Point { x: 2, y: 2 }),
        Some(Color::White)
    );
    assert_eq!(document.node(descendant).unwrap().parent(), Some(bad));
}
