#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::{
    AppSubscription, DocumentView, MoveRank, ProblemMovesQuery,
    actor::{Input, Signal, ViewFocus},
};
use document::{Command, NodeId};
use std::time::Duration;

const JAVA: &[u8] = include_bytes!("../../../../../fixtures/lz/java-analysis.sgf");
const GIB: &[u8] = include_bytes!("../../../../../fixtures/gib/utf8-even.gib");

async fn event(output: &mut AppSubscription) -> AppEvent {
    tokio::time::timeout(Duration::from_secs(5), output.recv())
        .await
        .unwrap()
        .unwrap()
}

async fn document_event(output: &mut AppSubscription) -> DocumentView {
    loop {
        if let AppEvent::DocumentChanged { view, .. } = event(output).await {
            return view;
        }
    }
}

async fn analysis_event(output: &mut AppSubscription) -> AnalysisView {
    loop {
        if let AppEvent::AnalysisUpdated(view) = event(output).await {
            return view;
        }
    }
}

#[tokio::test]
async fn java_file_populates_graph_information_candidates_and_problem_moves_without_engine() {
    let (mut actor, mut output) = crate::actor::tests::actor();
    actor.open(JAVA.to_vec(), None).await.unwrap();
    assert!(actor.running.is_none());
    let document = document_event(&mut output).await;
    let root = document.graph.points[0].summary.unwrap();
    assert_eq!(root.visits, 12345);
    assert!((root.winrate - 0.55).abs() < 1e-12);
    assert_eq!(root.score_lead, 2.5);
    assert!(root.score_lead_available);
    let white = document.graph.points[1].summary.unwrap();
    assert!((white.winrate - 0.45).abs() < 1e-12);
    assert_eq!(white.score_lead, -3.0);
    let analysis = analysis_event(&mut output).await;
    assert_eq!(analysis.source, AnalysisSource::Saved);
    assert_eq!(analysis.visits, root.visits);
    assert_eq!(analysis.candidates[0].coordinate, "C3");
    assert_eq!(analysis.candidates[0].visits, 50000);
    assert_eq!(analysis.candidates[0].score_lead, 2.5);
    assert_eq!(analysis.candidates[0].pv.len(), 2);
    assert_eq!(analysis.ownership, Some(vec![0.2; 25]));
    let problems = actor
        .problem_moves(ProblemMovesQuery {
            minimum_rank: MoveRank::Good,
            ..Default::default()
        })
        .unwrap();
    let loss = &problems.entries[0].loss;
    assert!((loss.winrate_loss - 10.0).abs() < 1e-12);
    assert_eq!(loss.score_loss, 5.5);
    assert!(loss.score_loss_available);
    actor.apply(Input::Edit(Command::Step(1))).await.unwrap();
    let document = document_event(&mut output).await;
    let analysis = analysis_event(&mut output).await;
    assert_eq!(analysis.node_id, document.tree.current);
    assert_eq!(analysis.source, AnalysisSource::Saved);
    assert_eq!(analysis.score_lead, -3.0);
    assert_eq!(analysis.candidates[0].score_lead, -3.0);
    assert_eq!(analysis.ownership, Some(vec![-0.2; 25]));
}

#[tokio::test]
async fn saved_data_survives_engine_changes_and_only_strictly_deeper_results_replace_it() {
    let (mut actor, mut output) = crate::actor::tests::actor();
    actor.open(JAVA.to_vec(), None).await.unwrap();
    actor.set_summary_fingerprint(Some("new-engine".into()));
    assert_eq!(
        actor.stored_analysis(0).unwrap().response.root_info.visits,
        12345
    );
    let position = actor.document.position_at(NodeId(0)).unwrap();
    actor.focus = Some(ViewFocus {
        revision: actor.revision,
        node_id: 0,
        key: position.analysis_key(),
        size: position.board.size(),
        to_play: position.board.to_play(),
    });
    let saved = actor.stored_analysis(0).unwrap();
    for visits in [1, 12345] {
        actor.publish_saved_focus();
        let snapshot = crate::actor::tests::snapshot(&actor, visits, 0.8);
        actor.signal(Signal::Snapshot(Box::new(snapshot)));
        let retained = analysis_event(&mut output).await;
        assert_eq!(retained.source, AnalysisSource::Saved);
        assert_eq!(retained.visits, 12345);
        assert_eq!(retained.summary.visits, 12345);
        assert_eq!(retained.candidates[0].coordinate, "C3");
        assert_eq!(retained.score_lead, 2.5);
        assert!((retained.winrate - 0.55).abs() < 1e-12);
        assert_eq!(actor.stored_analysis(0).unwrap(), saved);
    }
    let snapshot = crate::actor::tests::snapshot(&actor, 12346, 0.8);
    actor.signal(Signal::Snapshot(Box::new(snapshot)));
    let live = analysis_event(&mut output).await;
    assert_eq!(live.source, AnalysisSource::Live);
    assert_eq!(live.visits, 12346);
    assert_eq!(live.summary.visits, 12346);
    assert_eq!(live.summary.winrate, 0.8);
    assert!(actor.stored_analysis(0).unwrap().saved.is_none());
    // The other imported node is independent from the active engine namespace.
    actor.set_summary_fingerprint(None);
    assert!(actor.stored_analysis(0).is_none());
    assert!(actor.stored_analysis(1).unwrap().saved.is_some());
}

