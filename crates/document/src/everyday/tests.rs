use go_core::{BoardSize, Color, Point};
use proptest::prelude::*;

use super::*;
use crate::{Command, NodeId};

fn document(source: &str) -> GameDocument {
    GameDocument::from_sgf(&sgf::parse(source.as_bytes()).unwrap().games[0]).unwrap()
}

fn serialize(tree: GameTree) -> String {
    sgf::serialize(&sgf::Collection {
        games: vec![tree],
        encoding: "UTF-8".into(),
        warnings: Vec::new(),
    })
}

#[test]
fn clean_exports_match_golden_without_mutating_document() {
    // Hand-written expectations for Java RAW / RAW_COMMENT / CURRENT_BRANCH:
    // analysis slots are omitted, arbitrary game metadata and markup retained.
    // Unlike Java's root-only leakage, the card requires clean roots as well.
    let mut doc = document(include_str!("../../fixtures/everyday/source.sgf"));
    doc.apply(Command::GoTo(NodeId(4))).unwrap();
    let before = doc.snapshot();
    let revision = doc.revision();
    let cases = [
        (
            false,
            false,
            include_str!("../../fixtures/everyday/clean.sgf"),
        ),
        (
            true,
            false,
            include_str!("../../fixtures/everyday/comments.sgf"),
        ),
        (
            false,
            true,
            include_str!("../../fixtures/everyday/branch.sgf"),
        ),
    ];
    for (comments, branch, expected) in cases {
        assert_eq!(serialize(doc.to_clean_sgf(comments, branch)), expected);
        assert_eq!(doc.snapshot(), before);
        assert_eq!(doc.revision(), revision);
        assert!(!doc.can_undo());
    }
}

#[test]
fn branch_export_includes_current_root_and_first_continuation_to_leaf() {
    let mut doc = document("(;SZ[9];B[aa](;W[bb];B[cc])(;W[dd];B[ee]))");
    assert_eq!(
        serialize(doc.to_clean_sgf(false, true)),
        "(;SZ[9]CA[UTF-8];B[aa];W[bb];B[cc])\n"
    );
    doc.apply(Command::GoTo(NodeId(4))).unwrap();
    assert_eq!(
        serialize(doc.to_clean_sgf(false, true)),
        "(;SZ[9]CA[UTF-8];B[aa];W[dd];B[ee])\n"
    );
}

fn property(doc: &GameDocument, id: u32, name: &str) -> Vec<String> {
    doc.node(NodeId(id))
        .unwrap()
        .properties()
        .iter()
        .find(|p| p.identifier == name)
        .unwrap()
        .values
        .clone()
}

fn check_transform(transform: BoardTransform, point: &str, range: &str, arrow: &str) {
    let mut doc = document(
        "(;SZ[9]AB[bc]AW[de]AE[fg]TR[ab:cd]SQ[bc]CR[bc]MA[bc]SL[bc]TB[bc]TW[bc]DD[bc]VW[]LB[bc:label:二]AR[bc:de]LN[bc:de]XX[bc];B[bc](;W[de])(;W[]))",
    );
    doc.apply(Command::Transform(transform)).unwrap();
    assert_eq!(property(&doc, 0, "AB"), [point]);
    let white = arrow.split_once(':').unwrap().1;
    assert_eq!(property(&doc, 0, "AW"), [white]);
    assert_eq!(property(&doc, 2, "W"), [white]);
    let erased = match transform {
        BoardTransform::RotateRight => "cf",
        BoardTransform::RotateLeft => "gd",
        BoardTransform::FlipHorizontal => "dg",
        BoardTransform::FlipVertical => "fc",
    };
    assert_eq!(property(&doc, 0, "AE"), [erased]);
    assert_eq!(property(&doc, 1, "B"), [point]);
    for name in ["SQ", "CR", "MA", "SL", "TB", "TW", "DD"] {
        assert_eq!(property(&doc, 0, name), [point]);
    }
    assert_eq!(property(&doc, 0, "TR"), [range]);
    assert_eq!(property(&doc, 0, "LB"), [format!("{point}:label:二")]);
    assert_eq!(property(&doc, 0, "AR"), [arrow]);
    assert_eq!(property(&doc, 0, "LN"), [arrow]);
    assert_eq!(property(&doc, 0, "XX"), ["bc"]);
    assert_eq!(property(&doc, 0, "VW"), [""]);
    assert_eq!(property(&doc, 3, "W"), [""]);
}

