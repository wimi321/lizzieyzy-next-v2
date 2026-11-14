#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::{AppEvent, AppSubscription, DocumentView, actor::Input};
use document::Command;
use std::time::Duration;

const JAVA: &[u8] = include_bytes!("../../../../../fixtures/lz/java-analysis.sgf");

async fn view(output: &mut AppSubscription) -> DocumentView {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(AppEvent::DocumentChanged { view, .. }) = output.recv().await {
                return view;
            }
        }
    })
    .await
    .unwrap()
}

fn exported(actor: &Actor) -> GameDocument {
    GameDocument::from_sgf(&actor.export_document_with_analysis().unwrap().0).unwrap()
}

#[tokio::test]
async fn clear_current_analysis_is_an_undoable_file_edit_with_stable_nodes() {
    let (mut actor, mut output) = crate::actor::tests::actor();
    actor.open(JAVA.to_vec(), None).await.unwrap();
    let original = actor.review.results.clone();
    let original_tree = actor.document.to_sgf();
    let initial = view(&mut output).await;
    assert!(!initial.modified);
    actor.clear_analysis(false).await.unwrap();
    let cleared = view(&mut output).await;
    assert!(cleared.modified);
    assert!(cleared.can_undo);
    assert!(!cleared.can_redo);
    assert_eq!(cleared.tree.current, 0);
    assert!(cleared.graph.points[0].summary.is_none());
    assert!(actor.stored_analysis(0).is_none());
    assert!(actor.stored_analysis(1).is_some());
    let saved = exported(&actor);
    assert!(saved.saved_analysis(NodeId(0)).unwrap().is_none());
    assert!(saved.saved_analysis(NodeId(1)).unwrap().is_some());
    assert_eq!(actor.document.to_sgf(), original_tree);
    actor.apply(Input::Undo).await.unwrap();
    let restored = view(&mut output).await;
    assert!(!restored.modified);
    assert!(!restored.can_undo);
    assert!(restored.can_redo);
    assert_eq!(actor.review.results, original);
    assert_eq!(
        actor.export_document_with_analysis().unwrap().0,
        original_tree
    );
    actor.apply(Input::Redo).await.unwrap();
    assert!(view(&mut output).await.modified);
    assert!(actor.stored_analysis(0).is_none());
}

#[tokio::test]
async fn analysis_edits_interleave_with_comments_and_navigation_without_losing_redo() {
    let (mut actor, mut output) = crate::actor::tests::actor();
    actor.open(JAVA.to_vec(), None).await.unwrap();
    actor
        .apply(Input::Edit(Command::SetComment("before clear".into())))
        .await
        .unwrap();
    actor.clear_analysis(true).await.unwrap();
    actor.apply(Input::Edit(Command::Step(1))).await.unwrap();
    actor
        .apply(Input::Edit(Command::SetComment("after clear".into())))
        .await
        .unwrap();
    actor.apply(Input::Undo).await.unwrap();
    assert!(actor.review.results.is_empty());
    actor.apply(Input::Undo).await.unwrap();
    assert_eq!(actor.document.current(), NodeId(0));
    assert!(actor.stored_analysis(0).is_some());
    assert!(actor.stored_analysis(1).is_some());
    assert_eq!(
        actor.document.node(NodeId(0)).unwrap().comment(),
        Some("before clear")
    );
    actor.apply(Input::Undo).await.unwrap();
    assert!(!view(&mut output).await.modified);
    actor.apply(Input::Redo).await.unwrap();
    actor.apply(Input::Redo).await.unwrap();
    assert!(actor.review.results.is_empty());
    actor.apply(Input::Redo).await.unwrap();
    assert_eq!(
        actor.document.node(NodeId(1)).unwrap().comment(),
        Some("after clear")
    );
}