#[tokio::test]
async fn importing_saved_analysis_only_yields_to_strictly_deeper_restored_engine_results() {
    for visits in [1, 12345, 12346] {
        let (mut actor, _) = crate::actor::tests::actor();
        actor.open(JAVA.to_vec(), None).await.unwrap();
        let saved = actor.stored_analysis(0).unwrap();
        let mut cached = saved.clone();
        cached.saved = None;
        cached.response.id = "restored-engine".into();
        cached.response.root_info.visits = visits;
        cached.response.root_info.winrate = 0.8;
        cached.response.root_info.score_lead = -3.5;
        // Reopening clears the old summaries before restoring engine cache and
        // importing file analysis. Exercise that complete replacement path.
        actor
            .document_replaced_with_cache(
                std::collections::HashMap::from([(cached.key, cached.clone())]),
                true,
            )
            .await
            .unwrap();
        let restored = actor.stored_analysis(0).unwrap();
        let view = actor.review_view(0).unwrap().unwrap();
        if visits > saved.response.root_info.visits {
            assert_eq!(restored, cached);
            assert_eq!(view.source, AnalysisSource::Cache);
            assert_eq!(view.summary.visits, visits);
            assert_eq!(view.winrate, 0.8);
        } else {
            assert_eq!(restored, saved);
            assert_eq!(view.source, AnalysisSource::Saved);
            assert_eq!(view.summary.visits, 12345);
            assert!((view.winrate - 0.55).abs() < 1e-12);
            assert_eq!(view.candidates[0].coordinate, "C3");
        }
    }
}

#[tokio::test]
async fn saved_file_roundtrip_and_position_edits_never_export_stale_primary_analysis() {
    let temp = tempfile::tempdir().unwrap();
    let (mut actor, _) = crate::actor::tests::actor();
    actor.directory = temp.path().to_owned();
    actor.open(JAVA.to_vec(), None).await.unwrap();
    let before = actor.document.saved_analysis(NodeId(0)).unwrap().unwrap();
    let path = temp.path().join("saved.sgf");
    actor.save_document(path.clone()).await.unwrap();
    let collection = sgf::parse(&tokio::fs::read(&path).await.unwrap()).unwrap();
    let restored = document::GameDocument::from_sgf(&collection.games[0]).unwrap();
    let after = restored.saved_analysis(NodeId(0)).unwrap().unwrap();
    assert_eq!(before, after);
    actor
        .apply(Input::Edit(Command::SetComment("still valid".into())))
        .await
        .unwrap();
    assert!(actor.stored_analysis(0).is_some());
    let old = actor.document.game_info().unwrap();
    let info = document::GameInfoEdit {
        black_name: old.black_name,
        black_rank: old.black_rank,
        white_name: old.white_name,
        white_rank: old.white_rank,
        komi: 6.5,
        rules: old.rules_preset,
        result: old.result,
        date: old.date,
        event: old.event,
    };
    actor
        .apply(Input::Edit(Command::SetGameInfo(info)))
        .await
        .unwrap();
    assert!(actor.stored_analysis(0).is_none());
    let exported = actor.export_document_with_analysis().unwrap().0;
    let restored = document::GameDocument::from_sgf(&exported).unwrap();
    assert!(restored.saved_analysis(NodeId(0)).unwrap().is_none());
    assert!(restored.saved_analysis(NodeId(1)).unwrap().is_none());
    actor.apply(Input::Undo).await.unwrap();
    assert!(actor.stored_analysis(0).is_some());
}

#[tokio::test]
async fn malformed_lz_only_warns_and_does_not_block_other_nodes() {
    let (mut actor, mut output) = crate::actor::tests::actor();
    actor.open(b"(;SZ[5]LZ[bad];B[aa]LZ[main 40 100 2 0 0\nmove C3 visits 80 winrate 6000 scoreMean 2 pv C3])".to_vec(), None).await.unwrap();
    let document = document_event(&mut output).await;
    assert_eq!(
        document
            .warnings
            .iter()
            .filter(|w| w.property == "LZ")
            .count(),
        1
    );
    assert!(document.graph.points[0].summary.is_none());
    assert_eq!(document.graph.points[1].summary.unwrap().visits, 100);
}

