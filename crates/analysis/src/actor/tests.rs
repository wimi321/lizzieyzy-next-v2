#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{collections::BTreeMap, ffi::OsString, time::Duration};

use document::GameDocument;
use engine_session::{SessionConfig, TransportSpec};
use serde_json::json;
use tokio::time::{advance, pause, resume, timeout};

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
        SessionConfig {
            startup_timeout: Duration::from_secs(86_400),
            response_timeout: Duration::from_secs(86_400),
            ..SessionConfig::default()
        },
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
            limited: None,
            limited_on_current: false,
            playout_exempt: false,
            cache: Cache::new(NonZeroUsize::new(8).unwrap()),
            throttle: Throttle::new(),
            last_published: None,
            display_floor: None,
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
        visits_per_second: None,
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

#[tokio::test]
async fn restart_keeps_display_and_cache_but_resets_floor_even_after_revision_refresh() {
    let (mut actor, mut output) = actor();
    timeout(Duration::from_secs(5), actor.start())
        .await
        .unwrap()
        .unwrap();
    let before = snapshot(&actor, 1_000, SnapshotSource::Live);
    actor.event(Some(QueryEvent::Partial(before.response.clone())));
    let pending = snapshot(&actor, 1_100, SnapshotSource::Live);
    let now = Instant::now();
    actor.throttle.rebase_revision(1, now, true);
    assert!(actor.throttle.push(pending, now).is_none());

    actor.event(Some(QueryEvent::Restarted));
    assert_eq!(*actor.status.borrow(), AnalysisStatus::Restarting);
    assert_eq!(actor.throttle.deadline(), None);
    assert_eq!(actor.last_published, Some(before.clone()));
    assert_eq!(immediately_available(&mut output).await, before);
    assert_no_output(&mut output).await;

    // A revision-only refresh during recovery must retain the old display
    // without reinstating its visit floor for the replayed query.
    let mut revised = actor.focus.clone().unwrap();
    revised.revision = 2;
    actor.rebase_focus(revised.clone());
    let mut republished = before.clone();
    republished.revision = 2;
    assert_eq!(immediately_available(&mut output).await, republished);
    let after = snapshot(&actor, 100, SnapshotSource::Live);
    actor.event(Some(QueryEvent::Partial(after.response.clone())));
    flush_pending(&mut actor);
    assert_eq!(immediately_available(&mut output).await, after);
    assert_eq!(actor.last_published, Some(after));
    assert_eq!(
        actor
            .cache
            .get(revised.key, &actor.config.engine_fingerprint),
        Some(before.response),
    );

    let mut shallow = snapshot(&actor, 99, SnapshotSource::Live);
    actor.event(Some(QueryEvent::Partial(shallow.response.clone())));
    shallow.response.is_during_search = false;
    actor.event(Some(QueryEvent::Final(shallow.response)));
    flush_pending(&mut actor);
    assert_no_output(&mut output).await;
    assert!(actor.active.is_none());
    assert_eq!(*actor.status.borrow(), AnalysisStatus::Idle);
    stop(actor).await;
}

#[tokio::test]
async fn retained_seed_respects_deeper_lru_and_does_not_replace_published_same_focus() {
    let (mut actor, mut output) = actor();
    actor.config.pondering = false;
    let cached = snapshot(&actor, 1000, SnapshotSource::Cache);
    actor.cache.insert(
        cached.key,
        &actor.config.engine_fingerprint,
        cached.response.clone(),
    );
    let seed = snapshot(&actor, 500, SnapshotSource::Cache);
    let focus = actor.focus.clone().unwrap();
    actor
        .command(Input::Focus(
            Box::new(focus.clone()),
            Some(Box::new(seed.response)),
            false,
        ))
        .await
        .unwrap();
    assert_eq!(immediately_available(&mut output).await, cached);

    let newer = snapshot(&actor, 2000, SnapshotSource::Cache);
    actor
        .command(Input::Focus(
            Box::new(focus.clone()),
            Some(Box::new(newer.response.clone())),
            false,
        ))
        .await
        .unwrap();
    assert_no_output(&mut output).await;
    assert_eq!(actor.last_published, Some(cached.clone()));
    let mut revised = focus;
    revised.revision += 1;
    actor
        .command(Input::Focus(
            Box::new(revised.clone()),
            Some(Box::new(newer.response)),
            false,
        ))
        .await
        .unwrap();
    let republished = immediately_available(&mut output).await;
    assert_eq!(republished.response, cached.response);
    assert_eq!(republished.revision, revised.revision);
    stop(actor).await;
}

