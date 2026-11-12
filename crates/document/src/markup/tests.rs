use super::*;
use crate::Command;

fn load(text: &str) -> GameDocument {
    GameDocument::from_sgf(&sgf::parse(text.as_bytes()).unwrap().games[0]).unwrap()
}

fn set(doc: &mut GameDocument, x: u8, y: u8, kind: MarkupKind) {
    doc.apply(Command::SetMarkup {
        point: Point { x, y },
        kind,
    })
    .unwrap();
}

#[test]
fn imported_markup_is_listed_and_untouched_sgf_properties_roundtrip_exactly() {
    let text = "(;SZ[9]TR[aa][ab:ac]SQ[ba]CR[ca]MA[da]LB[ea:A][eb:12][ec:hello:world][ed:字]XX[keep];B[ff])";
    let doc = load(text);
    let marks = doc.markup_at(doc.root()).unwrap();
    assert_eq!(marks.len(), 10);
    assert_eq!(
        marks[0],
        Markup {
            point: Point { x: 0, y: 0 },
            kind: MarkupKind::Triangle,
            text: None
        }
    );
    assert_eq!(marks[7].kind, MarkupKind::Number);
    assert_eq!(marks[7].text.as_deref(), Some("12"));
    assert_eq!(marks[8].text.as_deref(), Some("hello:world"));
    let raw = doc.to_sgf();
    assert_eq!(raw, sgf::parse(text.as_bytes()).unwrap().games[0]);
    let encoded = sgf::serialize(&sgf::Collection {
        games: vec![raw],
        encoding: "UTF-8".into(),
        warnings: vec![],
    });
    let imported = load(&encoded);
    assert_eq!(imported.markup_at(imported.root()).unwrap(), marks);
    assert!(doc.markup_at(NodeId(999)).is_err());
}

#[test]
fn each_markup_command_is_reversible_and_clicking_a_different_tool_also_removes() {
    for kind in [
        MarkupKind::Triangle,
        MarkupKind::Square,
        MarkupKind::Circle,
        MarkupKind::Cross,
        MarkupKind::Letter,
        MarkupKind::Number,
    ] {
        let mut doc = load("(;SZ[9];B[aa]C[keep]XX[keep])");
        doc.apply(Command::GoTo(NodeId(1))).unwrap();
        let original = doc.snapshot();
        set(&mut doc, 1, 1, kind);
        let changed = doc.snapshot();
        assert_eq!(doc.markup_at(doc.current()).unwrap()[0].kind, kind);
        assert!(doc.markup_at(doc.root()).unwrap().is_empty());
        doc.undo().unwrap();
        assert_eq!(doc.snapshot(), original);
        doc.redo().unwrap();
        assert_eq!(doc.snapshot(), changed);
        set(&mut doc, 1, 1, kind);
        assert!(doc.markup_at(doc.current()).unwrap().is_empty());
        doc.undo().unwrap();
        set(&mut doc, 1, 1, MarkupKind::Cross);
        assert!(doc.markup_at(doc.current()).unwrap().is_empty());
        doc.undo().unwrap();
        set(&mut doc, 1, 1, MarkupKind::Erase);
        assert!(doc.markup_at(doc.current()).unwrap().is_empty());
        doc.undo().unwrap();
        assert_eq!(doc.snapshot(), changed);
        doc.redo().unwrap();
        assert!(doc.markup_at(doc.current()).unwrap().is_empty());
    }
}