#[tokio::test]
async fn clearing_all_removes_malformed_and_secondary_slots_in_every_variation() {
    let (mut actor, mut output) = crate::actor::tests::actor();
    actor
        .open(
            b"(;SZ[5]C[keep]LZ[bad]LZ2[bad2](;B[aa]LZOP[broken])(;B[bb]LZOP2[broken2]TR[cc]))"
                .to_vec(),
            None,
        )
        .await
        .unwrap();
    let original = actor.document.to_sgf();
    assert_eq!(view(&mut output).await.warnings.len(), 4);
    actor.clear_analysis(true).await.unwrap();
    let cleared = view(&mut output).await;
    assert!(cleared.warnings.is_empty());
    let saved = exported(&actor);
    for node in saved.variation_tree().unwrap() {
        assert!(
            saved
                .node(node.id)
                .unwrap()
                .properties()
                .iter()
                .all(|p| !is_analysis(&p.identifier))
        );
    }
    assert_eq!(saved.node(NodeId(0)).unwrap().comment(), Some("keep"));
    assert_eq!(saved.markup_at(NodeId(2)).unwrap().len(), 1);
    actor.apply(Input::Undo).await.unwrap();
    assert_eq!(actor.export_document_with_analysis().unwrap().0, original);
}

#[tokio::test]
async fn clear_save_reopen_and_undo_preserve_the_saved_baseline_and_edit_history() {
    let directory = tempfile::tempdir().unwrap();
    let (mut actor, mut output) = crate::actor::tests::actor();
    actor.directory = directory.path().to_owned();
    actor.open(JAVA.to_vec(), None).await.unwrap();
    actor.clear_analysis(true).await.unwrap();
    let path = directory.path().join("cleared.sgf");
    actor.save_document(path.clone()).await.unwrap();
    assert!(!view(&mut output).await.modified);
    actor.apply(Input::Undo).await.unwrap();
    let undone = view(&mut output).await;
    assert!(undone.modified);
    assert!(actor.stored_analysis(0).is_some());
    actor.apply(Input::Redo).await.unwrap();
    assert!(!view(&mut output).await.modified);
    let bytes = tokio::fs::read(&path).await.unwrap();
    actor.open(bytes, Some(path)).await.unwrap();
    assert!(actor.review.results.is_empty());
    let reopened = view(&mut output).await;
    assert!(!reopened.modified);
    assert!(!reopened.can_undo);
}

#[tokio::test]
async fn no_op_clear_does_not_create_history_and_new_clear_discards_old_redo() {
    let (mut actor, _) = crate::actor::tests::actor();
    let revision = actor.revision;
    actor.clear_analysis(false).await.unwrap();
    assert_eq!(actor.revision, revision);
    assert!(!actor.file_state.analysis_edits.can_undo(&actor.document));
    actor.open(JAVA.to_vec(), None).await.unwrap();
    actor
        .apply(Input::Edit(Command::SetComment("discard me".into())))
        .await
        .unwrap();
    actor.apply(Input::Undo).await.unwrap();
    actor.clear_analysis(false).await.unwrap();
    assert!(!actor.file_state.analysis_edits.can_redo(&actor.document));
    assert!(matches!(
        actor.apply(Input::Redo).await,
        Err(AppError::Document(document::DocError::NothingToRedo))
    ));
    actor.apply(Input::Undo).await.unwrap();
    assert!(actor.stored_analysis(0).is_some());
    // The domain's abandoned comment redo must remain inaccessible.
    actor.apply(Input::Redo).await.unwrap();
    assert!(actor.stored_analysis(0).is_none());
    assert!(actor.apply(Input::Redo).await.is_err());
}

