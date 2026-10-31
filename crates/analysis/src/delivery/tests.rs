#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use document::{AnalysisKey, NodeId};
use serde_json::json;
use tokio::time::timeout;

use super::{AnalysisSubscription, channel};
use crate::{AnalysisSnapshot, SnapshotSource};

fn snapshot(revision: u64, visits: u64, source: SnapshotSource) -> AnalysisSnapshot {
    AnalysisSnapshot {
        revision,
        node_id: NodeId(revision as u32),
        key: AnalysisKey([revision as u8; 32]),
        response: serde_json::from_value(json!({
            "id": format!("query-{revision}"),
            "turnNumber": 0,
            "isDuringSearch": true,
            "moveInfos": [],
            "rootInfo": {
                "visits": visits,
                "winrate": 0.7,
                "scoreLead": 3.5,
                "utility": 0.4,
                "currentPlayer": "W"
            }
        }))
        .unwrap(),
        source,
        visits_per_second: None,
    }
}

async fn receive(subscription: &mut AnalysisSubscription) -> Option<AnalysisSnapshot> {
    timeout(Duration::from_secs(5), subscription.recv())
        .await
        .expect("delivery did not produce its buffered result or close")
}

#[tokio::test]
async fn clearing_focus_removes_unread_cache_and_live_results() {
    let (sender, mut subscription) = channel();
    sender.send(snapshot(1, 100, SnapshotSource::Cache));
    sender.send(snapshot(1, 10, SnapshotSource::Live));
    sender.clear();
    let current = snapshot(2, 20, SnapshotSource::Live);
    sender.send(current.clone());
    drop(sender);
    assert_eq!(receive(&mut subscription).await, Some(current));
    assert_eq!(receive(&mut subscription).await, None);
}

#[tokio::test]
async fn clearing_without_replacement_leaves_no_analysis_to_consume() {
    let (sender, mut subscription) = channel();
    sender.send(snapshot(1, 100, SnapshotSource::Cache));
    sender.send(snapshot(1, 10, SnapshotSource::Live));
    sender.clear();
    drop(sender);
    assert_eq!(receive(&mut subscription).await, None);
}

#[tokio::test]
async fn slow_consumer_always_receives_cache_before_latest_live_final() {
    let (sender, mut subscription) = channel();
    let cached = snapshot(2, 100, SnapshotSource::Cache);
    sender.send(cached.clone());
    for visits in 1..1000 {
        sender.send(snapshot(2, visits, SnapshotSource::Live));
    }
    let mut final_result = snapshot(2, 1000, SnapshotSource::Live);
    final_result.response.is_during_search = false;
    sender.send(final_result.clone());
    // The closed sender still leaves both required outputs available.
    drop(sender);
    assert_eq!(receive(&mut subscription).await, Some(cached));
    assert_eq!(receive(&mut subscription).await, Some(final_result));
    assert_eq!(receive(&mut subscription).await, None);
}

#[tokio::test]
async fn live_partials_coalesce_and_final_is_delivered_once() {
    let (sender, mut subscription) = channel();
    sender.send(snapshot(3, 1, SnapshotSource::Live));
    let latest_partial = snapshot(3, 2, SnapshotSource::Live);
    sender.send(latest_partial.clone());
    assert_eq!(receive(&mut subscription).await, Some(latest_partial));
    let mut final_result = snapshot(3, 3, SnapshotSource::Live);
    final_result.response.is_during_search = false;
    sender.send(final_result.clone());
    assert_eq!(receive(&mut subscription).await, Some(final_result));
    drop(sender);
    assert_eq!(receive(&mut subscription).await, None);
}

#[tokio::test]
async fn keeping_subscription_alive_does_not_prevent_channel_closure() {
    let (sender, mut subscription) = channel();
    drop(sender);
    assert_eq!(receive(&mut subscription).await, None);
}