fn partial(actor: &mut Actor, visits: u64) {
    let response = snapshot(actor, visits, SnapshotSource::Live).response;
    actor.event(Some(QueryEvent::Partial(response)));
}

// Process I/O uses real time; only the deterministic injected samples below
// use the paused clock. No test waits for a delay to synchronise with KataGo.
async fn command_between_samples(actor: &mut Actor, input: Input) {
    resume();
    timeout(Duration::from_secs(5), actor.command(input))
        .await
        .expect("coordinator command exceeded its safety deadline")
        .unwrap();
    pause();
}

async fn start_sampling(actor: &mut Actor) {
    timeout(Duration::from_secs(5), actor.start())
        .await
        .expect("coordinator start exceeded its safety deadline")
        .unwrap();
    pause();
}

#[tokio::test]
async fn speed_samples_partials_even_when_cached_depth_suppresses_their_display() {
    let (mut actor, mut output) = actor();
    start_sampling(&mut actor).await;
    let cached = snapshot(&actor, 1000, SnapshotSource::Cache);
    actor.offer(cached.clone());
    assert_eq!(immediately_available(&mut output).await, cached);
    partial(&mut actor, 100);
    advance(Duration::from_millis(500)).await;
    partial(&mut actor, 600);
    assert_no_output(&mut output).await;
    assert_eq!(actor.last_published, Some(cached));
    advance(Duration::from_millis(500)).await;
    partial(&mut actor, 1100);
    let live = immediately_available(&mut output).await;
    assert_eq!(live.source, SnapshotSource::Live);
    assert_eq!(live.visits_per_second, Some(1000.0));
    resume();
    stop(actor).await;
}

#[tokio::test]
async fn speed_survives_revision_refresh_and_final_without_sampling_the_final() {
    let (mut actor, mut output) = actor();
    start_sampling(&mut actor).await;
    let query_id = actor.active.as_ref().unwrap().subscription.id().to_owned();
    partial(&mut actor, 100);
    assert_eq!(
        immediately_available(&mut output).await.visits_per_second,
        None
    );
    advance(Duration::from_millis(500)).await;
    partial(&mut actor, 600);
    assert_eq!(
        immediately_available(&mut output).await.visits_per_second,
        Some(1000.0)
    );

    let mut focus = actor.focus.clone().unwrap();
    focus.revision += 1;
    actor
        .command(Input::Focus(Box::new(focus), None, false))
        .await
        .unwrap();
    assert_eq!(actor.active.as_ref().unwrap().subscription.id(), query_id);
    let refreshed = immediately_available(&mut output).await;
    assert_eq!(refreshed.revision, 2);
    assert_eq!(refreshed.visits_per_second, Some(1000.0));
    advance(Duration::from_millis(500)).await;
    partial(&mut actor, 1100);
    assert_eq!(
        immediately_available(&mut output).await.visits_per_second,
        Some(1000.0)
    );

    // A delayed Final has a different visit count but must freeze the last
    // Partial's speed rather than treating the quiet interval as slow search.
    advance(Duration::from_secs(4)).await;
    let mut final_response = snapshot(&actor, 9000, SnapshotSource::Live).response;
    final_response.is_during_search = false;
    actor.event(Some(QueryEvent::Final(final_response)));
    let finished = immediately_available(&mut output).await;
    assert!(!finished.response.is_during_search);
    assert_eq!(finished.visits_per_second, Some(1000.0));
    assert!(actor.active.is_none());
    assert_eq!(*actor.status.borrow(), AnalysisStatus::Idle);
    advance(Duration::from_secs(4)).await;
    assert_eq!(actor.last_published, Some(finished));
    assert_no_output(&mut output).await;
    resume();
    stop(actor).await;
}

