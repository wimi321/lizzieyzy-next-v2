#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::Command;
use proptest::prelude::*;

fn size() -> BoardSize {
    BoardSize::new(5, 5).unwrap()
}
fn document(bytes: &[u8]) -> GameDocument {
    GameDocument::from_sgf(&sgf::parse(bytes).unwrap().games[0]).unwrap()
}
fn serialized(tree: GameTree) -> String {
    sgf::serialize(&sgf::Collection {
        games: vec![tree],
        encoding: "UTF-8".into(),
        warnings: vec![],
    })
}
fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
}

#[test]
fn java_writer_golden_keeps_root_counts_candidates_extensions_and_black_perspective() {
    let doc = document(include_bytes!("../../../../fixtures/lz/java-analysis.sgf"));
    assert!(doc.warnings().is_empty());
    let root = doc.saved_analysis(NodeId(0)).unwrap().unwrap();
    close(root.winrate, 0.55);
    assert_eq!(root.visits, 12345);
    assert_eq!(root.score_lead, Some(2.5));
    assert_eq!(root.score_stdev, Some(1.2));
    assert_eq!(root.pda, Some(-2.0));
    assert_eq!(root.wrn, Some(0.04));
    assert_eq!(root.engine_name, "Main");
    let candidate = &root.candidates[0];
    assert_eq!(candidate.coordinate, "C3");
    assert_eq!(candidate.visits, 50000);
    close(candidate.winrate, 0.55);
    close(candidate.prior, 0.0123);
    assert_eq!(candidate.order, 10);
    assert_eq!(candidate.edge_visits, Some(0));
    assert_eq!(candidate.pv, ["C3", "D4"]);
    assert_eq!(candidate.pv_visits, [123, 45]);
    assert_eq!(root.candidates[1].coordinate, "pass");
    close(root.candidates[1].prior, 0.015);
    assert_eq!(root.ownership, Some(vec![0.2; 25]));
    let white_to_play = doc.saved_analysis(NodeId(1)).unwrap().unwrap();
    close(white_to_play.winrate, 0.45);
    assert_eq!(white_to_play.visits, 2000);
    assert_eq!(white_to_play.score_lead, Some(-3.0));
    close(white_to_play.candidates[0].winrate, 0.45);
    assert_eq!(white_to_play.candidates[0].score_lead, Some(-3.0));
    assert_eq!(white_to_play.ownership, Some(vec![-0.2; 25]));
    assert_eq!(white_to_play.wrn, Some(0.0));
    assert!(doc.saved_analysis(NodeId(2)).unwrap().is_none());
}

#[test]
fn java_primary_alias_and_secondary_slots_are_preserved_without_mixing_engines() {
    let doc = document(
        b"(;SZ[5]LZOP[Main 45 12 2.5 1.2 -2 wrn=0.04]LZOP2[Second 55 23 -2.5 1.3 1 wrn=0.1])",
    );
    let primary = doc.saved_analysis(doc.root()).unwrap().unwrap();
    assert_eq!(primary.engine_name, "Main");
    let saved = serialized(
        doc.to_sgf_with_analysis(&BTreeMap::from([(doc.root(), primary)]))
            .unwrap(),
    );
    assert!(saved.contains("LZOP2[Second 55 23 -2.5 1.3 1 wrn=0.1]"));
    let reparsed = document(saved.as_bytes());
    close(
        reparsed
            .saved_analysis(reparsed.root())
            .unwrap()
            .unwrap()
            .winrate,
        0.55,
    );
    let secondary_only = document(b"(;SZ[5]LZ2[Second 55 23 -2.5 1.3 1])");
    assert!(
        secondary_only
            .saved_analysis(secondary_only.root())
            .unwrap()
            .is_none()
    );
}

#[test]
fn short_headers_and_unknown_wrn_do_not_invent_analysis_fields() {
    for extension in ["", " wrn=NaN", " wrn=3", " wrn=-0.1", " wrn=", " wrn=bad"] {
        let analysis = parse(
            &format!("Main 45 12 2.5 1.2 -2{extension}"),
            size(),
            Color::Black,
        )
        .unwrap();
        assert_eq!(analysis.wrn, None);
    }
    let short = parse("Main 45 12", size(), Color::Black).unwrap();
    assert_eq!(short.score_lead, None);
    assert_eq!(short.score_stdev, None);
    assert_eq!(short.pda, None);
    let roundtrip = parse(
        &format(&short, size(), Color::Black).unwrap(),
        size(),
        Color::Black,
    )
    .unwrap();
    assert_eq!(roundtrip.score_lead, None);
    assert_eq!(roundtrip.wrn, None);
    close(roundtrip.winrate, short.winrate);
    let candidate_score = parse(
        "Main 45 12\nmove C3 visits 12 winrate 5500 scoreMean 2.5 pv C3",
        size(),
        Color::Black,
    )
    .unwrap();
    assert_eq!(candidate_score.score_lead, Some(2.5));
}