#[tokio::test]
async fn fresh_result_after_clear_is_exported_and_a_second_clear_marks_modified() {
    let directory = tempfile::tempdir().unwrap();
    let (mut actor, mut output) = crate::actor::tests::actor();
    actor.directory = directory.path().to_owned();
    actor.open(JAVA.to_vec(), None).await.unwrap();
    actor.clear_analysis(false).await.unwrap();
    let position = actor.document.position_at(NodeId(0)).unwrap();
    actor.focus = Some(crate::actor::ViewFocus {
        revision: actor.revision,
        node_id: 0,
        key: position.analysis_key(),
        size: position.board.size(),
        to_play: position.board.to_play(),
    });
    let fresh = crate::actor::tests::snapshot(&actor, 5, 0.7);
    actor.signal(crate::actor::Signal::Snapshot(Box::new(fresh)));
    assert_eq!(
        exported(&actor)
            .saved_analysis(NodeId(0))
            .unwrap()
            .unwrap()
            .visits,
        5
    );
    actor
        .save_document(directory.path().join("fresh.sgf"))
        .await
        .unwrap();
    assert!(!view(&mut output).await.modified);
    actor.clear_analysis(false).await.unwrap();
    assert!(view(&mut output).await.modified);
    assert!(
        exported(&actor)
            .saved_analysis(NodeId(0))
            .unwrap()
            .is_none()
    );
    actor.apply(Input::Undo).await.unwrap();
    assert!(!view(&mut output).await.modified);
    assert_eq!(
        actor.stored_analysis(0).unwrap().response.root_info.visits,
        5
    );
}

#[tokio::test]
async fn trial_restores_analysis_edit_history_and_original_clear_markers() {
    let (mut actor, mut output) = crate::actor::tests::actor();
    actor.open(JAVA.to_vec(), None).await.unwrap();
    actor.clear_analysis(false).await.unwrap();
    actor.start_move_trial().await.unwrap();
    actor
        .apply(Input::Edit(Command::SetComment("temporary".into())))
        .await
        .unwrap();
    actor.end_move_trial().await.unwrap();
    assert!(view(&mut output).await.can_undo);
    actor.apply(Input::Undo).await.unwrap();
    assert!(actor.stored_analysis(0).is_some());
    assert!(!actor.is_analysis_cleared(NodeId(0)));
    assert!(!view(&mut output).await.modified);
}

#[tokio::test]
async fn current_clear_follows_node_identity_after_variation_reordering() {
    let (mut actor, _) = crate::actor::tests::actor();
    actor
        .open(
            b"(;SZ[5](;B[aa]LZ2[first])(;B[bb]LZ2[second]))".to_vec(),
            None,
        )
        .await
        .unwrap();
    actor
        .apply(Input::Edit(Command::GoTo(NodeId(2))))
        .await
        .unwrap();
    actor.clear_analysis(false).await.unwrap();
    actor
        .apply(Input::Edit(Command::PromoteToMain(NodeId(2))))
        .await
        .unwrap();
    let tree = actor.export_document_with_analysis().unwrap().0;
    assert!(
        tree.children[0].sequence[0]
            .properties
            .iter()
            .all(|p| !is_analysis(&p.identifier))
    );
    assert!(
        tree.children[1].sequence[0]
            .properties
            .iter()
            .any(|p| p.identifier == "LZ2" && p.values == ["first"])
    );
    actor.apply(Input::Undo).await.unwrap();
    actor.apply(Input::Undo).await.unwrap();
    let tree = actor.export_document_with_analysis().unwrap().0;
    assert!(
        tree.children[1].sequence[0]
            .properties
            .iter()
            .any(|p| p.identifier == "LZ2" && p.values == ["second"])
    );
}