#[tokio::test]
async fn speed_is_preserved_on_pause_and_reset_on_resume_settings_and_restart() {
    let (mut actor, mut output) = actor();
    start_sampling(&mut actor).await;
    partial(&mut actor, 100);
    immediately_available(&mut output).await;
    advance(Duration::from_millis(500)).await;
    partial(&mut actor, 600);
    let before_pause = immediately_available(&mut output).await;
    assert_eq!(before_pause.visits_per_second, Some(1000.0));

    command_between_samples(&mut actor, Input::SetPondering(false)).await;
    assert!(actor.active.is_none());
    advance(Duration::from_secs(4)).await;
    assert_eq!(actor.last_published, Some(before_pause));
    assert_no_output(&mut output).await;
    command_between_samples(&mut actor, Input::SetPondering(true)).await;
    // Keep visits increasing across new queries so the test proves lifecycle
    // resets, independently of the counter-regression safeguard.
    partial(&mut actor, 700);
    flush_pending(&mut actor);
    assert_eq!(
        immediately_available(&mut output).await.visits_per_second,
        None
    );
    advance(Duration::from_millis(500)).await;
    partial(&mut actor, 1200);
    assert_eq!(
        immediately_available(&mut output).await.visits_per_second,
        Some(1000.0)
    );

    let mut settings = actor.config.live.clone();
    settings.max_visits += 1;
    command_between_samples(&mut actor, Input::SetLiveSettings(settings)).await;
    partial(&mut actor, 1300);
    flush_pending(&mut actor);
    assert_eq!(
        immediately_available(&mut output).await.visits_per_second,
        None
    );
    advance(Duration::from_millis(500)).await;
    partial(&mut actor, 1800);
    assert_eq!(
        immediately_available(&mut output).await.visits_per_second,
        Some(1000.0)
    );

    actor.event(Some(QueryEvent::Restarted));
    assert_eq!(*actor.status.borrow(), AnalysisStatus::Restarting);
    partial(&mut actor, 1900);
    flush_pending(&mut actor);
    assert_eq!(
        immediately_available(&mut output).await.visits_per_second,
        None
    );
    advance(Duration::from_millis(500)).await;
    partial(&mut actor, 2400);
    assert_eq!(
        immediately_available(&mut output).await.visits_per_second,
        Some(1000.0)
    );
    resume();
    stop(actor).await;
}

#[tokio::test]
async fn speed_restarts_for_node_or_position_key_changes_and_cache_has_no_speed() {
    let (mut actor, mut output) = actor();
    start_sampling(&mut actor).await;
    partial(&mut actor, 100);
    immediately_available(&mut output).await;
    advance(Duration::from_millis(500)).await;
    partial(&mut actor, 600);
    assert_eq!(
        immediately_available(&mut output).await.visits_per_second,
        Some(1000.0)
    );

    for change_node in [true, false] {
        let mut focus = actor.focus.clone().unwrap();
        focus.revision += 1;
        if change_node {
            // A different node with an identical position can reuse its cache,
            // but the new live query must never reuse its speed samples.
            focus.node_id = NodeId(focus.node_id.0 + 1);
        } else {
            let mut document = GameDocument::new();
            document
                .apply(document::Command::Play(go_core::Move::Play(
                    go_core::Color::Black,
                    go_core::Point { x: 3, y: 3 },
                )))
                .unwrap();
            focus.position = document.position_at(document.current()).unwrap();
            focus.key = focus.position.analysis_key();
        }
        let query_id = actor.active.as_ref().unwrap().subscription.id().to_owned();
        command_between_samples(&mut actor, Input::Focus(Box::new(focus), None, false)).await;
        assert_ne!(actor.active.as_ref().unwrap().subscription.id(), query_id);
        if change_node {
            let cached = immediately_available(&mut output).await;
            assert_eq!(cached.source, SnapshotSource::Cache);
            assert_eq!(cached.visits_per_second, None);
        }
        let first_visits = if change_node { 1000 } else { 2000 };
        partial(&mut actor, first_visits);
        flush_pending(&mut actor);
        assert_eq!(
            immediately_available(&mut output).await.visits_per_second,
            None
        );
        advance(Duration::from_millis(500)).await;
        partial(&mut actor, first_visits + 500);
        assert_eq!(
            immediately_available(&mut output).await.visits_per_second,
            Some(1000.0)
        );
    }
    resume();
    stop(actor).await;
}

