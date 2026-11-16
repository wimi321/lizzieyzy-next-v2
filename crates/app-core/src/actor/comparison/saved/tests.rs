#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::actor::{Input, Signal, ViewFocus};
use crate::{AnalysisSource, AppSubscription, DocumentView, EngineRules, LayoutMode};
use document::{BoardTransform, Command, GameDocument};
use std::{future::Future, time::Duration};

const JAVA: &[u8] = include_bytes!("../../../../../../fixtures/lz/java-analysis.sgf");

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("operation deadline")
}

async fn document_event(output: &mut AppSubscription) -> DocumentView {
    within(async {
        loop {
            if let AppEvent::DocumentChanged { view, .. } = output.recv().await.unwrap() {
                return view;
            }
        }
    })
    .await
}

async fn comparison_event(output: &mut AppSubscription) -> AnalysisView {
    within(async {
        loop {
            if let AppEvent::ComparisonAnalysisUpdated(view) = output.recv().await.unwrap() {
                return view;
            }
        }
    })
    .await
}

fn exported(actor: &Actor) -> GameDocument {
    GameDocument::from_sgf(&actor.export_document_with_analysis().unwrap().0).unwrap()
}

fn focus(actor: &mut Actor) {
    let node = actor.document.current();
    let position = actor.document.position_at(node).unwrap();
    actor.focus = Some(ViewFocus {
        revision: actor.revision,
        node_id: node.0,
        key: position.analysis_key(),
        size: position.board.size(),
        to_play: position.board.to_play(),
    });
}

fn comparison_snapshot(actor: &mut Actor, visits: u64, winrate: f64) {
    focus(actor);
    actor.comparison_snapshot(crate::actor::tests::snapshot(actor, visits, winrate));
}

#[tokio::test]
async fn saved_comparison_is_visible_only_in_double_layout_without_a_running_engine() {
    let (mut actor, mut output) = crate::actor::tests::actor();
    within(actor.open(JAVA.to_vec(), None)).await.unwrap();
    assert!(document_event(&mut output).await.has_comparison_analysis);
    assert!(actor.running.is_none());
    assert!(actor.comparison.running.is_none());
    assert!(actor.comparison_view().analysis.is_none());
    actor.settings.display.layout_mode = LayoutMode::DoubleEngine;
    actor.publish_document().unwrap();
    let document = document_event(&mut output).await;
    let view = comparison_event(&mut output).await;
    assert_eq!(view.source, AnalysisSource::Saved);
    assert_eq!(
        (view.node_id, view.revision),
        (document.tree.current, actor.revision)
    );
    assert_eq!(view.visits, 23456);
    assert_eq!(view.candidates[0].coordinate, "D4");
    assert!((view.winrate - 0.45).abs() < 1e-12);
    assert!(view.ownership.is_none()); // The Java fixture's secondary array has only two entries.
    assert_eq!(document.graph.points[0].summary.unwrap().visits, 12345);
    assert_eq!(
        actor.review_view(0).unwrap().unwrap().candidates[0].coordinate,
        "C3"
    );
    within(actor.apply(Input::Edit(Command::Step(1))))
        .await
        .unwrap();
    assert!(actor.comparison_view().analysis.is_none());
    assert_eq!(actor.review_view(1).unwrap().unwrap().visits, 2000);
    actor.settings.display.layout_mode = LayoutMode::Normal;
    within(actor.apply(Input::Edit(Command::Step(-1))))
        .await
        .unwrap();
    assert!(actor.comparison_view().analysis.is_none());
}