#[tokio::test]
async fn gib_path_drop_and_bytes_import_without_claiming_a_save_target() {
    let temp = tempfile::tempdir().unwrap();
    let gib = temp.path().join("import.GIB");
    tokio::fs::write(&gib, GIB).await.unwrap();
    let (mut actor, mut output) = crate::actor::tests::actor();
    actor.directory = temp.path().to_owned();
    actor
        .apply(Input::OpenDroppedPaths(vec![
            temp.path().join("missing.gib"),
            gib.clone(),
        ]))
        .await
        .unwrap();
    let opened = document_event(&mut output).await;
    assert_eq!(opened.file_name.as_deref(), Some("import.GIB"));
    assert_eq!(opened.file_path, None);
    assert_eq!(opened.tree.main_line.len(), 5);
    assert_eq!(opened.game_info.black_rank.as_deref(), Some("3d"));
    assert_eq!(actor.settings.recent_files[0], gib.to_string_lossy());
    assert!(matches!(
        actor.save_document(gib.clone()).await,
        Err(AppError::GibSaveTarget)
    ));
    assert_eq!(tokio::fs::read(&gib).await.unwrap(), GIB);
    let sgf = temp.path().join("converted.sgf");
    actor.save_document(sgf.clone()).await.unwrap();
    assert_eq!(actor.file_state.path, Some(sgf.clone()));
    assert!(sgf::parse(&tokio::fs::read(sgf).await.unwrap()).is_ok());
    actor.open(GIB.to_vec(), None).await.unwrap();
    assert!(actor.file_state.path.is_none());
}

#[tokio::test]
async fn legacy_winrate_only_has_explicit_score_availability_and_no_fake_score_loss() {
    let (mut actor, mut output) = crate::actor::tests::actor();
    actor.open(b"(;SZ[5]LZ[Leela 40 100\nmove C3 visits 80 winrate 6000 pv C3];B[aa]LZ[Leela 45 100\nmove D4 visits 80 winrate 5500 pv D4])".to_vec(), None).await.unwrap();
    let document = document_event(&mut output).await;
    assert!(
        !document.graph.points[0]
            .summary
            .unwrap()
            .score_lead_available
    );
    let view = analysis_event(&mut output).await;
    assert!(!view.candidates[0].score_lead_available);
    let problems = actor.problem_moves(ProblemMovesQuery::default()).unwrap();
    let loss = problems.error_bar[1].loss.as_ref().unwrap();
    assert!(!loss.score_loss_available);
    assert!((loss.winrate_loss - 15.0).abs() < 1e-12);
}