#[test]
fn rotate_right_transforms_tree_setup_and_markup() {
    check_transform(BoardTransform::RotateRight, "gb", "fa:hc", "gb:ed");
}
#[test]
fn rotate_left_transforms_tree_setup_and_markup() {
    check_transform(BoardTransform::RotateLeft, "ch", "bg:di", "ch:ef");
}
#[test]
fn horizontal_flip_matches_java_left_right_effect() {
    check_transform(BoardTransform::FlipHorizontal, "hc", "gb:id", "hc:fe");
}
#[test]
fn vertical_flip_matches_java_top_bottom_effect() {
    check_transform(BoardTransform::FlipVertical, "bg", "af:ch", "bg:de");
}

#[test]
fn transform_is_one_undoable_content_edit_and_changes_analysis_key() {
    let mut doc = document("(;SZ[9]AB[bc];W[de](;B[fg])(;B[aa]))");
    doc.apply(Command::GoTo(NodeId(2))).unwrap();
    let before = doc.snapshot();
    let key = doc.position_at(doc.current()).unwrap().analysis_key();
    let content_revision = doc.content_revision();
    doc.apply(Command::Transform(BoardTransform::RotateRight))
        .unwrap();
    let after = doc.snapshot();
    assert_eq!(doc.current(), before.current);
    assert_eq!(
        after.nodes.iter().map(|n| n.id()).collect::<Vec<_>>(),
        before.nodes.iter().map(|n| n.id()).collect::<Vec<_>>()
    );
    assert_ne!(doc.position_at(doc.current()).unwrap().analysis_key(), key);
    assert!(doc.content_revision() > content_revision);
    doc.undo().unwrap();
    assert_eq!(doc.snapshot(), before);
    assert_eq!(doc.position_at(doc.current()).unwrap().analysis_key(), key);
    assert!(!doc.can_undo());
    doc.redo().unwrap();
    assert_eq!(doc.snapshot(), after);
}

#[test]
fn rotations_reject_rectangular_boards_atomically() {
    for transform in [BoardTransform::RotateRight, BoardTransform::RotateLeft] {
        let mut doc = document("(;SZ[9:13];B[bc])");
        let before = doc.snapshot();
        assert_eq!(
            doc.apply(Command::Transform(transform)),
            Err(DocError::TransformRequiresSquare(
                BoardSize::new(9, 13).unwrap()
            ))
        );
        assert_eq!(doc.snapshot(), before);
        assert_eq!(doc.revision(), 0);
        assert!(!doc.can_undo());
    }
}

