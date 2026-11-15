#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::{
    AppEvent, AppSubscription, EngineProfile,
    actor::{Input, Signal},
    test_support::within,
};
use analysis::{
    SnapshotSource,
    persistent_cache::{CacheStatus, DEFAULT_MAX_BYTES, PersistentCache},
};
use document::{GameDocument, NodeId};

fn points() -> Vec<PointView> {
    vec![PointView { x: 3, y: 3 }]
}

#[tokio::test]
async fn paused_restricted_extras_and_restricted_pure_network_only_refresh_research_view() {
    for pure_network in [false, true] {
        let (mut actor, mut output) = crate::actor::tests::actor();
        let ordinary = crate::actor::tests::snapshot(&actor, 100, 0.5);
        actor.signal(Signal::Snapshot(Box::new(ordinary)));
        let summaries = actor.summaries.clone();
        let retained = actor.review.results.clone();
        within(actor.change_restriction(RestrictionKind::Allow, points(), false))
            .await
            .unwrap();
        actor.settings.pondering = false;
        actor.config.analysis.pure_network = pure_network;
        let mut snapshot = crate::actor::tests::snapshot(&actor, 1, 0.95);
        snapshot.source = if pure_network {
            SnapshotSource::PureNetwork
        } else {
            SnapshotSource::Restricted
        };
        snapshot.response.policy = Some(vec![0.0; 362]);
        actor.signal(Signal::Snapshot(Box::new(snapshot)));
        let view = within(async {
            loop {
                match output.recv().await.unwrap() {
                    AppEvent::DocumentChanged { view, .. }
                        if view.restricted_analysis.is_some() =>
                    {
                        break view;
                    }
                    AppEvent::AnalysisUpdated(_) => {
                        panic!("restricted foreground output reached the normal graph event")
                    }
                    _ => {}
                }
            }
        })
        .await;
        let research = view.restricted_analysis.unwrap();
        assert_eq!(research.visits, 1);
        assert_eq!(research.policy.unwrap().len(), 362);
        assert_eq!(actor.summaries, summaries);
        assert_eq!(actor.review.results, retained);
    }
}

async fn paused_actor(
    directory: &std::path::Path,
    program: std::path::PathBuf,
) -> (Actor, AppSubscription) {
    let (mut actor, output) = crate::actor::tests::actor();
    actor.directory = directory.to_owned();
    actor.settings.pondering = false;
    actor.settings.auto_quick_analyze_on_load = false;
    within(actor.apply(Input::ConfigureEngine(Some(EngineProfile {
        remote: None,
        human_model: None,
        canonical_model_name: None,
        program: program.clone(),
        model: program.clone(),
        config: program,
        extra_args: Vec::new(),
    }))))
    .await
    .unwrap();
    actor.cache = Some(PersistentCache::spawn(
        directory.join("analysis.sqlite3"),
        DEFAULT_MAX_BYTES,
        true,
    ));
    (actor, output)
}

async fn close(actor: &mut Actor) {
    within(actor.stop_engine()).await.unwrap();
    if let Some(cache) = &actor.cache {
        within(cache.shutdown()).await;
    }
}

async fn inject(actor: &mut Actor, visits: u64, winrate: f64, source: SnapshotSource) {
    let mut snapshot = crate::actor::tests::snapshot(actor, visits, winrate);
    snapshot.source = source;
    actor.settings.pondering = true;
    within(actor.cache_signal(Signal::Snapshot(Box::new(snapshot)))).await;
    actor.settings.pondering = false;
}

#[tokio::test]
async fn restricted_display_preserves_node_graph_sgf_and_persistent_cache_after_clear() {
    let program = fake_katago::binary_path().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let (mut actor, mut output) = within(paused_actor(directory.path(), program)).await;
    within(inject(&mut actor, 100, 0.65, SnapshotSource::Live)).await;
    let retained = actor.review.results.clone();
    let summaries = actor.summaries.clone();
    let sgf = actor.export_document_with_analysis().unwrap().0;
    let node = actor.document.current();
    let position = actor.document.position_at(node).unwrap();
    within(actor.change_restriction(RestrictionKind::Allow, points(), false))
        .await
        .unwrap();
    within(inject(&mut actor, 9999, 0.95, SnapshotSource::Restricted)).await;
    assert_eq!(actor.review.results, retained);
    assert_eq!(actor.summaries, summaries);
    assert_eq!(actor.export_document_with_analysis().unwrap().0, sgf);
    let displayed = within(async {
        loop {
            match output.recv().await.unwrap() {
                AppEvent::DocumentChanged { view, .. } if view.restricted_analysis.is_some() => {
                    break view;
                }
                AppEvent::AnalysisUpdated(_) => {
                    panic!("restricted result leaked into the ordinary graph event")
                }
                _ => {}
            }
        }
    })
    .await;
    assert_eq!(displayed.restricted_analysis.unwrap().visits, 9999);
    assert_eq!(displayed.graph.points[0].summary.unwrap().visits, 100);
    within(actor.install_restriction(None)).await.unwrap();
    assert!(actor.restricted_analysis.is_none());
    assert_eq!(actor.review.results, retained);
    // Remove only memory state to prove the database retained the ordinary
    // response rather than the much deeper restricted display.
    actor.review.results.clear();
    assert_eq!(
        within(actor.cached_focus(node, &position))
            .await
            .unwrap()
            .root_info
            .visits,
        100
    );
    within(close(&mut actor)).await;
}