#[tokio::test]
async fn comparison_file_and_live_results_never_enter_primary_views_or_graph() {
    let (mut actor, mut output) = crate::actor::tests::actor();
    actor.settings.display.layout_mode = LayoutMode::DoubleEngine;
    within(actor.open(b"(;SZ[5]LZOP2[Second 45 100 2])".to_vec(), None))
        .await
        .unwrap();
    let document = document_event(&mut output).await;
    assert!(
        document
            .graph
            .points
            .iter()
            .all(|point| point.summary.is_none())
    );
    assert!(actor.stored_analysis(0).is_none());
    assert!(actor.review_view(0).unwrap().is_none());
    assert!(
        actor
            .problem_moves(Default::default())
            .unwrap()
            .entries
            .is_empty()
    );
    for visits in [50, 100] {
        comparison_snapshot(&mut actor, visits, 0.9);
        let view = comparison_event(&mut output).await;
        assert_eq!(view.source, AnalysisSource::Saved);
        assert_eq!(view.visits, 100);
    }
    comparison_snapshot(&mut actor, 101, 0.9);
    assert_eq!(
        comparison_event(&mut output).await.source,
        AnalysisSource::Live
    );
    comparison_snapshot(&mut actor, 30, 0.2);
    assert_eq!(
        actor
            .comparison_stored(NodeId(0))
            .unwrap()
            .response
            .root_info
            .visits,
        101
    );
    assert!(actor.stored_analysis(0).is_none());
    let main = crate::actor::tests::snapshot(&actor, 500, 0.7);
    actor.signal(Signal::Snapshot(Box::new(main)));
    let saved = exported(&actor);
    assert_eq!(
        saved.saved_analysis(NodeId(0)).unwrap().unwrap().visits,
        500
    );
    let comparison = saved.saved_comparison_analysis(NodeId(0)).unwrap().unwrap();
    assert_eq!(comparison.visits, 101);
    assert_eq!(comparison.winrate, 0.9);
    actor.publish_document().unwrap();
    assert_eq!(
        document_event(&mut output).await.graph.points[0]
            .summary
            .unwrap()
            .visits,
        500
    );
}

#[tokio::test]
async fn rule_and_komi_overrides_never_export_new_comparison_evaluations_or_revive_replaced_raw() {
    for override_komi in [false, true] {
        let (mut actor, _) = crate::actor::tests::actor();
        actor.settings.display.layout_mode = LayoutMode::DoubleEngine;
        within(actor.open(JAVA.to_vec(), None)).await.unwrap();
        if override_komi {
            actor.settings.engine_settings.read_komi = false;
            actor.settings.engine_settings.default_komi = 6.5;
        } else {
            actor.settings.engine_settings.rules = EngineRules::Japanese;
        }
        // Original Saved data remains explicitly file sourced until replaced.
        assert!(
            exported(&actor)
                .saved_comparison_analysis(NodeId(0))
                .unwrap()
                .is_some()
        );
        comparison_snapshot(&mut actor, 1, 0.8);
        assert!(actor.comparison_stored(NodeId(0)).unwrap().saved.is_none());
        assert!(
            exported(&actor)
                .saved_comparison_analysis(NodeId(0))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            exported(&actor)
                .saved_analysis(NodeId(0))
                .unwrap()
                .unwrap()
                .visits,
            12345
        );
        actor.set_comparison_fingerprint(Some("different-profile".into()));
        assert!(
            exported(&actor)
                .saved_comparison_analysis(NodeId(0))
                .unwrap()
                .is_none()
        );
    }
}

#[tokio::test]
async fn clear_current_and_all_analysis_restore_both_slots_in_one_undo() {
    let (mut actor, mut output) = crate::actor::tests::actor();
    actor.settings.display.layout_mode = LayoutMode::DoubleEngine;
    within(actor.open(JAVA.to_vec(), None)).await.unwrap();
    let original = actor.export_document_with_analysis().unwrap().0;
    let original_comparison = actor.comparison.results.clone();
    for all in [false, true] {
        within(actor.clear_analysis(all)).await.unwrap();
        let view = document_event(&mut output).await;
        assert!(!view.has_comparison_analysis);
        assert!(view.can_undo);
        assert!(view.modified);
        assert!(actor.comparison.latest.is_none());
        assert!(actor.comparison_view().analysis.is_none());
        assert!(actor.comparison_stored(NodeId(0)).is_none());
        let saved = exported(&actor);
        assert!(saved.saved_analysis(NodeId(0)).unwrap().is_none());
        assert!(
            saved
                .saved_comparison_analysis(NodeId(0))
                .unwrap()
                .is_none()
        );
        assert_eq!(saved.saved_analysis(NodeId(1)).unwrap().is_none(), all);
        within(actor.apply(Input::Undo)).await.unwrap();
        assert_eq!(actor.export_document_with_analysis().unwrap().0, original);
        assert_eq!(actor.comparison.results, original_comparison);
        assert_eq!(
            comparison_event(&mut output).await.source,
            AnalysisSource::Saved
        );
    }
    within(actor.apply(Input::Redo)).await.unwrap();
    assert!(actor.comparison.results.is_empty());
}