#[test]
fn rectangular_flips_transform_tree_analysis_and_row_major_ownership() {
    let size = BoardSize::new(13, 9).unwrap();
    let ownership: Vec<_> = (0..117).map(|index| index as f64 / 1000.0).collect();
    let values = ownership
        .iter()
        .map(|value| format!("{value:.3}"))
        .collect::<Vec<_>>()
        .join(" ");
    let payload = format!(
        "test 45.125 3 2.125 1.25 0.1 rootVisits=5 wrn=1.5 unknown=42\ninfo move B7 visits 3 winrate 5500 prior 3.25 pv B7 N1 pass pvVisits 3 2 1 ownership {values}"
    );
    for (transform, black, white, erased, markup, label, arrow, played, child, candidate, pv) in [
        (
            BoardTransform::FlipHorizontal,
            "lc",
            "be",
            "ad",
            "kb:md",
            "ai:末端",
            "ma:ai",
            "jh",
            "ca",
            "M7",
            ["M7", "A1", "pass"],
        ),
        (
            BoardTransform::FlipVertical,
            "bg",
            "le",
            "mf",
            "af:ch",
            "ma:末端",
            "ai:ma",
            "db",
            "ki",
            "B3",
            ["B3", "N9", "pass"],
        ),
    ] {
        let mut doc = document(&format!(
            "(;SZ[13:9]AB[bc]AW[le]AE[md]TR[ab:cd]LB[mi:末端]AR[aa:mi]LZOP[{payload}]LZOP2[{payload}];W[dh]LZ[{payload}]LZ2[{payload}](;B[ka])(;B[]))"
        ));
        doc.apply(Command::GoTo(NodeId(2))).unwrap();
        let before = doc.snapshot();
        let key = doc.position_at(doc.current()).unwrap().analysis_key();
        doc.apply(Command::Transform(transform)).unwrap();
        assert_eq!(property(&doc, 0, "SZ"), ["13:9"]);
        assert_eq!(property(&doc, 0, "AB"), [black]);
        assert_eq!(property(&doc, 0, "AW"), [white]);
        assert_eq!(property(&doc, 0, "AE"), [erased]);
        assert_eq!(property(&doc, 0, "TR"), [markup]);
        assert_eq!(property(&doc, 0, "LB"), [label]);
        assert_eq!(property(&doc, 0, "AR"), [arrow]);
        assert_eq!(property(&doc, 1, "W"), [played]);
        assert_eq!(property(&doc, 2, "B"), [child]);
        assert_eq!(property(&doc, 3, "B"), [""]);
        assert_eq!(doc.current(), before.current);
        assert_ne!(doc.position_at(doc.current()).unwrap().analysis_key(), key);

        // An independent row-based oracle catches width/height transposition:
        // horizontal reverses each 13-value row; vertical reverses the 9 rows.
        let expected: Vec<_> = match transform {
            BoardTransform::FlipHorizontal => ownership
                .chunks_exact(13)
                .flat_map(|row| row.iter().rev().copied())
                .collect(),
            BoardTransform::FlipVertical => ownership
                .chunks_exact(13)
                .rev()
                .flatten()
                .copied()
                .collect(),
            _ => unreachable!(),
        };
        for (id, name) in [(0, "LZOP"), (0, "LZOP2"), (1, "LZ"), (1, "LZ2")] {
            let values = property(&doc, id, name);
            let text = &values[0];
            let saved = crate::lz::parse(text, size, Color::Black).unwrap();
            assert_eq!(saved.candidates[0].coordinate, candidate);
            assert_eq!(saved.candidates[0].pv, pv);
            assert_eq!(saved.ownership.as_deref(), Some(expected.as_slice()));
            assert!(
                text.starts_with("test 45.125 3 2.125 1.25 0.1 rootVisits=5 wrn=1.5 unknown=42\n")
            );
            assert!(text.contains("pvVisits 3 2 1 ownership"));
        }

        let after = doc.snapshot();
        doc.undo().unwrap();
        assert_eq!(doc.snapshot(), before);
        assert!(!doc.can_undo());
        doc.redo().unwrap();
        assert_eq!(doc.snapshot(), after);
        doc.apply(Command::Transform(transform)).unwrap();
        assert_eq!(doc.snapshot(), before);
    }
}

#[test]
fn invalid_coordinate_or_analysis_in_later_branch_rejects_whole_edit() {
    for source in [
        "(;SZ[9];B[aa](;W[bb])(;W[zz]))",
        "(;SZ[9]AB[ab];W[bb]LZ[broken])",
        "(;SZ[9]LB[zz:bad])",
        "(;SZ[2]LZ[test 50 1\nownership 0])",
    ] {
        let mut doc = document(source);
        let before = doc.snapshot();
        assert!(matches!(
            doc.apply(Command::Transform(BoardTransform::RotateRight)),
            Err(DocError::InvalidProperty { .. })
        ));
        assert_eq!(doc.snapshot(), before);
        assert_eq!(doc.revision(), 0);
        assert!(!doc.can_undo());
    }
}