#[tokio::test]
async fn restricted_results_create_no_node_analysis_or_sgf_payload() {
    let program = fake_katago::binary_path().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let (mut actor, _) = within(paused_actor(directory.path(), program)).await;
    within(actor.change_restriction(RestrictionKind::Avoid, points(), false))
        .await
        .unwrap();
    within(inject(&mut actor, 500, 0.99, SnapshotSource::Restricted)).await;
    assert!(actor.review.results.is_empty());
    assert!(actor.summaries.is_empty());
    let exported =
        GameDocument::from_sgf(&actor.export_document_with_analysis().unwrap().0).unwrap();
    assert!(exported.saved_analysis(NodeId(0)).unwrap().is_none());
    within(actor.install_restriction(None)).await.unwrap();
    let position = actor.document.position_at(NodeId(0)).unwrap();
    assert!(
        within(actor.cached_focus(NodeId(0), &position))
            .await
            .is_none()
    );
    within(close(&mut actor)).await;
}

#[tokio::test]
async fn restricted_focus_does_not_read_persistent_cache() {
    let program = fake_katago::binary_path().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let (mut actor, _) = within(paused_actor(directory.path(), program)).await;
    within(actor.cache.as_ref().unwrap().shutdown()).await;
    // A directory can never be a SQLite file. A forbidden read would disable
    // this cache; flush with no pending entries does not open it.
    let cache = PersistentCache::spawn(directory.path().to_owned(), DEFAULT_MAX_BYTES, true);
    actor.cache = Some(cache.clone());
    within(actor.change_restriction(RestrictionKind::Allow, points(), false))
        .await
        .unwrap();
    within(actor.update_focus()).await;
    assert_eq!(*cache.status().borrow(), CacheStatus::Enabled);
    within(actor.install_restriction(None)).await.unwrap();
    assert!(matches!(*cache.status().borrow(), CacheStatus::Failed(_)));
    within(close(&mut actor)).await;
}

#[tokio::test]
async fn search_parameters_isolate_retained_and_persistent_results_without_replacing_engine() {
    let program = fake_katago::binary_path().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let (mut actor, _) = within(paused_actor(directory.path(), program)).await;
    within(inject(&mut actor, 100, 0.65, SnapshotSource::Live)).await;
    let profile = actor.running.as_ref().unwrap().fingerprint.clone();
    let original_identity = actor.evaluation_fingerprint(&profile).unwrap();
    let original_status = actor.running.as_ref().unwrap().engine.status();
    let node = actor.document.current();
    let position = actor.document.position_at(node).unwrap();
    let params = SearchParams {
        playout_doubling_advantage: Some(1.0),
        wide_root_noise: Some(0.5),
        root_policy_temperature: Some(1.3),
    };
    within(actor.change_search_params(Some(params.clone())))
        .await
        .unwrap();
    assert!(original_status.same_channel(&actor.running.as_ref().unwrap().engine.status()));
    assert_ne!(
        actor.evaluation_fingerprint(&profile).unwrap(),
        original_identity
    );
    assert!(actor.review.results.is_empty());
    assert!(within(actor.cached_focus(node, &position)).await.is_none());
    within(inject(&mut actor, 50, 0.25, SnapshotSource::Live)).await;
    actor.review.results.clear();
    assert_eq!(
        within(actor.cached_focus(node, &position))
            .await
            .unwrap()
            .root_info
            .visits,
        50
    );
    within(actor.change_search_params(None)).await.unwrap();
    assert_eq!(
        actor.evaluation_fingerprint(&profile).unwrap(),
        original_identity
    );
    assert_eq!(
        within(actor.cached_focus(node, &position))
            .await
            .unwrap()
            .root_info
            .visits,
        100
    );
    within(actor.change_search_params(Some(params)))
        .await
        .unwrap();
    assert_eq!(
        within(actor.cached_focus(node, &position))
            .await
            .unwrap()
            .root_info
            .visits,
        50
    );
    within(close(&mut actor)).await;
}

