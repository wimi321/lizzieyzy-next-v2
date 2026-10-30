#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{collections::BTreeMap, ffi::OsString, time::Duration};

use document::GameDocument;
use engine_session::{SessionConfig, TransportSpec};
use serde_json::json;
use tokio::time::timeout;

use super::*;
use crate::{AnalysisSubscription, delivery};

fn actor() -> (Actor, AnalysisSubscription) {
    let engine = engine_session::spawn(
        TransportSpec::LocalProcess {
            program: fake_katago::binary_path().expect("build fake KataGo"),
            args: [
                "analysis",
                "-config",
                "unused.cfg",
                "-model",
                "unused.bin.gz",
            ]
            .into_iter()
            .map(OsString::from)
            .collect(),
            cwd: None,
            env: BTreeMap::new(),
        },
        SessionConfig::default(),
    );
    let document = GameDocument::new();
    let position = document.position_at(document.root()).unwrap();
    let focus = Focus {
        revision: 1,
        node_id: document.root(),
        key: position.analysis_key(),
        position,
    };
    let (output, subscription) = delivery::channel();
    let (status, _) = watch::channel(AnalysisStatus::Idle);
    (
        Actor {
            engine,
            config: AnalysisConfig::default(),
            focus: Some(focus),
            active: None,
            cache: Cache::new(NonZeroUsize::new(8).unwrap()),
            throttle: Throttle::new(),
            last_published: None,
            status,
            output,
        },
        subscription,
    )
}

fn snapshot(actor: &Actor, visits: u64, source: SnapshotSource) -> AnalysisSnapshot {
    let focus = actor.focus.as_ref().unwrap();
    AnalysisSnapshot {
        revision: focus.revision,
        node_id: focus.node_id,
        key: focus.key,
        response: serde_json::from_value(json!({
            "id": "current-query", "turnNumber": 0, "isDuringSearch": true,
            "moveInfos": [],
            "rootInfo": {
                "visits": visits, "winrate": 0.7, "scoreLead": 3.5,
                "utility": 0.4, "currentPlayer": "B"
            }
        }))
        .unwrap(),
        source,
    }
}

fn flush_pending(actor: &mut Actor) {
    if let Some(deadline) = actor.throttle.deadline() {
        let snapshot = actor.throttle.flush(deadline).unwrap();
        actor.publish(snapshot);
    }
}

async fn immediately_available(output: &mut AnalysisSubscription) -> AnalysisSnapshot {
    tokio::select! {
        biased;
        value = output.recv() => value.expect("snapshot channel closed"),
        _ = std::future::ready(()) => panic!("expected an immediately available snapshot"),
    }
}

async fn assert_no_output(output: &mut AnalysisSubscription) {
    tokio::select! {
        biased;
        value = output.recv() => panic!("unexpected output: {value:?}"),
        _ = std::future::ready(()) => {}
    }
}

async fn stop(actor: Actor) {
    let engine = actor.engine.clone();
    drop(actor);
    timeout(Duration::from_secs(5), engine.shutdown())
        .await
        .expect("fake engine shutdown exceeded its safety deadline")
        .unwrap();
}

#[tokio::test]
async fn published_cache_rejects_shallower_partial_and_final_but_accepts_equal_and_deeper() {
    let (mut actor, mut output) = actor();
    let cached = snapshot(&actor, 100, SnapshotSource::Cache);
    actor.offer(cached.clone());
    let mut shallow = snapshot(&actor, 99, SnapshotSource::Live);
    actor.offer(shallow.clone());
    shallow.response.is_during_search = false;
    actor.offer(shallow.clone());
    // Also exercise the final publication boundary, after throttling.
    actor.publish(shallow);
    assert_eq!(actor.last_published, Some(cached.clone()));
    assert_eq!(actor.throttle.deadline(), None);
    assert_eq!(immediately_available(&mut output).await, cached);
    assert_no_output(&mut output).await;

    let equal = snapshot(&actor, 100, SnapshotSource::Live);
    actor.offer(equal.clone());
    flush_pending(&mut actor);
    assert_eq!(immediately_available(&mut output).await, equal);
    let deeper = snapshot(&actor, 101, SnapshotSource::Live);
    actor.offer(deeper.clone());
    flush_pending(&mut actor);
    assert_eq!(immediately_available(&mut output).await, deeper);
    stop(actor).await;
}

#[tokio::test]
async fn revision_refresh_immediately_republishes_latest_and_preserves_pending_final() {
    let (mut actor, mut output) = actor();
    let mut latest = snapshot(&actor, 100, SnapshotSource::Live);
    actor.offer(latest.clone());
    let mut final_snapshot = snapshot(&actor, 101, SnapshotSource::Live);
    final_snapshot.response.is_during_search = false;
    // Use a supplied logical instant so inserting pending data never depends
    // on how much wall time the machine needs to execute the assertions.
    let now = Instant::now();
    actor.throttle.rebase_revision(1, now, true);
    assert!(actor.throttle.push(final_snapshot.clone(), now).is_none());
    let mut focus = actor.focus.clone().unwrap();
    focus.revision = 2;
    actor.rebase_focus(focus);
    latest.revision = 2;
    assert_eq!(immediately_available(&mut output).await, latest);
    assert_eq!(actor.last_published, Some(latest));
    assert!(actor.active.is_none());
    flush_pending(&mut actor);
    final_snapshot.revision = 2;
    assert_eq!(immediately_available(&mut output).await, final_snapshot);
    assert_no_output(&mut output).await;
    stop(actor).await;
}

#[tokio::test]
async fn unpublished_deeper_pending_result_does_not_raise_the_display_floor() {
    let (mut actor, mut output) = actor();
    let cached = snapshot(&actor, 100, SnapshotSource::Cache);
    actor.offer(cached.clone());
    let pending = snapshot(&actor, 200, SnapshotSource::Live);
    let now = Instant::now();
    actor.throttle.rebase_revision(1, now, true);
    assert!(actor.throttle.push(pending, now).is_none());
    assert_eq!(actor.last_published, Some(cached.clone()));
    let accepted = snapshot(&actor, 150, SnapshotSource::Live);
    actor.offer(accepted.clone());
    flush_pending(&mut actor);
    assert_eq!(immediately_available(&mut output).await, cached);
    assert_eq!(immediately_available(&mut output).await, accepted);
    assert_eq!(actor.last_published, Some(accepted));
    stop(actor).await;
}