#[tokio::test]
async fn clearing_comparison_only_live_results_records_history_and_checks_its_own_fingerprint() {
    for change_identity in [false, true] {
        let (mut actor, _) = crate::actor::tests::actor();
        actor.settings.display.layout_mode = LayoutMode::DoubleEngine;
        actor.set_comparison_fingerprint(Some("comparison-before".into()));
        comparison_snapshot(&mut actor, 25, 0.9);
        within(actor.clear_analysis(false)).await.unwrap();
        assert!(actor.file_state.analysis_edits.can_undo(&actor.document));
        assert!(actor.comparison_stored(NodeId(0)).is_none());
        // A changed main engine must not invalidate comparison undo data.
        actor.set_summary_fingerprint(Some("unrelated-main".into()));
        if change_identity {
            actor.set_comparison_fingerprint(Some("comparison-after".into()));
        }
        within(actor.apply(Input::Undo)).await.unwrap();
        assert_eq!(
            actor.comparison_stored(NodeId(0)).is_none(),
            change_identity
        );
        assert!(actor.stored_analysis(0).is_none());
    }
}

#[tokio::test]
async fn fresh_comparison_after_clear_exports_even_when_shallower_than_old_raw_and_roundtrips() {
    let directory = tempfile::tempdir().unwrap();
    let (mut actor, _) = crate::actor::tests::actor();
    actor.directory = directory.path().to_owned();
    actor.settings.display.layout_mode = LayoutMode::DoubleEngine;
    within(actor.open(JAVA.to_vec(), None)).await.unwrap();
    within(actor.clear_analysis(false)).await.unwrap();
    comparison_snapshot(&mut actor, 5, 0.8);
    let path = directory.path().join("comparison.sgf");
    within(actor.save_document(path.clone())).await.unwrap();
    let bytes = within(tokio::fs::read(&path)).await.unwrap();
    let reopened = GameDocument::from_sgf(&sgf::parse(&bytes).unwrap().games[0]).unwrap();
    let saved = reopened
        .saved_comparison_analysis(NodeId(0))
        .unwrap()
        .unwrap();
    assert_eq!(saved.visits, 5);
    assert_eq!(saved.winrate, 0.8);
    assert!(reopened.saved_analysis(NodeId(0)).unwrap().is_none());
    within(actor.clear_analysis(false)).await.unwrap();
    within(actor.apply(Input::Undo)).await.unwrap();
    assert_eq!(
        actor
            .comparison_stored(NodeId(0))
            .unwrap()
            .response
            .root_info
            .visits,
        5
    );
    within(actor.open(bytes, Some(path))).await.unwrap();
    assert_eq!(
        actor.comparison_view().analysis.unwrap().source,
        AnalysisSource::Saved
    );
    assert!(actor.stored_analysis(0).is_none());
}

