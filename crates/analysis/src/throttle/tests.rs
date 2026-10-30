#![allow(clippy::unwrap_used)]

use super::*;
use crate::SnapshotSource;
use document::{AnalysisKey, NodeId};
use katago_protocol::{EngineLine, parse_line};
use tokio::time::{advance, pause};

fn snapshot(visits: u64) -> AnalysisSnapshot {
    let line = format!(
        r#"{{"id":"query","turnNumber":0,"isDuringSearch":true,"moveInfos":[],
        "rootInfo":{{"visits":{visits},"winrate":0.7,"scoreLead":3.5,
        "utility":0.4,"currentPlayer":"B"}}}}"#
    );
    let EngineLine::Analysis(response) = parse_line(&line).unwrap() else {
        panic!("expected analysis response");
    };
    AnalysisSnapshot {
        revision: 1,
        node_id: NodeId(1),
        key: AnalysisKey([1; 32]),
        response: *response,
        source: SnapshotSource::Live,
    }
}

#[tokio::test]
async fn exact_boundary_coalesces_to_latest_and_delivers_trailing_final() {
    pause();
    let mut throttle = Throttle::new();
    let start = Instant::now();
    assert_eq!(throttle.push(snapshot(1), start), Some(snapshot(1)));
    assert_eq!(throttle.deadline(), None);
    advance(Duration::from_millis(10)).await;
    assert_eq!(throttle.push(snapshot(2), Instant::now()), None);
    advance(Duration::from_millis(39)).await;
    assert_eq!(throttle.push(snapshot(3), Instant::now()), None);
    assert_eq!(throttle.deadline(), Some(start + INTERVAL));
    assert_eq!(throttle.flush(Instant::now()), None);
    advance(Duration::from_millis(1)).await;
    assert_eq!(throttle.flush(Instant::now()), Some(snapshot(3)));
    assert_eq!(throttle.deadline(), None);

    let mut final_snapshot = snapshot(4);
    final_snapshot.response.is_during_search = false;
    assert_eq!(throttle.push(final_snapshot.clone(), Instant::now()), None);
    advance(INTERVAL).await;
    assert_eq!(throttle.flush(Instant::now()), Some(final_snapshot));
    assert_eq!(throttle.flush(Instant::now()), None);
}

#[tokio::test]
async fn hundred_hertz_input_emits_twenty_updates_in_half_open_second() {
    pause();
    let mut throttle = Throttle::new();
    let mut outputs = Vec::new();
    for visits in 0..100 {
        if let Some(value) = throttle.push(snapshot(visits), Instant::now()) {
            outputs.push((Instant::now(), value));
        }
        advance(Duration::from_millis(10)).await;
    }
    assert_eq!(outputs.len(), 20);
    assert!(
        outputs
            .windows(2)
            .all(|pair| pair[1].0 - pair[0].0 == INTERVAL)
    );
    assert_eq!(throttle.flush(Instant::now()), Some(snapshot(99)));
}

#[tokio::test]
async fn late_flush_does_not_catch_up_with_burst() {
    pause();
    let mut throttle = Throttle::new();
    assert!(throttle.push(snapshot(1), Instant::now()).is_some());
    assert!(throttle.push(snapshot(2), Instant::now()).is_none());
    advance(Duration::from_secs(2)).await;
    assert_eq!(throttle.flush(Instant::now()), Some(snapshot(2)));
    assert_eq!(throttle.push(snapshot(3), Instant::now()), None);
    advance(INTERVAL).await;
    assert_eq!(throttle.flush(Instant::now()), Some(snapshot(3)));
}