#[test]
fn malformed_lz_only_adds_diagnostics_and_does_not_affect_opening_or_raw_properties() {
    for payload in [
        "bad",
        "KataGo NaN 10",
        "KataGo 101 10",
        "KataGo 50 bogus",
        "KataGo 50 10\nmove Z99 visits 1 winrate 5000 pv Z99",
        "KataGo 50 10\nmove C3 visits 1 winrate 5000 pv D99",
    ] {
        let input = format!("(;SZ[5]LZ[{payload}];B[aa])");
        let doc = document(input.as_bytes());
        assert!(doc.saved_analysis(doc.root()).unwrap().is_none());
        assert_eq!(doc.warnings().len(), 1);
        assert_eq!(doc.warnings()[0].property, "LZ");
        assert!(doc.position_at(NodeId(1)).is_ok());
        let saved = serialized(doc.to_sgf_with_analysis(&BTreeMap::new()).unwrap());
        assert!(saved.contains(payload));
    }
}

#[test]
fn export_drops_stale_primary_but_keeps_secondary_bad_payloads_and_live_document_unchanged() {
    let mut doc=document(b"(;SZ[5]LZ[Main 45 12 2.5 1.2 -2];B[aa]LZ[Main 35 24 -3 1 0]LZ2[Second 50 20 1 1 0];W[bb]LZ[bad])");
    let root = doc.root();
    let mut valid = BTreeMap::from([(root, doc.saved_analysis(root).unwrap().unwrap())]);
    doc.apply(Command::SetGameInfo(crate::GameInfoEdit {
        black_name: None,
        black_rank: None,
        white_name: None,
        white_rank: None,
        komi: 6.5,
        rules: "chinese".into(),
        result: None,
        date: None,
        event: None,
    }))
    .unwrap();
    valid.clear();
    let before = doc.snapshot();
    let exported = serialized(doc.to_sgf_with_analysis(&valid).unwrap());
    assert!(!exported.contains("LZ[Main"));
    assert!(exported.contains("LZ2[Second 50 20 1 1 0]"));
    assert!(exported.contains("LZ[bad]"));
    assert_eq!(doc.snapshot(), before);
}

proptest! {
    #[test]
    fn round_trip_preserves_black_evaluations_and_java_candidate_precision(
        root_wr in 0_u32..=10000,
        candidate_wr in 0_u32..=10000,
        visits in 1_u32..=1_000_000,
        score in -10000_i32..=10000,
        black in any::<bool>(),
    ) {
        let original=SavedAnalysis {
            engine_name:"KataGo".into(),winrate:f64::from(root_wr)/10000.0,
            visits:u64::from(visits),score_lead:Some(f64::from(score)/100.0),
            score_stdev:Some(1.25),pda:Some(0.0),wrn:Some(0.04),
            candidates:vec![SavedCandidate {
                coordinate:"C3".into(),visits:u64::from(visits),winrate:f64::from(candidate_wr)/10000.0,
                score_lead:Some(f64::from(score)/100.0),score_stdev:Some(1.25),prior:0.1,order:2,
                edge_visits:Some(0),pv:vec!["C3".into(),"pass".into(),"D4".into()],pv_visits:vec![12,5,1],
            }],ownership:Some(vec![0.2;25]),
        };
        let base = document(if black { b"(;SZ[5]PL[B])" } else { b"(;SZ[5]PL[W])" });
        let saved = serialized(base.to_sgf_with_analysis(&BTreeMap::from([(base.root(),original.clone())])).unwrap());
        let reopened = document(saved.as_bytes());
        let parsed = reopened.saved_analysis(reopened.root()).unwrap().unwrap();
        prop_assert!((parsed.winrate-original.winrate).abs()<1e-12);
        prop_assert_eq!(parsed.visits,original.visits);
        prop_assert_eq!(parsed.score_lead,original.score_lead);
        prop_assert_eq!(parsed.score_stdev,original.score_stdev);
        prop_assert_eq!(parsed.pda,original.pda);
        prop_assert_eq!(parsed.wrn,original.wrn);
        prop_assert!((parsed.candidates[0].winrate-original.candidates[0].winrate).abs()<1e-12);
        prop_assert_eq!(&parsed.candidates[0].pv,&original.candidates[0].pv);
        prop_assert_eq!(&parsed.candidates[0].pv_visits,&original.candidates[0].pv_visits);
        prop_assert_eq!(parsed.candidates[0].prior,original.candidates[0].prior);
        prop_assert_eq!(parsed.ownership,original.ownership);
    }
    #[test]
    fn arbitrary_lz_never_panics(value in ".{0,500}") {
        let _=parse(&value,size(),Color::Black);
    }
}