#[tokio::test]
async fn fake_engine_strictly_deeper_result_replaces_file_data_and_is_saved() {
    // Resolve/build the test executable outside the operation's safety timeout.
    let program = fake_katago::binary_path().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let model = temp.path().join("model");
    let config_path = temp.path().join("analysis.cfg");
    std::fs::write(&model, "fake model").unwrap();
    std::fs::write(&config_path, "fake config").unwrap();
    let mut config = crate::AppConfig::default();
    config.analysis.live.max_visits = 12346;
    config.analysis.live.report_interval = Duration::from_secs(60);
    let (core, mut output) = crate::spawn_with_config(temp.path(), config).await.unwrap();
    core.set_auto_quick_analyze_on_load(false).await.unwrap();
    core.open_bytes(JAVA.to_vec()).await.unwrap();
    assert_eq!(
        analysis_event(&mut output).await.source,
        AnalysisSource::Saved
    );
    core.configure_engine(Some(crate::EngineProfile {
        remote: None,
        human_model: None,
        program,
        model,
        config: config_path,
        extra_args: vec!["--visits-per-sec".into(), "100000".into()],
        canonical_model_name: None,
    }))
    .await
    .unwrap();
    let live = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(AppEvent::AnalysisUpdated(view)) = output.recv().await
                && view.source == AnalysisSource::Live
            {
                break view;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(live.visits, 12346);
    assert_eq!(live.summary.visits, 12346);
    let path = temp.path().join("engine.sgf");
    core.save(&path).await.unwrap();
    let collection = sgf::parse(&tokio::fs::read(path).await.unwrap()).unwrap();
    let restored = document::GameDocument::from_sgf(&collection.games[0]).unwrap();
    let root = restored.saved_analysis(NodeId(0)).unwrap().unwrap();
    assert_eq!(root.visits, live.visits);
    assert!((root.winrate - live.winrate).abs() < 1e-12);
    assert_eq!(root.score_lead, Some(live.score_lead));
    assert_eq!(root.candidates[0].coordinate, live.candidates[0].coordinate);
    assert!((root.candidates[0].winrate - live.candidates[0].winrate).abs() <= 0.00005);
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn broken_sgf_with_gib_text_is_not_reinterpreted_and_keeps_previous_document() {
    let (mut actor, _) = crate::actor::tests::actor();
    actor.open(JAVA.to_vec(), None).await.unwrap();
    let before = actor.document.to_sgf();
    let revision = actor.revision;
    let broken = " (;C[notes\nSTO 0 1 1 3 3\n";
    let utf16: Vec<_> = [0xff, 0xfe]
        .into_iter()
        .chain(broken.encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    for bytes in [broken.as_bytes().to_vec(), utf16] {
        assert!(actor.open(bytes, None).await.is_err());
        assert_eq!(actor.document.to_sgf(), before);
        assert_eq!(actor.revision, revision);
        assert!(actor.stored_analysis(0).is_some());
    }
}

#[tokio::test]
async fn system_open_during_trial_returns_error_and_reports_open_failed() {
    let (mut actor, mut output) = crate::actor::tests::actor();
    actor.open(JAVA.to_vec(), None).await.unwrap();
    actor.start_move_trial().await.unwrap();
    let before = actor.document.to_sgf();
    let path = std::path::PathBuf::from("another.gib");
    assert!(
        actor
            .apply(Input::OpenDroppedPaths(vec![path.clone()]))
            .await
            .is_err()
    );
    loop {
        if let AppEvent::OpenFailed {
            path: failed,
            reason,
        } = event(&mut output).await
        {
            assert_eq!(failed, path.to_string_lossy());
            assert!(!reason.is_empty());
            break;
        }
    }
    assert_eq!(actor.document.to_sgf(), before);
    assert!(actor.move_trial.is_some());
}

#[tokio::test]
async fn unchanged_imported_slots_preserve_exact_java_properties_when_saved() {
    let (mut actor, _) = crate::actor::tests::actor();
    actor.open(JAVA.to_vec(), None).await.unwrap();
    assert_eq!(
        actor.export_document_with_analysis().unwrap().0,
        actor.document.to_sgf()
    );
}

#[tokio::test]
async fn saving_oversized_lz_counts_succeeds_and_publishes_one_warning_without_mutating_analysis() {
    let temp = tempfile::tempdir().unwrap();
    let (mut actor, mut output) = crate::actor::tests::actor();
    actor.directory = temp.path().to_owned();
    let bytes = b"(;SZ[5]LZ[Main 45 2147483648 2.5 1.2 0 rootVisits=2147483648\nmove C3 visits 2147483649 winrate 5500 edgeVisits 2147483650 pv C3 pvVisits 2147483651])";
    actor.open(bytes.to_vec(), None).await.unwrap();
    document_event(&mut output).await;
    let original = actor.stored_analysis(0).unwrap();
    let original_tree = actor.document.to_sgf();
    let revision = actor.revision;
    let path = temp.path().join("clamped.sgf");
    for _ in 0..2 {
        actor.save_document(path.clone()).await.unwrap();
        let view = document_event(&mut output).await;
        assert_eq!(view.file_path.as_deref(), path.to_str());
        assert!(!view.modified);
        let warnings: Vec<_> = view
            .warnings
            .iter()
            .filter(|warning| warning.property == "LZ")
            .collect();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].node, 0);
        assert_eq!(
            warnings[0].kind,
            crate::WarningKindView::OutOfRange {
                fallback: i32::MAX.to_string()
            }
        );
        assert_eq!(actor.stored_analysis(0).unwrap(), original);
        assert_eq!(actor.document.to_sgf(), original_tree);
        assert_eq!(actor.revision, revision);
    }
    let bytes = tokio::fs::read(&path).await.unwrap();
    let collection = sgf::parse(&bytes).unwrap();
    let reopened = document::GameDocument::from_sgf(&collection.games[0]).unwrap();
    let saved = reopened.saved_analysis(reopened.root()).unwrap().unwrap();
    assert_eq!(saved.visits, i32::MAX as u64);
    assert_eq!(saved.candidates[0].visits, i32::MAX as u64);
    assert_eq!(saved.candidates[0].edge_visits, Some(i32::MAX as u64));
    assert_eq!(saved.candidates[0].pv_visits, [i32::MAX as u64]);
}

#[tokio::test]
async fn failed_save_does_not_publish_a_truncation_warning_for_a_file_that_was_not_written() {
    let temp = tempfile::tempdir().unwrap();
    let (mut actor, _) = crate::actor::tests::actor();
    actor.directory = temp.path().to_owned();
    actor
        .open(b"(;SZ[5]LZ[Main 45 2147483648])".to_vec(), None)
        .await
        .unwrap();
    assert!(
        actor
            .save_document(temp.path().join("missing/game.sgf"))
            .await
            .is_err()
    );
    assert!(actor.file_state.save_warnings.is_empty());
    assert!(actor.file_state.path.is_none());
}