#[test]
fn clear_preserves_comment_setup_moves_and_unknown_properties() {
    let mut doc = load("(;SZ[9]AB[aa]PL[W]C[keep]XX[value]TR[bb]LB[cc:A]SQ[dd]CR[ee]MA[ff])");
    let original = doc.snapshot();
    let position = doc.position_at(doc.root()).unwrap();
    doc.apply(Command::ClearMarkup).unwrap();
    assert!(doc.markup_at(doc.root()).unwrap().is_empty());
    assert_eq!(doc.position_at(doc.root()).unwrap(), position);
    assert_eq!(doc.node(doc.root()).unwrap().comment(), Some("keep"));
    assert!(
        doc.node(doc.root())
            .unwrap()
            .properties()
            .iter()
            .any(|p| p.identifier == "XX")
    );
    let changed = doc.snapshot();
    doc.undo().unwrap();
    assert_eq!(doc.snapshot(), original);
    doc.redo().unwrap();
    assert_eq!(doc.snapshot(), changed);
    let revision = doc.revision();
    doc.apply(Command::ClearMarkup).unwrap();
    set(&mut doc, 8, 8, MarkupKind::Erase);
    assert_eq!(doc.revision(), revision);
    // Unlike Java's broad isListProperty check, a setup stone is not markup.
    set(&mut doc, 0, 0, MarkupKind::Triangle);
    assert_eq!(doc.position_at(doc.root()).unwrap(), position);
    assert_eq!(doc.markup_at(doc.root()).unwrap().len(), 1);
}

#[test]
fn java_letter_rule_uses_highest_first_utf16_character_and_skips_z_to_lowercase_a() {
    // LizzieFrame.tryToMarkup: max(first char, '@') + 1; no hole filling or AA.
    for (labels, expected) in [
        ("", "A"),
        ("LB[aa:A][ab:C]", "D"),
        ("LB[aa:Z]", "a"),
        ("LB[aa:a][ab:z]", "{"),
        ("LB[aa:Apple][ab:3]", "B"),
    ] {
        let mut doc = load(&format!("(;SZ[9]{labels})"));
        set(&mut doc, 8, 8, MarkupKind::Letter);
        assert_eq!(
            doc.markup_at(doc.root())
                .unwrap()
                .last()
                .unwrap()
                .text
                .as_deref(),
            Some(expected)
        );
    }
    let mut doc = load("(;SZ[9]LB[aa:A][ab:C])");
    set(&mut doc, 0, 0, MarkupKind::Erase);
    set(&mut doc, 8, 8, MarkupKind::Letter);
    assert_eq!(
        doc.markup_at(doc.root())
            .unwrap()
            .last()
            .unwrap()
            .text
            .as_deref(),
        Some("D")
    );
}

#[test]
fn java_number_rule_uses_highest_parseable_i32_plus_one_instead_of_first_hole() {
    for (labels, expected) in [
        ("", "1"),
        ("LB[aa:1][ab:3]", "4"),
        ("LB[aa:A][ab:-4]", "1"),
        ("LB[aa:+8][ab:2147483648][ac: 99]", "9"),
        ("LB[aa:001]", "2"),
        ("LB[aa:12:note]", "13"),
        ("LB[aa:１２]", "13"),
        ("LB[aa:١٢]", "13"),
        ("LB[aa:+１２]", "13"),
        ("LB[aa:𝟙𝟚][ab:2²][ac: 99]", "1"),
        ("LB[aa:-2147483648]", "1"),
        ("LB[aa:12,cc:99]", "1"),
    ] {
        let mut doc = load(&format!("(;SZ[9]{labels})"));
        set(&mut doc, 8, 8, MarkupKind::Number);
        assert_eq!(
            doc.markup_at(doc.root())
                .unwrap()
                .last()
                .unwrap()
                .text
                .as_deref(),
            Some(expected)
        );
    }
    let mut doc = load("(;SZ[9]LB[aa:2147483647])");
    let original = doc.snapshot();
    assert!(matches!(
        doc.apply(Command::SetMarkup {
            point: Point { x: 8, y: 8 },
            kind: MarkupKind::Number
        }),
        Err(DocError::MarkupLabelLimit { .. })
    ));
    assert_eq!(doc.snapshot(), original);
}