#[tokio::test]
async fn restriction_edits_preserve_saved_analysis_and_open_clears_runtime_state() {
    let directory = tempfile::tempdir().unwrap();
    let (mut actor, _) = crate::actor::tests::actor();
    actor.directory = directory.path().to_owned();
    within(actor.open(
        include_bytes!("../../../../../fixtures/lz/java-analysis.sgf").to_vec(),
        None,
    ))
    .await
    .unwrap();
    let saved = actor.review.results.clone();
    let exported = actor.export_document_with_analysis().unwrap().0;
    within(actor.change_restriction(RestrictionKind::Allow, points(), false))
        .await
        .unwrap();
    within(actor.change_restriction(
        RestrictionKind::Allow,
        vec![points()[0], PointView { x: 4, y: 4 }],
        true,
    ))
    .await
    .unwrap();
    assert_eq!(actor.restriction.as_ref().unwrap().points.len(), 2);
    within(actor.restriction_depth(RestrictionKind::Allow, RestrictionDepth::WholeSearch))
        .await
        .unwrap();
    assert_eq!(actor.restriction.as_ref().unwrap().depth, 1_000_000);
    within(actor.change_restriction(RestrictionKind::Avoid, points(), true))
        .await
        .unwrap();
    assert_eq!(actor.restriction.as_ref().unwrap().points, points());
    assert_eq!(actor.restriction.as_ref().unwrap().depth, 5);
    within(actor.install_restriction(None)).await.unwrap();
    assert_eq!(actor.review.results, saved);
    assert_eq!(actor.export_document_with_analysis().unwrap().0, exported);
    within(actor.change_restriction(RestrictionKind::Allow, points(), false))
        .await
        .unwrap();
    within(actor.open(b"(;SZ[9])".to_vec(), None))
        .await
        .unwrap();
    assert!(actor.restriction.is_none());
    assert!(actor.restricted_analysis.is_none());
    assert_eq!(
        actor.settings.restriction_depths.allow,
        RestrictionDepth::WholeSearch
    );
    assert_eq!(
        actor.settings.restriction_depths.avoid,
        RestrictionDepth::Custom(5)
    );
    let loaded = within(crate::Settings::load(directory.path()))
        .await
        .unwrap();
    assert_eq!(
        loaded.restriction_depths.allow,
        RestrictionDepth::WholeSearch
    );
}

#[tokio::test]
async fn saved_analysis_cannot_reenter_parameterized_results_through_clear_undo_without_engine() {
    let directory = tempfile::tempdir().unwrap();
    let (mut actor, _) = crate::actor::tests::actor();
    actor.directory = directory.path().to_owned();
    within(actor.open(
        include_bytes!("../../../../../fixtures/lz/java-analysis.sgf").to_vec(),
        None,
    ))
    .await
    .unwrap();
    assert!(!actor.review.results.is_empty());
    assert!(actor.summary_fingerprint.is_none());
    let original = actor.review.results.clone();
    within(actor.clear_analysis(false)).await.unwrap();
    within(actor.change_search_params(Some(SearchParams {
        wide_root_noise: Some(0.5),
        ..Default::default()
    })))
    .await
    .unwrap();
    assert!(actor.review.results.is_empty());
    assert!(actor.summaries.is_empty());
    within(actor.apply(Input::Undo)).await.unwrap();
    assert!(!actor.is_analysis_cleared(NodeId(0)));
    assert!(actor.review.results.is_empty());
    assert!(actor.summaries.is_empty());
    within(actor.change_search_params(None)).await.unwrap();
    assert_eq!(actor.review.results, original);
}

#[tokio::test]
async fn transform_undo_redo_does_not_reimport_default_saved_results_under_search_parameters() {
    let directory = tempfile::tempdir().unwrap();
    let (mut actor, _) = crate::actor::tests::actor();
    actor.directory = directory.path().to_owned();
    let mut fixture = sgf::parse(include_bytes!(
        "../../../../../fixtures/lz/java-analysis.sgf"
    ))
    .unwrap();
    // This import fixture deliberately has malformed secondary ownership.
    // The transform regression needs a fully transformable primary analysis.
    fixture.games[0].sequence[0]
        .properties
        .retain(|p| p.identifier != "LZ2");
    within(actor.open(sgf::serialize(&fixture).into_bytes(), None))
        .await
        .unwrap();
    within(actor.apply(Input::Edit(document::Command::Transform(
        document::BoardTransform::RotateRight,
    ))))
    .await
    .unwrap();
    assert!(!actor.review.results.is_empty());
    let rotated = actor.review.results.clone();
    within(actor.change_search_params(Some(SearchParams {
        root_policy_temperature: Some(1.5),
        ..Default::default()
    })))
    .await
    .unwrap();
    assert!(actor.review.results.is_empty());
    within(actor.apply(Input::Undo)).await.unwrap();
    assert!(actor.review.results.is_empty());
    assert!(actor.summaries.is_empty());
    within(actor.apply(Input::Redo)).await.unwrap();
    assert!(actor.review.results.is_empty());
    assert!(actor.summaries.is_empty());
    within(actor.change_search_params(None)).await.unwrap();
    assert_eq!(actor.review.results, rotated);
}