#[test]
fn zero_visits_matches_java_header_only_behavior() {
    let parsed = parse("Main 45 0\ninvalid body", size(), Color::Black).unwrap();
    assert_eq!(parsed.visits, 0);
    assert!(parsed.candidates.is_empty());
}

#[test]
fn overflowing_counts_saturate_without_changing_evaluations_or_in_memory_values() {
    let doc = document(include_bytes!("../../../../fixtures/lz/java-analysis.sgf"));
    let mut original = doc.saved_analysis(doc.root()).unwrap().unwrap();
    original.visits = u64::MAX;
    original.candidates[0].visits = JAVA_MAX_VISITS + 1;
    original.candidates[0].edge_visits = Some(u64::MAX);
    original.candidates[0].pv_visits = vec![0, JAVA_MAX_VISITS, JAVA_MAX_VISITS + 1, u64::MAX];
    let before = original.clone();
    let (payload, warned) = format_with_warnings(&original, size(), Color::Black).unwrap();
    assert!(warned);
    let parsed = parse(&payload, size(), Color::Black).unwrap();
    assert_eq!(parsed.visits, JAVA_MAX_VISITS);
    assert_eq!(parsed.candidates[0].visits, JAVA_MAX_VISITS);
    assert_eq!(parsed.candidates[0].edge_visits, Some(JAVA_MAX_VISITS));
    assert_eq!(
        parsed.candidates[0].pv_visits,
        [0, JAVA_MAX_VISITS, JAVA_MAX_VISITS, JAVA_MAX_VISITS]
    );
    close(parsed.winrate, original.winrate);
    assert_eq!(parsed.score_lead, original.score_lead);
    assert_eq!(parsed.candidates[0].pv, original.candidates[0].pv);
    assert_eq!(parsed.candidates[1], original.candidates[1]);
    assert_eq!(original, before);
    assert!(format(&original, size(), Color::Black).is_ok());
}

#[test]
fn counts_at_java_boundary_do_not_warn_and_candidate_order_is_not_a_count() {
    let doc = document(include_bytes!("../../../../fixtures/lz/java-analysis.sgf"));
    let mut original = doc.saved_analysis(doc.root()).unwrap().unwrap();
    original.visits = JAVA_MAX_VISITS;
    original.candidates[0].visits = JAVA_MAX_VISITS;
    original.candidates[0].edge_visits = Some(JAVA_MAX_VISITS);
    original.candidates[0].pv_visits = vec![0, JAVA_MAX_VISITS];
    original.candidates[0].order = i32::MAX as u32;
    let (payload, warned) = format_with_warnings(&original, size(), Color::Black).unwrap();
    assert!(!warned);
    assert_eq!(
        parse(&payload, size(), Color::Black).unwrap().visits,
        JAVA_MAX_VISITS
    );
    original.candidates[0].order += 1;
    assert!(format_with_warnings(&original, size(), Color::Black).is_err());
}

#[test]
fn export_clamps_unchanged_imported_counts_and_reports_one_warning_per_affected_node() {
    let payload = "Main 45 2147483648 2.5 1.2 -2 rootVisits=2147483648\nmove C3 visits 2147483649 winrate 5500 edgeVisits 2147483650 pv C3 pvVisits 2147483651";
    let doc = document(
        format!("(;SZ[5]LZ[{payload}];B[aa]LZ[{payload}];W[bb]LZ[Main 45 2147483647])").as_bytes(),
    );
    let analyses: BTreeMap<_, _> = [NodeId(0), NodeId(1), NodeId(2)]
        .into_iter()
        .map(|node| (node, doc.saved_analysis(node).unwrap().unwrap()))
        .collect();
    let before = doc.snapshot();
    let (tree, warnings) = doc.to_sgf_with_analysis_and_warnings(&analyses).unwrap();
    assert_eq!(
        warnings,
        [NodeId(0), NodeId(1)].map(|node| DocumentWarning {
            node,
            property: "LZ".into(),
            kind: WarningKind::OutOfRange {
                fallback: "2147483647".into(),
            },
        })
    );
    let saved = GameDocument::from_sgf(&tree).unwrap();
    for node in [NodeId(0), NodeId(1)] {
        let result = saved.saved_analysis(node).unwrap().unwrap();
        assert_eq!(result.visits, JAVA_MAX_VISITS);
        assert_eq!(result.candidates[0].visits, JAVA_MAX_VISITS);
        assert_eq!(result.candidates[0].edge_visits, Some(JAVA_MAX_VISITS));
        assert_eq!(result.candidates[0].pv_visits, [JAVA_MAX_VISITS]);
    }
    assert_eq!(
        saved.node(NodeId(2)).unwrap().properties(),
        doc.node(NodeId(2)).unwrap().properties()
    );
    assert_eq!(doc.snapshot(), before);
    assert_eq!(analyses[&NodeId(0)].visits, JAVA_MAX_VISITS + 1);
}