#[test]
fn editing_only_expands_touched_range_and_keeps_other_raw_values() {
    let mut doc = load("(;SZ[9]TR[aa:cc][dd:ee]LB[ff:arbitrary text]C[keep])");
    let original = doc.snapshot();
    set(&mut doc, 1, 1, MarkupKind::Erase);
    assert_eq!(doc.markup_at(doc.root()).unwrap().len(), 13);
    let props = doc.node(doc.root()).unwrap().properties();
    assert!(
        props
            .iter()
            .find(|p| p.identifier == "TR")
            .unwrap()
            .values
            .contains(&"dd:ee".to_owned())
    );
    assert_eq!(
        props.iter().find(|p| p.identifier == "LB").unwrap().values,
        ["ff:arbitrary text"]
    );
    doc.undo().unwrap();
    assert_eq!(doc.snapshot(), original);
}

#[test]
fn malformed_imported_markup_stays_raw_and_illegal_moves_do_not_block_markup() {
    let mut doc = load("(;SZ[9];B[zz]TR[zz]LB[bad][aa:text])");
    doc.apply(Command::GoTo(NodeId(1))).unwrap();
    assert!(doc.position_at(doc.current()).is_err());
    assert_eq!(doc.markup_at(doc.current()).unwrap().len(), 1);
    set(&mut doc, 1, 1, MarkupKind::Circle);
    let properties = doc.node(doc.current()).unwrap().properties();
    assert_eq!(
        properties
            .iter()
            .find(|p| p.identifier == "TR")
            .unwrap()
            .values,
        ["zz"]
    );
    assert_eq!(
        properties
            .iter()
            .find(|p| p.identifier == "LB")
            .unwrap()
            .values,
        ["bad", "aa:text"]
    );
    let original = doc.snapshot();
    assert!(
        doc.apply(Command::SetMarkup {
            point: Point { x: 9, y: 1 },
            kind: MarkupKind::Erase
        })
        .is_err()
    );
    assert_eq!(doc.snapshot(), original);
}

proptest::proptest! {
    #[test]
    fn arbitrary_markup_edits_roundtrip_and_undo_redo(
        operations in proptest::collection::vec((0u8..9, 0u8..9, 0u8..8), 0..80)
    ) {
        let mut doc = load("(;SZ[9]C[unchanged]XX[private])");
        let mut history = vec![doc.snapshot()];
        for (x, y, tool) in operations {
            let old_revision = doc.revision();
            let kind = match tool {
                0 => MarkupKind::Triangle, 1 => MarkupKind::Square,
                2 => MarkupKind::Circle, 3 => MarkupKind::Cross,
                4 => MarkupKind::Letter, 5 => MarkupKind::Number,
                _ => MarkupKind::Erase,
            };
            if tool == 7 { doc.apply(Command::ClearMarkup).unwrap(); }
            else { set(&mut doc, x, y, kind); }
            if old_revision != doc.revision() { history.push(doc.snapshot()); }
        }
        let encoded = sgf::serialize(&sgf::Collection {
            games: vec![doc.to_sgf()], encoding: "UTF-8".into(), warnings: vec![],
        });
        let reloaded = load(&encoded);
        proptest::prop_assert_eq!(doc.markup_at(doc.root()).unwrap(), reloaded.markup_at(reloaded.root()).unwrap());
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

#[test]
fn imported_unicode_decimal_labels_keep_text_and_number_kind() {
    let doc = load("(;SZ[9]LB[aa:１２][ab:١٢][ac:+１２][ad:𝟙𝟚])");
    let marks = doc.markup_at(doc.root()).unwrap();
    for mark in &marks[..3] {
        assert_eq!(mark.kind, MarkupKind::Number);
    }
    assert_eq!(marks[0].text.as_deref(), Some("１２"));
    assert_eq!(marks[1].text.as_deref(), Some("١٢"));
    assert_eq!(marks[2].text.as_deref(), Some("+１２"));
    assert_eq!(marks[3].kind, MarkupKind::Letter);
    assert_eq!(parse_java_number("-2147483648"), Some(i32::MIN));
    assert_eq!(parse_java_number("2147483648"), None);
    assert_eq!(parse_java_number("-2147483649"), None);
}