#[tokio::test]
async fn reset_drops_old_focus_and_cache_counts_toward_new_focus_rate() {
    pause();
    let mut throttle = Throttle::new();
    assert!(throttle.push(snapshot(1), Instant::now()).is_some());
    assert!(throttle.push(snapshot(2), Instant::now()).is_none());
    throttle.reset();
    assert_eq!(throttle.deadline(), None);
    assert_eq!(throttle.flush(Instant::now()), None);
    let mut cached = snapshot(100);
    cached.revision = 2;
    cached.node_id = NodeId(2);
    cached.source = SnapshotSource::Cache;
    assert_eq!(throttle.push(cached.clone(), Instant::now()), Some(cached));
    let mut live = snapshot(10);
    live.revision = 2;
    live.node_id = NodeId(2);
    assert_eq!(throttle.push(live.clone(), Instant::now()), None);
    advance(INTERVAL).await;
    assert_eq!(throttle.flush(Instant::now()), Some(live));
}

#[tokio::test]
async fn cancellation_clears_pending_without_resetting_focus_rate() {
    pause();
    let mut throttle = Throttle::new();
    assert!(throttle.push(snapshot(1), Instant::now()).is_some());
    assert!(throttle.push(snapshot(2), Instant::now()).is_none());
    throttle.clear_pending();
    assert_eq!(throttle.deadline(), None);
    assert!(throttle.push(snapshot(3), Instant::now()).is_none());
    advance(INTERVAL).await;
    assert_eq!(throttle.flush(Instant::now()), Some(snapshot(3)));
}

#[tokio::test]
async fn revision_refresh_preserves_pending_final_and_waits_a_full_interval() {
    pause();
    let mut throttle = Throttle::new();
    assert!(throttle.push(snapshot(1), Instant::now()).is_some());
    advance(Duration::from_millis(10)).await;
    let mut final_snapshot = snapshot(2);
    final_snapshot.response.is_during_search = false;
    assert!(
        throttle
            .push(final_snapshot.clone(), Instant::now())
            .is_none()
    );
    advance(Duration::from_millis(10)).await;
    let refreshed_at = Instant::now();
    throttle.rebase_revision(2, refreshed_at, true);
    assert_eq!(throttle.deadline(), Some(refreshed_at + INTERVAL));
    advance(Duration::from_millis(49)).await;
    assert_eq!(throttle.flush(Instant::now()), None);
    advance(Duration::from_millis(1)).await;
    final_snapshot.revision = 2;
    assert_eq!(throttle.flush(Instant::now()), Some(final_snapshot));
    assert_eq!(throttle.flush(Instant::now()), None);
}

#[tokio::test]
async fn revision_change_without_republication_keeps_the_original_deadline() {
    pause();
    let mut throttle = Throttle::new();
    let start = Instant::now();
    assert!(throttle.push(snapshot(1), start).is_some());
    assert!(throttle.push(snapshot(2), start).is_none());
    advance(Duration::from_millis(20)).await;
    throttle.rebase_revision(2, Instant::now(), false);
    assert_eq!(throttle.deadline(), Some(start + INTERVAL));
    advance(Duration::from_millis(29)).await;
    assert_eq!(throttle.flush(Instant::now()), None);
    advance(Duration::from_millis(1)).await;
    let mut expected = snapshot(2);
    expected.revision = 2;
    assert_eq!(throttle.flush(Instant::now()), Some(expected));
}

#[tokio::test]
async fn repeated_revision_changes_retag_new_pending_data_and_keep_it_throttled() {
    pause();
    let mut throttle = Throttle::new();
    throttle.rebase_revision(2, Instant::now(), false);
    let mut current = snapshot(1);
    current.revision = 2;
    assert_eq!(
        throttle.push(current.clone(), Instant::now()),
        Some(current)
    );
    throttle.rebase_revision(3, Instant::now(), true);
    let mut next = snapshot(2);
    next.revision = 3;
    assert!(throttle.push(next.clone(), Instant::now()).is_none());
    advance(Duration::from_millis(10)).await;
    throttle.rebase_revision(4, Instant::now(), true);
    advance(Duration::from_millis(49)).await;
    assert_eq!(throttle.flush(Instant::now()), None);
    advance(Duration::from_millis(1)).await;
    next.revision = 4;
    assert_eq!(throttle.flush(Instant::now()), Some(next));
}