#[tokio::test]
async fn secondary_only_transform_reimports_new_coordinates_and_undo_redo_without_stale_latest() {
    let (mut actor, _) = crate::actor::tests::actor();
    actor.settings.display.layout_mode = LayoutMode::DoubleEngine;
    within(
        actor.open(
            b"(;SZ[5]LZOP2[Second 45 20 2\nmove A5 visits 20 winrate 5500 scoreMean 2 pv A5 B4])"
                .to_vec(),
            None,
        ),
    )
    .await
    .unwrap();
    actor.comparison.latest = actor.comparison_focus_view();
    within(actor.apply(Input::Edit(Command::Transform(BoardTransform::RotateRight))))
        .await
        .unwrap();
    assert_eq!(
        actor.comparison_view().analysis.unwrap().candidates[0].coordinate,
        "E5"
    );
    assert!(actor.comparison.latest.is_none());
    assert!(actor.stored_analysis(0).is_none());
    within(actor.apply(Input::Undo)).await.unwrap();
    assert_eq!(
        actor.comparison_view().analysis.unwrap().candidates[0].coordinate,
        "A5"
    );
    within(actor.apply(Input::Redo)).await.unwrap();
    assert_eq!(
        actor.comparison_view().analysis.unwrap().candidates[0].coordinate,
        "E5"
    );
    assert_eq!(
        exported(&actor)
            .saved_comparison_analysis(NodeId(0))
            .unwrap()
            .unwrap()
            .candidates[0]
            .coordinate,
        "E5"
    );
}