#[test]
fn legacy_three_to_six_header_slots_keep_missing_values_unknown() {
    for suffix in ["", " 2.5", " 2.5 1.2", " 2.5 1.2 -2"] {
        let original = parse(&format!("Main 45 12{suffix}"), size(), Color::Black).unwrap();
        let output = format(&original, size(), Color::Black).unwrap();
        let reopened = parse(&output, size(), Color::Black).unwrap();
        assert_eq!(reopened.score_lead, original.score_lead);
        assert_eq!(reopened.score_stdev, original.score_stdev);
        assert_eq!(reopened.pda, original.pda);
        assert_eq!(reopened.wrn, None);
    }
}

#[test]
fn comparison_golden_is_parsed_by_the_same_codec_without_primary_fallback() {
    let doc = document(include_bytes!("../../../../fixtures/lz/java-analysis.sgf"));
    let secondary = doc.saved_comparison_analysis(doc.root()).unwrap().unwrap();
    assert!(doc.has_comparison_analysis());
    assert_eq!(secondary.engine_name, "Second");
    assert_eq!(secondary.visits, 23456);
    close(secondary.winrate, 0.45);
    assert_eq!(secondary.score_lead, Some(-2.5));
    assert_eq!(secondary.wrn, Some(0.1));
    assert_eq!(secondary.candidates[0].coordinate, "D4");
    assert_eq!(secondary.candidates[0].edge_visits, Some(17));
    assert_eq!(secondary.ownership, Some(vec![-0.4, 0.5]));
    assert!(doc.saved_comparison_analysis(NodeId(1)).unwrap().is_none());
    assert_eq!(doc.saved_analysis(NodeId(1)).unwrap().unwrap().visits, 2000);
    let white = document(
        b"(;SZ[5]PL[W]LZOP2[Second 45 30 3 1 0\nmove C3 visits 30 winrate 5500 scoreMean 3 pv C3 ownership 0.2])",
    );
    let secondary = white
        .saved_comparison_analysis(white.root())
        .unwrap()
        .unwrap();
    close(secondary.winrate, 0.45);
    assert_eq!(secondary.score_lead, Some(-3.0));
    close(secondary.candidates[0].winrate, 0.45);
    assert_eq!(secondary.candidates[0].score_lead, Some(-3.0));
    assert_eq!(secondary.ownership, Some(vec![-0.2]));
    assert!(white.saved_analysis(white.root()).unwrap().is_none());
}

#[test]
fn comparison_presence_tracks_active_nodes_and_malformed_raw_properties() {
    let mut doc = document(b"(;SZ[5];B[aa]LZ2[malformed])");
    assert!(doc.has_comparison_analysis());
    assert!(doc.saved_comparison_analysis(NodeId(1)).unwrap().is_none());
    assert_eq!(doc.warnings()[0].property, "LZ2");
    doc.apply(Command::DeleteNode(NodeId(1))).unwrap();
    assert!(!doc.has_comparison_analysis());
    doc.undo().unwrap();
    assert!(doc.has_comparison_analysis());
}

