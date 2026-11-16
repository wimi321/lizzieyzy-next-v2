#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::actor::{
    Input,
    tests::{actor, snapshot},
};
use crate::{AnalysisExtras, AnalysisSource, LayoutMode, PointView, RestrictionKind, SearchParams};
use std::{future::Future, time::Duration};
use tokio::sync::oneshot;

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("comparison freshness operation timed out")
}

fn pure_result(actor: &mut Actor) {
    let mut result = snapshot(actor, 100, 0.6);
    result.source = SnapshotSource::PureNetwork;
    actor.comparison_snapshot(result);
    assert_eq!(
        actor.comparison_view().analysis.unwrap().source,
        AnalysisSource::Live
    );
}

async fn restrict(actor: &mut Actor, x: u8) {
    let (reply, response) = oneshot::channel();
    within(
        actor.analysis_tools_command(crate::analysis_tools::Request::Set(
            RestrictionKind::Allow,
            vec![PointView { x, y: 3 }],
            reply,
        )),
    )
    .await;
    within(response).await.unwrap().unwrap();
}

#[tokio::test]
async fn pure_mode_transitions_do_not_reuse_the_previous_mode_at_the_same_revision() {
    let (mut actor, _output) = actor();
    actor.settings.display.layout_mode = LayoutMode::DoubleEngine;
    actor.comparison_snapshot(snapshot(&actor, 80, 0.4));
    assert_eq!(
        actor.comparison_view().analysis.unwrap().source,
        AnalysisSource::Live
    );
    let revision = actor.revision;
    within(actor.apply(Input::SetPureNetwork(true)))
        .await
        .unwrap();
    assert_eq!(actor.revision, revision);
    assert!(actor.comparison_view().analysis.is_none());
    pure_result(&mut actor);
    within(actor.apply(Input::SetPureNetwork(false)))
        .await
        .unwrap();
    let restored = actor.comparison_view().analysis.unwrap();
    assert_eq!(restored.source, AnalysisSource::Cache);
    assert_eq!(restored.visits, 80);
    assert_eq!(actor.revision, revision);
}

#[tokio::test]
async fn replacing_a_restriction_invalidates_the_previous_restricted_view() {
    let (mut actor, _output) = actor();
    actor.settings.display.layout_mode = LayoutMode::DoubleEngine;
    restrict(&mut actor, 3).await;
    let mut result = snapshot(&actor, 100, 0.6);
    result.source = SnapshotSource::Restricted;
    actor.comparison_snapshot(result);
    assert_eq!(
        actor.comparison_view().analysis.unwrap().source,
        AnalysisSource::Live
    );
    let revision = actor.revision;
    restrict(&mut actor, 4).await;
    assert_eq!(actor.revision, revision);
    assert!(actor.comparison_view().analysis.is_none());
    assert!(actor.comparison.results.is_empty());
}

#[tokio::test]
async fn live_settings_and_payload_changes_invalidate_transient_comparison_results() {
    let directory = tempfile::tempdir().unwrap();
    let (mut actor, _output) = actor();
    actor.directory = directory.path().to_owned();
    actor.settings.display.layout_mode = LayoutMode::DoubleEngine;
    within(actor.apply(Input::SetPureNetwork(true)))
        .await
        .unwrap();
    let revision = actor.revision;
    pure_result(&mut actor);
    within(actor.apply(Input::SetAnalysisExtras(AnalysisExtras {
        policy: true,
        moves_ownership: true,
    })))
    .await
    .unwrap();
    assert!(actor.comparison_view().analysis.is_none());
    pure_result(&mut actor);
    let mut settings = actor.settings.engine_settings.clone();
    settings.report_interval_ms += 1;
    within(actor.set_engine_settings(settings)).await.unwrap();
    assert!(actor.comparison_view().analysis.is_none());
    assert_eq!(actor.revision, revision);
}

#[tokio::test]
async fn search_parameters_invalidate_saved_comparison_without_an_engine_identity() {
    let directory = tempfile::tempdir().unwrap();
    let (mut actor, _output) = actor();
    actor.directory = directory.path().to_owned();
    actor.settings.display.layout_mode = LayoutMode::DoubleEngine;
    within(actor.open(b"(;SZ[5]LZ2[Second 45 100])".to_vec(), None))
        .await
        .unwrap();
    assert_eq!(
        actor.comparison_view().analysis.unwrap().source,
        AnalysisSource::Saved
    );
    assert!(actor.comparison.fingerprint.is_none());
    assert!(actor.comparison.running.is_none());
    let params = Some(SearchParams {
        wide_root_noise: Some(0.5),
        ..Default::default()
    });
    let (reply, response) = oneshot::channel();
    within(
        actor.analysis_tools_command(crate::analysis_tools::Request::Search(
            params.clone(),
            reply,
        )),
    )
    .await;
    assert_eq!(within(response).await.unwrap().unwrap(), params);
    assert!(actor.comparison_view().analysis.is_none());
    assert!(actor.comparison.results.is_empty());
    let (reply, response) = oneshot::channel();
    within(actor.analysis_tools_command(crate::analysis_tools::Request::Search(None, reply))).await;
    assert!(within(response).await.unwrap().unwrap().is_none());
    let restored = actor.comparison_view().analysis.unwrap();
    assert_eq!(restored.source, AnalysisSource::Saved);
    assert_eq!(restored.visits, 100);
}

#[tokio::test]
async fn primary_game_query_clears_the_transient_comparison_focus() {
    let (mut actor, _output) = actor();
    actor.settings.display.layout_mode = LayoutMode::DoubleEngine;
    within(actor.apply(Input::SetPureNetwork(true)))
        .await
        .unwrap();
    pure_result(&mut actor);
    within(actor.clear_game_live_query()).await.unwrap();
    assert!(actor.focus.is_none());
    assert!(actor.comparison_view().analysis.is_none());
}