#[tokio::test]
async fn editing_komi_invalidates_the_view_but_preserves_untouched_comparison_raw_and_undo_restores()
 {
    let (mut actor, _) = crate::actor::tests::actor();
    actor.settings.display.layout_mode = LayoutMode::DoubleEngine;
    within(actor.open(b"(;SZ[5]KM[7.5]LZ2[Second 45 100])".to_vec(), None))
        .await
        .unwrap();
    let old = actor.document.game_info().unwrap();
    let edit = document::GameInfoEdit {
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
    within(actor.apply(Input::Edit(Command::SetGameInfo(edit))))
        .await
        .unwrap();
    assert!(actor.comparison_stored(NodeId(0)).is_none());
    assert!(actor.comparison_view().analysis.is_none());
    assert!(
        exported(&actor)
            .saved_comparison_analysis(NodeId(0))
            .unwrap()
            .is_some()
    );
    within(actor.apply(Input::Undo)).await.unwrap();
    assert_eq!(actor.comparison_view().analysis.unwrap().visits, 100);
    assert!(actor.stored_analysis(0).is_none());
}

#[tokio::test]
async fn skipped_import_with_search_parameters_preserves_secondary_raw_until_a_new_result_replaces_it()
 {
    let (mut actor, _) = crate::actor::tests::actor();
    actor.settings.display.layout_mode = LayoutMode::DoubleEngine;
    actor.settings.search_params = Some(crate::SearchParams {
        playout_doubling_advantage: Some(1.0),
        ..Default::default()
    });
    within(actor.open(JAVA.to_vec(), None)).await.unwrap();
    assert!(actor.comparison.results.is_empty());
    let raw_secondary = |doc: &GameDocument| {
        doc.node(NodeId(0))
            .unwrap()
            .properties()
            .iter()
            .filter(|p| matches!(p.identifier.as_str(), "LZ2" | "LZOP2"))
            .cloned()
            .collect::<Vec<_>>()
    };
    let original = raw_secondary(&actor.document);
    assert!(!original.is_empty());
    assert_eq!(raw_secondary(&exported(&actor)), original);
    within(actor.clear_analysis(false)).await.unwrap();
    assert!(raw_secondary(&exported(&actor)).is_empty());
    within(actor.apply(Input::Undo)).await.unwrap();
    assert_eq!(raw_secondary(&exported(&actor)), original);
    comparison_snapshot(&mut actor, 1, 0.8);
    assert!(actor.comparison.replaced_raw.contains(&NodeId(0)));
    assert!(raw_secondary(&exported(&actor)).is_empty());
    actor.set_comparison_fingerprint(Some("changed-identity".into()));
    assert!(actor.comparison.results.is_empty());
    assert!(raw_secondary(&exported(&actor)).is_empty());
}

#[tokio::test]
async fn transform_saved_in_one_slot_does_not_reimport_replaced_raw_in_the_other_slot() {
    for secondary_is_live in [false, true] {
        let (mut actor, _) = crate::actor::tests::actor();
        actor.settings.display.layout_mode = LayoutMode::DoubleEngine;
        within(actor.open(
            b"(;SZ[5]LZ[Main 45 100 2\nmove A5 visits 100 winrate 5500 scoreMean 2 pv A5]LZ2[Second 40 200 3\nmove E1 visits 200 winrate 6000 scoreMean 3 pv E1])".to_vec(),
            None,
        )).await.unwrap();
        actor.settings.engine_settings.read_komi = false;
        focus(&mut actor);
        if secondary_is_live {
            comparison_snapshot(&mut actor, 1, 0.8);
        } else {
            let fresh = crate::actor::tests::snapshot(&actor, 1, 0.8);
            actor.signal(Signal::Snapshot(Box::new(fresh)));
        }
        for command in [
            Input::Edit(Command::Transform(BoardTransform::RotateRight)),
            Input::Undo,
            Input::Redo,
        ] {
            within(actor.apply(command)).await.unwrap();
            let main = actor.stored_analysis(0).unwrap();
            let comparison = actor.comparison_stored(NodeId(0)).unwrap();
            assert_eq!(main.saved.is_none(), !secondary_is_live);
            assert_eq!(comparison.saved.is_none(), secondary_is_live);
            assert_eq!(
                if secondary_is_live {
                    comparison.response.root_info.visits
                } else {
                    main.response.root_info.visits
                },
                1
            );
        }
    }
}

const TEMPORARY_SOURCE: &[u8] =
    b"(;SZ[19]KM[6.5]RU[japanese]AB[aa][ab][ba]AW[bb][bc]PL[B]LZ2[Second 45 100 2](;B[ac])(;B[cc]))";

async fn enter_temporary(actor: &mut Actor, tsumego: bool) {
    if tsumego {
        within(actor.generate_tsumego(Default::default()))
            .await
            .unwrap();
    } else {
        within(actor.start_move_trial()).await.unwrap();
    }
}

async fn leave_temporary(actor: &mut Actor, tsumego: bool) {
    if tsumego {
        within(actor.exit_tsumego()).await.unwrap();
    } else {
        within(actor.end_move_trial()).await.unwrap();
    }
}

#[tokio::test]
async fn temporary_boards_restore_deepest_comparison_before_publishing_original_focus() {
    for tsumego in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let (mut actor, mut output) = crate::actor::tests::actor();
        actor.directory = directory.path().to_owned();
        actor.settings.display.layout_mode = LayoutMode::DoubleEngine;
        within(actor.open(TEMPORARY_SOURCE.to_vec(), None))
            .await
            .unwrap();
        actor.set_comparison_fingerprint(Some("original-engine".into()));
        comparison_snapshot(&mut actor, 101, 0.8);
        let original = actor.document.to_sgf();
        enter_temporary(&mut actor, tsumego).await;
        comparison_snapshot(&mut actor, 999, 0.2);
        if tsumego {
            // Regenerating keeps the first document's backup, not the latest
            // generated board's results, even though both roots use node 0.
            enter_temporary(&mut actor, true).await;
            comparison_snapshot(&mut actor, 888, 0.3);
        }
        leave_temporary(&mut actor, tsumego).await;
        assert_eq!(actor.document.to_sgf(), original);
        assert!(actor.comparison.latest.is_none());
        let published = document_event(&mut output).await;
        assert_eq!(published.tree.current, 0);
        assert!(published.has_comparison_analysis);
        let view = comparison_event(&mut output).await;
        assert_eq!((view.node_id, view.revision), (0, actor.revision));
        assert_eq!(view.source, AnalysisSource::Cache);
        assert_eq!((view.visits, view.winrate), (101, 0.8));
        assert_eq!(
            exported(&actor)
                .saved_comparison_analysis(NodeId(0))
                .unwrap()
                .unwrap()
                .visits,
            101
        );
        assert!(actor.stored_analysis(0).is_none());
    }
}