#[tokio::test]
async fn fake_engine_clear_undo_and_redo_preserve_automatic_time_and_playout_stops() {
    let program = fake_katago::binary_path().unwrap();
    for time_limit in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let mut config = crate::AppConfig::default();
        config.analysis.live.max_visits = 1_000_000;
        config.analysis.live.report_interval = Duration::from_millis(10);
        let (core, mut output) = crate::spawn_with_config(directory.path(), config)
            .await
            .unwrap();
        core.set_auto_quick_analyze_on_load(false).await.unwrap();
        core.set_analysis_limits(crate::AnalysisLimitSettings {
            limit_time: time_limit,
            max_analyze_time_seconds: 1,
            limit_playout: !time_limit,
            limit_playouts: 1,
        })
        .await
        .unwrap();
        core.open_bytes(JAVA.to_vec()).await.unwrap();
        core.configure_engine(Some(crate::EngineProfile {
            human_model: None,
            program: program.clone(),
            model: program.clone(),
            config: program.clone(),
            extra_args: vec!["--visits-per-sec".into(), "10000".into()],
            canonical_model_name: None,
        }))
        .await
        .unwrap();
        let expected = if time_limit {
            crate::AnalysisPauseReason::TimeLimit
        } else {
            crate::AnalysisPauseReason::PlayoutLimit
        };
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Some(AppEvent::AnalysisStateChanged(state)) = output.recv().await
                    && state.pause_reason == Some(expected)
                {
                    break;
                }
            }
        })
        .await
        .unwrap();
        core.clear_analysis(false).await.unwrap();
        assert_eq!(
            core.analysis_state().await.unwrap().pause_reason,
            Some(expected)
        );
        assert!(core.whole_game_analysis(0).await.unwrap().is_none());
        core.undo().await.unwrap();
        assert_eq!(
            core.analysis_state().await.unwrap().pause_reason,
            Some(expected)
        );
        assert!(core.whole_game_analysis(0).await.unwrap().is_some());
        core.redo().await.unwrap();
        assert_eq!(
            core.analysis_state().await.unwrap().pause_reason,
            Some(expected)
        );
        assert!(core.whole_game_analysis(0).await.unwrap().is_none());
        core.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn new_live_after_clear_survives_coordinator_eviction_without_reviving_deeper_sqlite_data() {
    let program = fake_katago::binary_path().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let (mut actor, _) = crate::actor::tests::actor();
    actor.directory = directory.path().to_owned();
    actor.settings.pondering = false;
    actor.cache = Some(analysis::persistent_cache::PersistentCache::spawn(
        directory.path().join("cache.sqlite3"),
        analysis::persistent_cache::DEFAULT_MAX_BYTES,
        true,
    ));
    actor
        .apply(Input::ConfigureEngine(Some(crate::EngineProfile {
            human_model: None,
            program: program.clone(),
            model: program.clone(),
            config: program,
            extra_args: Vec::new(),
            canonical_model_name: None,
        })))
        .await
        .unwrap();
    let node = actor.document.current();
    let position = actor.document.position_at(node).unwrap();
    let old = crate::actor::tests::snapshot(&actor, 1000, 0.8);
    actor.persist_analysis(&old);
    actor.settings.pondering = true;
    actor.signal(crate::actor::Signal::Snapshot(Box::new(old)));
    actor.settings.pondering = false;
    actor.clear_analysis(false).await.unwrap();
    assert!(actor.cached_focus(node, &position).await.is_none());
    let fresh = crate::actor::tests::snapshot(&actor, 50, 0.3);
    actor.settings.pondering = true;
    actor.signal(crate::actor::Signal::Snapshot(Box::new(fresh.clone())));
    actor.settings.pondering = false;
    actor
        .running
        .as_ref()
        .unwrap()
        .coordinator
        .discard_cached_analysis()
        .await
        .unwrap();
    assert_eq!(
        actor.cached_focus(node, &position).await,
        Some(fresh.response)
    );
    // Removing only the document-edit marker exposes the old database result,
    // proving both that clear did not erase it and that the marker blocked it.
    let token = actor
        .file_state
        .analysis_edits
        .cleared
        .remove(&node)
        .unwrap();
    assert_eq!(
        actor
            .cached_focus(node, &position)
            .await
            .unwrap()
            .root_info
            .visits,
        1000
    );
    actor.file_state.analysis_edits.cleared.insert(node, token);
    actor.stop_engine().await.unwrap();
    actor.cache.as_ref().unwrap().shutdown().await;
}