#[tokio::test]
async fn limit_clock_starts_with_first_live_result_and_revision_does_not_reset_it() {
    let (mut actor, _) = actor();
    actor.config.limits.max_analyze_time_seconds = 10;
    start_sampling(&mut actor).await;
    advance(Duration::from_secs(100)).await;
    assert_eq!(actor.limit_reached(), None);
    partial(&mut actor, 1);
    let started = actor.active.as_ref().unwrap().started;
    advance(Duration::from_secs(9)).await;
    assert_eq!(actor.limit_reached(), None);
    let mut focus = actor.focus.clone().unwrap();
    focus.revision += 1;
    actor
        .command(Input::Focus(Box::new(focus), None, false))
        .await
        .unwrap();
    assert_eq!(actor.active.as_ref().unwrap().started, started);
    advance(Duration::from_secs(1)).await;
    assert_eq!(actor.limit_reached(), Some(LimitReason::Time));
    resume();
    stop(actor).await;
}

#[tokio::test]
async fn playout_limit_uses_strict_root_visits_even_below_display_floor() {
    let (mut actor, mut output) = actor();
    actor.config.limits.limit_playout = true;
    actor.config.limits.limit_playouts = 100;
    start_sampling(&mut actor).await;
    let cached = snapshot(&actor, 1000, SnapshotSource::Cache);
    actor.offer(cached.clone());
    immediately_available(&mut output).await;
    let equal = snapshot(&actor, 100, SnapshotSource::Live).response;
    assert_eq!(actor.event(Some(QueryEvent::Partial(equal))), None);
    let over = snapshot(&actor, 101, SnapshotSource::Live).response;
    assert_eq!(
        actor.event(Some(QueryEvent::Partial(over))),
        Some(LimitReason::Playouts)
    );
    assert_eq!(actor.last_published, Some(cached));
    assert_no_output(&mut output).await;
    resume();
    stop(actor).await;
}

#[tokio::test]
async fn disabled_limits_do_not_stop_and_changing_thresholds_keeps_clock() {
    let (mut actor, _) = actor();
    actor.config.limits = AnalysisLimits {
        limit_time: false,
        max_analyze_time_seconds: 1,
        limit_playout: false,
        limit_playouts: 1,
    };
    start_sampling(&mut actor).await;
    partial(&mut actor, 1000);
    let started = actor.active.as_ref().unwrap().started;
    advance(Duration::from_secs(100)).await;
    assert_eq!(actor.limit_reached(), None);
    let limits = AnalysisLimits {
        limit_time: true,
        max_analyze_time_seconds: 101,
        ..actor.config.limits.clone()
    };
    actor.command(Input::SetLimits(limits)).await.unwrap();
    assert_eq!(actor.active.as_ref().unwrap().started, started);
    assert_eq!(actor.limit_reached(), None);
    advance(Duration::from_secs(1)).await;
    assert_eq!(actor.limit_reached(), Some(LimitReason::Time));
    resume();
    stop(actor).await;
}

#[tokio::test]
async fn limited_pause_preserves_latest_pending_result_and_pondering_intent() {
    let (mut actor, mut output) = actor();
    start_sampling(&mut actor).await;
    partial(&mut actor, 100);
    immediately_available(&mut output).await;
    partial(&mut actor, 101);
    assert!(actor.throttle.deadline().is_some());
    resume();
    timeout(
        Duration::from_secs(5),
        actor.stop_by_limit(LimitReason::Time),
    )
    .await
    .unwrap();
    pause();
    assert!(actor.config.pondering);
    assert!(actor.active.is_none());
    assert_eq!(
        *actor.status.borrow(),
        AnalysisStatus::Limited(LimitReason::Time)
    );
    flush_pending(&mut actor);
    assert_eq!(
        immediately_available(&mut output)
            .await
            .response
            .root_info
            .visits,
        101
    );
    let focus = actor.focus.clone().unwrap();
    actor
        .command(Input::Focus(Box::new(focus), None, false))
        .await
        .unwrap();
    actor.command(Input::SetPondering(true)).await.unwrap();
    let limits = AnalysisLimits {
        limit_time: false,
        ..actor.config.limits.clone()
    };
    actor.command(Input::SetLimits(limits)).await.unwrap();
    assert!(actor.active.is_none());
    assert_eq!(
        *actor.status.borrow(),
        AnalysisStatus::Limited(LimitReason::Time)
    );
    command_between_samples(&mut actor, Input::Resume).await;
    assert!(actor.active.is_some());
    assert_eq!(actor.active.as_ref().unwrap().started, None);
    resume();
    stop(actor).await;
}