#[test]
fn comparison_export_keeps_raw_without_new_result_and_only_replaces_with_deeper_analysis() {
    let doc = document(
        b"(;SZ[5]LZ[Main 30 10]LZOP2[Second  45  40 2.0 extra=kept]LZ2[bad];B[aa]LZ2[Untouched 55 80])",
    );
    let primary = BTreeMap::from([(doc.root(), doc.saved_analysis(doc.root()).unwrap().unwrap())]);
    let original = doc.saved_comparison_analysis(doc.root()).unwrap().unwrap();
    let before = doc.snapshot();
    for visits in [0, 20, 40, 41] {
        let mut fresh = original.clone();
        fresh.engine_name = "NewComparison".into();
        fresh.visits = visits;
        let comparison = if visits == 0 {
            BTreeMap::new()
        } else {
            BTreeMap::from([(doc.root(), fresh)])
        };
        let (tree, warnings) = doc
            .to_sgf_with_analyses_and_warnings(&primary, &comparison)
            .unwrap();
        assert!(warnings.is_empty());
        let text = serialized(tree);
        assert!(text.contains("LZ[Main 30 10]"));
        assert!(text.contains("LZ2[bad]"));
        assert!(text.contains("LZ2[Untouched 55 80]"));
        let reopened = document(text.as_bytes());
        let saved = reopened
            .saved_comparison_analysis(reopened.root())
            .unwrap()
            .unwrap();
        if visits <= 40 {
            assert!(text.contains("LZOP2[Second  45  40 2.0 extra=kept]"));
            assert_eq!(saved, original);
        } else {
            assert!(!text.contains("LZOP2[Second"));
            assert_eq!(saved.engine_name, "NewComparison");
            assert_eq!(saved.visits, 41);
        }
        assert_eq!(doc.snapshot(), before);
    }
}

#[test]
fn comparison_export_maps_edited_variations_and_saturates_its_own_slot_only() {
    let mut doc = document(b"(;SZ[5](;B[aa])(;B[bb]LZOP2[Old 45 1]))");
    doc.apply(Command::DeleteNode(NodeId(1))).unwrap();
    doc.apply(Command::GoTo(NodeId(2))).unwrap();
    doc.apply(Command::Play(Move::Pass(Color::White))).unwrap();
    let saved = SavedAnalysis {
        engine_name: "Comparison".into(),
        visits: u64::MAX,
        winrate: 0.55,
        ..SavedAnalysis::default()
    };
    let comparison = BTreeMap::from([(doc.current(), saved)]);
    let (tree, warnings) = doc
        .to_sgf_with_analyses_and_warnings(&BTreeMap::new(), &comparison)
        .unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].node, doc.current());
    assert_eq!(warnings[0].property, "LZ2");
    let reopened = GameDocument::from_sgf(&tree).unwrap();
    let end = reopened.variation_tree().unwrap().last().unwrap().id;
    assert_eq!(
        reopened
            .saved_comparison_analysis(end)
            .unwrap()
            .unwrap()
            .visits,
        JAVA_MAX_VISITS
    );
    assert!(reopened.saved_analysis(end).unwrap().is_none());
    assert_eq!(
        reopened
            .saved_comparison_analysis(NodeId(1))
            .unwrap()
            .unwrap()
            .engine_name,
        "Old"
    );
    let clean = serialized(doc.to_clean_sgf(true, false));
    assert!(!clean.contains("LZOP2["));
    assert!(!clean.contains("LZ2["));
}

proptest! {
    #[test]
    fn comparison_roundtrip_keeps_black_values_and_does_not_create_primary_results(
        black in any::<bool>(),
        visits in 1_u32..1_000_000,
        winrate in 0_u32..=10000,
        score in -1000_i32..1000,
    ) {
        let doc = document(if black { b"(;SZ[5]PL[B])" } else { b"(;SZ[5]PL[W])" });
        let analysis = SavedAnalysis {
            engine_name: "Comparison".into(),
            visits: u64::from(visits),
            winrate: f64::from(winrate) / 10000.0,
            score_lead: Some(f64::from(score) / 100.0),
            ownership: Some(vec![0.2; 25]),
            ..SavedAnalysis::default()
        };
        let (tree, warnings) = doc.to_sgf_with_analyses_and_warnings(
            &BTreeMap::new(), &BTreeMap::from([(doc.root(), analysis.clone())]),
        ).unwrap();
        prop_assert!(warnings.is_empty());
        let reopened = document(serialized(tree).as_bytes());
        let parsed = reopened.saved_comparison_analysis(reopened.root()).unwrap().unwrap();
        prop_assert_eq!(parsed.visits, analysis.visits);
        prop_assert!((parsed.winrate - analysis.winrate).abs() < 1e-12);
        prop_assert_eq!(parsed.score_lead, analysis.score_lead);
        prop_assert_eq!(parsed.ownership, analysis.ownership);
        prop_assert!(reopened.saved_analysis(reopened.root()).unwrap().is_none());
    }
}