#[tokio::test]
async fn temporary_boards_preserve_replaced_raw_markers_but_reject_changed_engine_results() {
    for tsumego in [false, true] {
        for change_engine in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let (mut actor, _) = crate::actor::tests::actor();
            actor.directory = directory.path().to_owned();
            actor.settings.display.layout_mode = LayoutMode::DoubleEngine;
            within(actor.open(TEMPORARY_SOURCE.to_vec(), None))
                .await
                .unwrap();
            actor.settings.engine_settings.read_komi = false;
            actor.set_comparison_fingerprint(Some("original-engine".into()));
            comparison_snapshot(&mut actor, 1, 0.8);
            assert!(
                exported(&actor)
                    .saved_comparison_analysis(NodeId(0))
                    .unwrap()
                    .is_none()
            );
            enter_temporary(&mut actor, tsumego).await;
            if change_engine {
                actor.set_comparison_fingerprint(Some("changed-engine".into()));
            }
            let current_identity = actor.comparison.fingerprint.clone();
            comparison_snapshot(&mut actor, 999, 0.2);
            leave_temporary(&mut actor, tsumego).await;
            assert_eq!(actor.comparison.fingerprint, current_identity);
            assert!(actor.comparison.replaced_raw.contains(&NodeId(0)));
            let result = actor.comparison_stored(NodeId(0));
            if change_engine {
                assert!(result.is_none());
                assert!(actor.comparison_view().analysis.is_none());
            } else {
                assert_eq!(result.unwrap().response.root_info.visits, 1);
            }
            assert!(
                exported(&actor)
                    .saved_comparison_analysis(NodeId(0))
                    .unwrap()
                    .is_none()
            );
        }
    }
}

#[tokio::test]
async fn temporary_boards_restore_saved_comparison_and_its_clear_undo_history() {
    for tsumego in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let (mut actor, _) = crate::actor::tests::actor();
        actor.directory = directory.path().to_owned();
        actor.settings.display.layout_mode = LayoutMode::DoubleEngine;
        within(actor.open(TEMPORARY_SOURCE.to_vec(), None))
            .await
            .unwrap();
        for clear in [false, true] {
            if clear {
                within(actor.clear_analysis(false)).await.unwrap();
            }
            enter_temporary(&mut actor, tsumego).await;
            actor.set_comparison_fingerprint(Some("new-engine".into()));
            leave_temporary(&mut actor, tsumego).await;
            if clear {
                assert!(actor.comparison_stored(NodeId(0)).is_none());
                assert!(
                    exported(&actor)
                        .saved_comparison_analysis(NodeId(0))
                        .unwrap()
                        .is_none()
                );
                within(actor.apply(Input::Undo)).await.unwrap();
            }
            let view = actor.comparison_view().analysis.unwrap();
            assert_eq!((view.source, view.visits), (AnalysisSource::Saved, 100));
            assert_eq!(
                exported(&actor)
                    .saved_comparison_analysis(NodeId(0))
                    .unwrap()
                    .unwrap()
                    .visits,
                100
            );
        }
    }
}

#[tokio::test]
async fn temporary_boards_do_not_restore_saved_comparison_under_new_search_parameters() {
    for tsumego in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let (mut actor, _) = crate::actor::tests::actor();
        actor.directory = directory.path().to_owned();
        actor.settings.display.layout_mode = LayoutMode::DoubleEngine;
        within(actor.open(TEMPORARY_SOURCE.to_vec(), None))
            .await
            .unwrap();
        enter_temporary(&mut actor, tsumego).await;
        actor.settings.search_params = Some(crate::SearchParams {
            playout_doubling_advantage: Some(1.0),
            ..Default::default()
        });
        leave_temporary(&mut actor, tsumego).await;
        assert!(actor.comparison_view().analysis.is_none());
        assert!(actor.comparison.results.is_empty());
        assert_eq!(
            exported(&actor)
                .saved_comparison_analysis(NodeId(0))
                .unwrap()
                .unwrap()
                .visits,
            100
        );
    }
}