#[tokio::test]
async fn new_focus_keeps_limit_pause_until_resume_and_resets_playout_exemption() {
    let (mut actor, _) = actor();
    actor.config.limits.limit_playout = true;
    actor.config.limits.limit_playouts = 10;
    start_sampling(&mut actor).await;
    partial(&mut actor, 11);
    resume();
    timeout(
        Duration::from_secs(5),
        actor.stop_by_limit(LimitReason::Playouts),
    )
    .await
    .unwrap();
    pause();
    command_between_samples(&mut actor, Input::Resume).await;
    assert!(actor.playout_exempt);
    partial(&mut actor, 100);
    assert_eq!(actor.limit_reached(), None);
    // A later time stop keeps the same-position visit exemption on resume.
    resume();
    timeout(
        Duration::from_secs(5),
        actor.stop_by_limit(LimitReason::Time),
    )
    .await
    .unwrap();
    pause();
    let mut focus = actor.focus.clone().unwrap();
    focus.node_id = NodeId(focus.node_id.0 + 1);
    focus.revision += 1;
    command_between_samples(&mut actor, Input::Focus(Box::new(focus), None, false)).await;
    assert!(!actor.playout_exempt);
    assert!(actor.active.is_none());
    assert_eq!(
        *actor.status.borrow(),
        AnalysisStatus::Limited(LimitReason::Time)
    );
    command_between_samples(&mut actor, Input::Resume).await;
    partial(&mut actor, 11);
    assert_eq!(actor.limit_reached(), Some(LimitReason::Playouts));
    resume();
    stop(actor).await;
}

#[tokio::test]
async fn restart_and_explicit_pause_resume_start_fresh_limit_clocks() {
    let (mut actor, _) = actor();
    actor.config.limits.max_analyze_time_seconds = 10;
    start_sampling(&mut actor).await;
    partial(&mut actor, 1);
    advance(Duration::from_secs(9)).await;
    actor.event(Some(QueryEvent::Restarted));
    assert_eq!(actor.active.as_ref().unwrap().started, None);
    advance(Duration::from_secs(20)).await;
    assert_eq!(actor.limit_reached(), None);
    partial(&mut actor, 1);
    advance(Duration::from_secs(9)).await;
    assert_eq!(actor.limit_reached(), None);
    command_between_samples(&mut actor, Input::SetPondering(false)).await;
    command_between_samples(&mut actor, Input::SetPondering(true)).await;
    assert_eq!(actor.active.as_ref().unwrap().started, None);
    partial(&mut actor, 2);
    advance(Duration::from_secs(9)).await;
    assert_eq!(actor.limit_reached(), None);
    advance(Duration::from_secs(1)).await;
    assert_eq!(actor.limit_reached(), Some(LimitReason::Time));
    resume();
    stop(actor).await;
}

#[tokio::test]
async fn navigating_after_playout_stop_does_not_exempt_the_new_position() {
    let (mut actor, _) = actor();
    actor.config.limits.limit_playout = true;
    actor.config.limits.limit_playouts = 10;
    start_sampling(&mut actor).await;
    partial(&mut actor, 11);
    resume();
    timeout(
        Duration::from_secs(5),
        actor.stop_by_limit(LimitReason::Playouts),
    )
    .await
    .unwrap();
    pause();
    let mut focus = actor.focus.clone().unwrap();
    focus.node_id = NodeId(focus.node_id.0 + 1);
    focus.revision += 1;
    command_between_samples(&mut actor, Input::Focus(Box::new(focus), None, false)).await;
    assert_eq!(
        *actor.status.borrow(),
        AnalysisStatus::Limited(LimitReason::Playouts)
    );
    command_between_samples(&mut actor, Input::Resume).await;
    assert!(!actor.playout_exempt);
    partial(&mut actor, 11);
    assert_eq!(actor.limit_reached(), Some(LimitReason::Playouts));
    resume();
    stop(actor).await;
}