#[test]
fn saved_analysis_all_slots_pv_ownership_and_unknown_extensions_follow_transform() {
    let payload = "test  45.125 3 2.125 1.25 0.1 rootVisits=5 wrn=1.5 unknown=42\ninfo move A3 visits 3 winrate 5500 prior 3.25 scoreMean 2.125 pv A3 b1 pass pvVisits 3 2 1 ownership 0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9";
    let mut doc = document(&format!(
        "(;SZ[3]LZOP[{payload}]LZOP2[{payload}];B[bb]LZ[{payload}]LZ2[{payload}])"
    ));
    let before = doc.snapshot();
    doc.apply(Command::Transform(BoardTransform::RotateRight))
        .unwrap();
    for (id, name) in [(0, "LZOP"), (0, "LZOP2"), (1, "LZ"), (1, "LZ2")] {
        let values = property(&doc, id, name);
        let text = &values[0];
        assert!(
            text.starts_with("test  45.125 3 2.125 1.25 0.1 rootVisits=5 wrn=1.5 unknown=42\n")
        );
        assert!(text.contains("move C3 visits 3"));
        assert!(text.contains("pv C3 a2 pass pvVisits 3 2 1"));
        assert!(text.ends_with("ownership 0.7 0.4 0.1 0.8 0.5 0.2 0.9 0.6 0.3"));
        let parsed = crate::lz::parse(text, BoardSize::new(3, 3).unwrap(), Color::Black).unwrap();
        assert_eq!(parsed.candidates[0].coordinate, "C3");
        assert_eq!(parsed.visits, 5);
    }
    for _ in 0..3 {
        doc.apply(Command::Transform(BoardTransform::RotateRight))
            .unwrap();
    }
    assert_eq!(doc.snapshot(), before);
}

#[test]
fn legacy_pass_is_preserved_and_large_board_tt_is_a_point() {
    let mut doc = document("(;SZ[19];B[tt];W[];DD[true])");
    let before = doc.snapshot();
    doc.apply(Command::Transform(BoardTransform::RotateRight))
        .unwrap();
    assert_eq!(doc.snapshot(), before);
    assert!(!doc.can_undo());
    let mut large = document("(;SZ[25];B[tt])");
    large
        .apply(Command::Transform(BoardTransform::RotateRight))
        .unwrap();
    assert_eq!(property(&large, 1, "B"), ["ft"]);
}

proptest! {
    #[test]
    fn four_rotations_restore_entire_tree(size in 2u8..=25, x in any::<u8>(), y in any::<u8>(), right in any::<bool>()) {
        let board = BoardSize::new(size, size).unwrap();
        let point = Point { x: x % size, y: y % size };
        let sgf = point.to_sgf(board).unwrap();
        let gtp = point.to_gtp(board).unwrap();
        let mut doc = document(&format!("(;SZ[{size}]AB[{sgf}]LB[{sgf}:label]TR[aa:{sgf}]LZOP[test 50 3\nmove {gtp} visits 3 winrate 5000 pv {gtp} pass];W[]C[comment](;B[{sgf}])(;B[]XX[unchanged]))"));
        let before = doc.snapshot();
        let transform = if right { BoardTransform::RotateRight } else { BoardTransform::RotateLeft };
        for _ in 0..4 { doc.apply(Command::Transform(transform)).unwrap(); }
        prop_assert_eq!(doc.snapshot(), before);
    }

    #[test]
    fn repeating_either_flip_restores_rectangular_tree(width in 2u8..=25, height in 2u8..=25, x in any::<u8>(), y in any::<u8>(), horizontal in any::<bool>()) {
        let board = BoardSize::new(width, height).unwrap();
        let point = Point { x: x % width, y: y % height };
        let sgf = point.to_sgf(board).unwrap();
        let gtp = point.to_gtp(board).unwrap();
        let ownership = (0..usize::from(width) * usize::from(height))
            .map(|index| format!("{:.3}", index as f64 / 1000.0))
            .collect::<Vec<_>>().join(" ");
        let mut doc = document(&format!("(;SZ[{width}:{height}]AW[{sgf}]LB[{sgf}:label]AR[aa:{sgf}]LZOP[test 50 3\nmove {gtp} visits 3 winrate 5000 pv {gtp} pass ownership {ownership}];B[]C[comment](;W[{sgf}])(;W[]))"));
        let before = doc.snapshot();
        let transform = if horizontal { BoardTransform::FlipHorizontal } else { BoardTransform::FlipVertical };
        for _ in 0..2 { doc.apply(Command::Transform(transform)).unwrap(); }
        prop_assert_eq!(doc.snapshot(), before);
    }
}