#[tokio::test]
async fn atomic_focus_resume_uses_only_old_pause_and_keeps_new_position_limits() {
    let (mut actor, _) = actor();
    actor.config.limits.limit_playout = true;
    actor.config.limits.limit_playouts = 10;
    start_sampling(&mut actor).await;
    partial(&mut actor, 11);
    resume();
    timeout(
        Duration::from_secs(5),
        actor.stop_by_limit(LimitReason::Playouts),
    )
    .await
    .unwrap();
    pause();
    let mut focus = actor.focus.clone().unwrap();
    focus.node_id = NodeId(focus.node_id.0 + 1);
    focus.revision += 1;
    command_between_samples(&mut actor, Input::Focus(Box::new(focus), None, true)).await;
    assert!(!actor.playout_exempt);
    assert!(actor.active.is_some());
    assert_eq!(actor.active.as_ref().unwrap().started, None);
    let over = snapshot(&actor, 11, SnapshotSource::Live).response;
    let reason = actor.event(Some(QueryEvent::Partial(over))).unwrap();
    assert_eq!(reason, LimitReason::Playouts);
    resume();
    timeout(Duration::from_secs(5), actor.stop_by_limit(reason))
        .await
        .unwrap();
    assert_eq!(
        *actor.status.borrow(),
        AnalysisStatus::Limited(LimitReason::Playouts)
    );
    assert!(actor.active.is_none());
    stop(actor).await;
}

#[tokio::test]
async fn atomic_same_focus_resume_keeps_playout_exemption_and_restarts_clock() {
    let (mut actor, _) = actor();
    actor.config.limits.limit_playout = true;
    actor.config.limits.limit_playouts = 10;
    start_sampling(&mut actor).await;
    partial(&mut actor, 11);
    resume();
    timeout(
        Duration::from_secs(5),
        actor.stop_by_limit(LimitReason::Playouts),
    )
    .await
    .unwrap();
    pause();
    let focus = actor.focus.clone().unwrap();
    command_between_samples(&mut actor, Input::Focus(Box::new(focus), None, true)).await;
    assert!(actor.playout_exempt);
    assert_eq!(actor.active.as_ref().unwrap().started, None);
    partial(&mut actor, 100);
    assert_eq!(actor.limit_reached(), None);
    resume();
    stop(actor).await;
}

#[tokio::test]
async fn discarding_analysis_clears_snapshots_and_cache_but_preserves_limit_pause() {
    let (mut actor, mut output) = actor();
    within_discard_test(actor.start()).await.unwrap();
    partial(&mut actor, 100);
    within_discard_test(actor.stop_by_limit(LimitReason::Time)).await;
    actor.playout_exempt = true;
    let focus = actor.focus.clone().unwrap();
    assert!(actor.last_published.is_some());
    assert!(
        actor
            .cache
            .get(focus.key, &actor.config.engine_fingerprint)
            .is_some()
    );
    within_discard_test(actor.command(Input::DiscardCachedAnalysis))
        .await
        .unwrap();
    assert_eq!(actor.focus, Some(focus.clone()));
    assert_eq!(actor.limited, Some(LimitReason::Time));
    assert!(actor.limited_on_current);
    assert!(actor.playout_exempt);
    assert!(actor.config.pondering);
    assert!(actor.active.is_none());
    assert!(actor.last_published.is_none());
    assert!(actor.display_floor.is_none());
    assert_eq!(actor.throttle.deadline(), None);
    assert_eq!(
        actor.cache.get(focus.key, &actor.config.engine_fingerprint),
        None
    );
    assert_eq!(
        *actor.status.borrow(),
        AnalysisStatus::Limited(LimitReason::Time)
    );
    assert_no_output(&mut output).await;
    stop(actor).await;
}

async fn within_discard_test<T>(future: impl std::future::Future<Output = T>) -> T {
    timeout(Duration::from_secs(5), future)
        .await
        .expect("discard test safety deadline")
}

#[tokio::test]
async fn query_overrides_cannot_reuse_an_existing_coordinator_cache_namespace() {
    let (mut actor, _) = actor();
    let original = actor.config.live.clone();
    for settings in [
        LiveSettings {
            query_rules: Some(katago_protocol::Rules::Named("aga".into())),
            ..original.clone()
        },
        LiveSettings {
            query_komi: Some(6.5),
            ..original.clone()
        },
    ] {
        assert!(matches!(
            actor.command(Input::SetLiveSettings(settings)).await,
            Err(AnalysisError::InvalidSettings(_))
        ));
        assert_eq!(actor.config.live, original);
    }
    stop(actor).await;
}
