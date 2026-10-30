#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{collections::BTreeMap, ffi::OsString, future::Future, time::Duration};

use analysis::{
    AnalysisConfig, AnalysisError, AnalysisHandle, AnalysisSnapshot, AnalysisStatus,
    AnalysisSubscription, Focus, LiveSettings, SnapshotSource,
};
use document::{Command, GameDocument};
use engine_session::{
    AnalysisRequest, EngineHandle, QueryEvent, SessionConfig, SessionError, TransportSpec,
};
use go_core::Move;
use katago_protocol::Player;
use proptest::prelude::*;
use serde_json::json;
use tokio::time::{Instant, MissedTickBehavior, interval_at, timeout};

const SAFETY: Duration = Duration::from_secs(5);

fn engine(arguments: &[&str]) -> EngineHandle {
    engine_with_config(arguments, SessionConfig::default())
}

fn engine_with_config(arguments: &[&str], config: SessionConfig) -> EngineHandle {
    let mut args: Vec<OsString> = [
        "analysis",
        "-config",
        "unused.cfg",
        "-model",
        "unused.bin.gz",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    args.extend(arguments.iter().map(OsString::from));
    engine_session::spawn(
        TransportSpec::LocalProcess {
            program: fake_katago::binary_path().expect("build fake KataGo"),
            args,
            cwd: None,
            env: BTreeMap::new(),
        },
        // These tests exercise events and deliberate process crashes, not
        // watchdog deadlines. Keep all timeouts safe on oversubscribed CI.
        config,
    )
}

fn live(max_visits: u64) -> LiveSettings {
    LiveSettings {
        max_visits,
        report_interval: Duration::from_millis(10),
        ..LiveSettings::default()
    }
}

fn coordinator(
    engine: &EngineHandle,
    settings: LiveSettings,
) -> (AnalysisHandle, AnalysisSubscription) {
    analysis::spawn(
        engine.clone(),
        AnalysisConfig {
            engine_fingerprint: "fake-model:sha256:test-config".into(),
            live: settings,
            ..AnalysisConfig::default()
        },
    )
    .unwrap()
}

fn focus(revision: u64, plies: u8) -> Focus {
    let mut doc = GameDocument::new();
    for _ in 0..plies {
        let position = doc.position_at(doc.current()).unwrap();
        doc.apply(Command::Play(Move::Pass(position.board.to_play())))
            .unwrap();
    }
    let position = doc.position_at(doc.current()).unwrap();
    Focus {
        revision,
        node_id: doc.current(),
        key: position.analysis_key(),
        position,
    }
}

async fn within<T>(future: impl Future<Output = T>) -> T {
    timeout(SAFETY, future)
        .await
        .expect("event-driven operation exceeded its safety deadline")
}

async fn snapshot(output: &mut AnalysisSubscription) -> AnalysisSnapshot {
    within(output.recv()).await.expect("analysis output closed")
}

fn belongs_to(snapshot: &AnalysisSnapshot, expected: &Focus) {
    assert_eq!(snapshot.revision, expected.revision);
    assert_eq!(snapshot.node_id, expected.node_id);
    assert_eq!(snapshot.key, expected.key);
}

async fn final_snapshot(output: &mut AnalysisSubscription, expected: &Focus) -> AnalysisSnapshot {
    final_snapshot_at_least(output, expected, 0).await
}

async fn final_snapshot_at_least(
    output: &mut AnalysisSubscription,
    expected: &Focus,
    mut floor: u64,
) -> AnalysisSnapshot {
    within(async {
        loop {
            let value = output.recv().await.expect("analysis output closed");
            belongs_to(&value, expected);
            assert!(value.response.root_info.visits >= floor);
            floor = value.response.root_info.visits;
            if value.source == SnapshotSource::Live && !value.response.is_during_search {
                return value;
            }
        }
    })
    .await
}

async fn progress_at_least(
    output: &mut AnalysisSubscription,
    expected: &Focus,
    target: u64,
) -> AnalysisSnapshot {
    within(async {
        let mut floor = 0;
        loop {
            let value = output.recv().await.expect("analysis output closed");
            belongs_to(&value, expected);
            assert!(value.response.root_info.visits >= floor);
            floor = value.response.root_info.visits;
            if floor >= target {
                return value;
            }
        }
    })
    .await
}

async fn immediate_snapshot(output: &mut AnalysisSubscription) -> AnalysisSnapshot {
    tokio::select! {
        biased;
        value = output.recv() => value.expect("analysis output closed"),
        _ = std::future::ready(()) => panic!("snapshot was not immediately available"),
    }
}

async fn assert_no_snapshot(output: &mut AnalysisSubscription) {
    tokio::select! {
        biased;
        value = output.recv() => panic!("unexpected analysis output: {value:?}"),
        _ = std::future::ready(()) => {}
    }
}

fn sequence(id: &str) -> u64 {
    id.rsplit_once('-').unwrap().1.parse().unwrap()
}

fn barrier_request() -> AnalysisRequest {
    serde_json::from_value(json!({
        "moves": [], "rules": "chinese", "boardXSize": 19, "boardYSize": 19,
        "komi": 7.5, "maxVisits": 1
    }))
    .unwrap()
}

async fn barrier(engine: &EngineHandle) {
    let mut query = within(engine.submit(barrier_request())).await.unwrap();
    within(async {
        loop {
            match query.recv().await {
                Some(QueryEvent::Final(_)) => return,
                Some(QueryEvent::Partial(_)) => {}
                other => panic!("barrier failed: {other:?}"),
            }
        }
    })
    .await;
}

async fn stop(handle: AnalysisHandle, engine: EngineHandle) {
    within(handle.shutdown()).await.unwrap();
    within(engine.shutdown()).await.unwrap();
}

async fn idle(handle: &AnalysisHandle) {
    let mut status = handle.status();
    within(async {
        loop {
            let value = status.borrow_and_update().clone();
            if value == AnalysisStatus::Idle {
                return;
            }
            assert!(!matches!(
                value,
                AnalysisStatus::Failed(_) | AnalysisStatus::Stopped
            ));
            status.changed().await.unwrap();
        }
    })
    .await;
}

#[tokio::test]
async fn fifty_rapid_focus_changes_discard_all_unread_old_results() {
    let engine = engine(&["--visits-per-sec", "10000"]);
    let (handle, mut output) = coordinator(&engine, live(100));
    // Seed an unread old snapshot before the navigation burst. A plain FIFO
    // output channel would deliver that stale snapshot after all fifty changes.
    let first = focus(0, 0);
    within(handle.focus(first)).await.unwrap();
    barrier(&engine).await;
    idle(&handle).await;
    let mut last = focus(0, 0);
    for revision in 1..=50 {
        last = focus(revision, (revision % 4) as u8);
        within(handle.focus(last.clone())).await.unwrap();
    }
    let result = final_snapshot(&mut output, &last).await;
    assert_eq!(result.response.root_info.visits, 100);
    stop(handle, engine).await;
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 16,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn every_output_matches_current_focus_under_random_switch_intervals(
        switches in prop::collection::vec((0u64..7, 0u8..4), 50),
        visits_per_sec in 1_000u32..10_000,
        time_scale in 1u32..5,
    ) {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        runtime.block_on(async {
            let engine = engine(&[
                "--visits-per-sec", &visits_per_sec.to_string(),
                "--time-scale", &time_scale.to_string(),
            ]);
            let mut settings = live(100);
            settings.report_interval = Duration::from_millis(1);
            let (handle, mut output) = coordinator(&engine, settings);
            let mut current = focus(0, 0);
            for (index, (gap_ms, plies)) in switches.into_iter().enumerate() {
                current = focus(index as u64 + 1, plies);
                within(handle.focus(current.clone())).await.unwrap();
                if gap_ms == 0 {
                    continue;
                }
                // This timer is randomized user input, not synchronization
                // with the child process. Check EVERY output while it runs.
                let gap = Duration::from_millis(gap_ms);
                let mut input = interval_at(Instant::now() + gap, gap);
                input.set_missed_tick_behavior(MissedTickBehavior::Skip);
                within(async {
                    loop {
                        tokio::select! {
                            biased;
                            _ = input.tick() => break,
                            value = output.recv() => {
                                belongs_to(&value.expect("analysis output closed"), &current);
                            }
                        }
                    }
                }).await;
            }
            // End with a different position, since changing only revision
            // intentionally keeps its existing query (which may be complete).
            current = focus(current.revision + 1, ((current.node_id.0 + 1) % 4) as u8);
            within(handle.focus(current.clone())).await.unwrap();
            final_snapshot(&mut output, &current).await;
            stop(handle, engine).await;
        });
    }
}

#[tokio::test]
async fn returning_focus_publishes_cache_before_live_and_retains_deeper_results() {
    let engine = engine(&["--visits-per-sec", "1000"]);
    let (handle, mut output) = coordinator(&engine, live(160));
    let a = focus(1, 0);
    within(handle.focus(a.clone())).await.unwrap();
    let original = final_snapshot(&mut output, &a).await;
    assert_eq!(original.response.root_info.visits, 160);

    within(handle.set_pondering(false)).await.unwrap();
    within(handle.focus(focus(2, 1))).await.unwrap();
    within(handle.set_live_settings(live(400))).await.unwrap();
    let returning = focus(3, 0);
    within(handle.focus(returning.clone())).await.unwrap();
    // focus() acknowledges only after cache publication; no child process or
    // elapsed-time assumption is needed to assert immediate availability.
    let cached = immediate_snapshot(&mut output).await;
    belongs_to(&cached, &returning);
    assert_eq!(cached.source, SnapshotSource::Cache);
    assert_eq!(cached.response, original.response);
    within(handle.set_pondering(true)).await.unwrap();
    let deeper = final_snapshot_at_least(&mut output, &returning, 160).await;
    assert_eq!(deeper.response.root_info.visits, 400);

    within(handle.set_pondering(false)).await.unwrap();
    within(handle.set_live_settings(live(10))).await.unwrap();
    within(handle.focus(focus(4, 1))).await.unwrap();
    let returning = focus(5, 0);
    within(handle.focus(returning.clone())).await.unwrap();
    within(handle.set_pondering(true)).await.unwrap();
    // The completed shallow query must leave the deeper cache as the only
    // output, including for a consumer that has not read that cache yet.
    barrier(&engine).await;
    idle(&handle).await;
    let cached = snapshot(&mut output).await;
    belongs_to(&cached, &returning);
    assert_eq!(cached.source, SnapshotSource::Cache);
    assert_eq!(cached.response, deeper.response);
    assert_no_snapshot(&mut output).await;
    let revised = focus(6, 0);
    within(handle.focus(revised.clone())).await.unwrap();
    let republished = immediate_snapshot(&mut output).await;
    belongs_to(&republished, &revised);
    assert_eq!(republished.source, SnapshotSource::Cache);
    assert_eq!(republished.response, deeper.response);

    // Changing settings on this same focus explicitly clears its floor, so a
    // lower-visit result under the new settings is now meaningful and allowed.
    within(handle.set_live_settings(live(20))).await.unwrap();
    let changed = final_snapshot(&mut output, &revised).await;
    assert_eq!(changed.response.root_info.visits, 20);

    within(handle.set_pondering(false)).await.unwrap();
    within(handle.focus(focus(7, 1))).await.unwrap();
    let returning = focus(8, 0);
    within(handle.focus(returning.clone())).await.unwrap();
    let retained = snapshot(&mut output).await;
    belongs_to(&retained, &returning);
    assert_eq!(retained.source, SnapshotSource::Cache);
    assert_eq!(retained.response, deeper.response);
    stop(handle, engine).await;
}

#[tokio::test]
async fn hundred_hertz_engine_output_is_capped_at_twenty_per_second() {
    let engine = engine(&["--visits-per-sec", "1000", "--time-scale", "1"]);
    let (handle, mut output) = coordinator(&engine, live(1200));
    let current = focus(1, 0);
    within(handle.focus(current.clone())).await.unwrap();
    let observed = within(async {
        let mut observed = Vec::new();
        loop {
            let value = output.recv().await.unwrap();
            belongs_to(&value, &current);
            assert_eq!(value.source, SnapshotSource::Live);
            observed.push(Instant::now());
            if !value.response.is_during_search {
                assert_eq!(value.response.root_info.visits, 1200);
                return observed;
            }
        }
    })
    .await;
    // Process scheduling can only lower throughput. Allow one extra boundary
    // sample because receipt can lag publication; exact timing lives in the
    // paused-clock throttle unit tests.
    for start in &observed {
        let count = observed
            .iter()
            .filter(|time| **time >= *start && **time < *start + Duration::from_secs(1))
            .count();
        assert!(count <= 21, "{count} outputs in one second");
    }
    stop(handle, engine).await;
}

#[tokio::test]
async fn pause_terminates_search_and_resume_allocates_a_new_request() {
    let engine = engine(&["--threads", "1"]);
    let (handle, mut output) = coordinator(&engine, live(1_000_000));
    let current = focus(1, 0);
    within(handle.focus(current.clone())).await.unwrap();
    let initial = snapshot(&mut output).await;
    belongs_to(&initial, &current);
    assert!(initial.response.is_during_search);
    within(handle.set_pondering(false)).await.unwrap();
    // A one-slot fake cannot finish this barrier if pause leaves the million
    // visit search alive. This proves cancellation without an elapsed wait.
    barrier(&engine).await;
    assert!(matches!(*handle.status().borrow(), AnalysisStatus::Idle));
    tokio::select! {
        biased;
        value = output.recv() => panic!("paused output was retained: {value:?}"),
        _ = std::future::ready(()) => {}
    }
    within(handle.set_live_settings(live(100))).await.unwrap();
    within(handle.set_pondering(true)).await.unwrap();
    let resumed = final_snapshot(&mut output, &current).await;
    assert_ne!(initial.response.id, resumed.response.id);
    assert_eq!(resumed.response.root_info.visits, 100);
    stop(handle, engine).await;
}

#[tokio::test]
async fn live_settings_clear_the_display_floor_and_replace_the_request() {
    let engine = engine(&["--threads", "1"]);
    let (handle, mut output) = coordinator(&engine, live(1_000_000));
    let current = focus(1, 1);
    within(handle.focus(current.clone())).await.unwrap();
    let before = progress_at_least(&mut output, &current, 300).await;
    let mut settings = live(30);
    settings.include_ownership = false;
    settings.include_policy = true;
    within(handle.set_live_settings(settings)).await.unwrap();
    let after = final_snapshot(&mut output, &current).await;
    assert_ne!(before.response.id, after.response.id);
    assert_eq!(after.response.root_info.visits, 30);
    assert!(after.response.ownership.is_none());
    assert!(after.response.policy.is_some());
    stop(handle, engine).await;
}

#[tokio::test]
async fn process_restart_replays_the_current_focus_without_resubmitting_it() {
    let engine = engine(&["--crash-after-queries", "3"]);
    barrier(&engine).await;
    let (handle, mut output) = coordinator(&engine, live(1_000_000));
    let current = focus(1, 1);
    within(handle.focus(current.clone())).await.unwrap();
    let before = progress_at_least(&mut output, &current, 1_000).await;
    assert!(before.response.is_during_search);
    // The third valid query crashes the process AFTER a live result. The
    // completed warm-up is not replayed, so recovery has only two requests and
    // cannot trigger another crash, regardless of cancellation timing.
    let trigger = within(engine.submit(barrier_request())).await.unwrap();
    let after = within(async {
        let mut floor = before.response.root_info.visits;
        loop {
            let value = output.recv().await.unwrap();
            belongs_to(&value, &current);
            if value.response.id != before.response.id {
                return value;
            }
            assert!(value.response.root_info.visits >= floor);
            floor = value.response.root_info.visits;
        }
    })
    .await;
    assert_eq!(after.source, SnapshotSource::Live);
    assert!(after.response.is_during_search);
    assert_ne!(after.response.id, before.response.id);
    assert!(
        after.response.root_info.visits < before.response.root_info.visits,
        "first restarted result must be shallower: before={}, after={}",
        before.response.root_info.visits,
        after.response.root_info.visits,
    );
    // The restart allocates a version query followed by the two replays;
    // an extra coordinator submit/terminate would consume another sequence.
    assert_eq!(sequence(&after.response.id), sequence(trigger.id()) + 2);
    within(handle.set_pondering(false)).await.unwrap();
    within(engine.terminate(trigger.id().to_owned()))
        .await
        .unwrap();
    within(handle.focus(focus(2, 0))).await.unwrap();
    let returning = focus(3, 1);
    within(handle.focus(returning.clone())).await.unwrap();
    let cached = immediate_snapshot(&mut output).await;
    belongs_to(&cached, &returning);
    assert_eq!(cached.source, SnapshotSource::Cache);
    assert_eq!(cached.response.id, before.response.id);
    assert!(cached.response.root_info.visits >= before.response.root_info.visits);
    assert_no_snapshot(&mut output).await;
    stop(handle, engine).await;
}

#[tokio::test]
async fn shutdown_cancels_only_its_own_query_and_closes_output() {
    let engine = engine(&["--threads", "1"]);
    let (handle, mut output) = coordinator(&engine, live(1_000_000));
    let current = focus(1, 0);
    within(handle.focus(current.clone())).await.unwrap();
    belongs_to(&snapshot(&mut output).await, &current);
    within(handle.shutdown()).await.unwrap();
    assert!(within(output.recv()).await.is_none());
    // A shared session must remain usable after its coordinator shuts down.
    barrier(&engine).await;
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn dropping_all_handles_cancels_search_even_with_output_receiver_alive() {
    let engine = engine(&["--threads", "1"]);
    let (handle, mut output) = coordinator(&engine, live(1_000_000));
    let current = focus(1, 0);
    within(handle.focus(current.clone())).await.unwrap();
    belongs_to(&snapshot(&mut output).await, &current);
    let clone = handle.clone();
    drop(handle);
    drop(clone);
    assert!(within(output.recv()).await.is_none());
    barrier(&engine).await;
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn revision_only_change_preserves_query_without_terminate_or_submit() {
    let engine = engine(&["--threads", "1"]);
    let (handle, mut output) = coordinator(&engine, live(1_000_000));
    let old = focus(1, 1);
    within(handle.focus(old.clone())).await.unwrap();
    let before = progress_at_least(&mut output, &old, 100).await;
    let current = focus(2, 1);
    assert_eq!(old.node_id, current.node_id);
    assert_eq!(old.key, current.key);
    within(handle.focus(current.clone())).await.unwrap();
    let republished = immediate_snapshot(&mut output).await;
    belongs_to(&republished, &current);
    assert_eq!(republished.source, SnapshotSource::Live);
    assert_eq!(republished.response.id, before.response.id);
    assert!(republished.response.root_info.visits >= before.response.root_info.visits);

    // Session commands and queries share one sequence. A terminate action or
    // another submit during the revision change would consume another number.
    let probe = within(engine.submit(barrier_request())).await.unwrap();
    assert_eq!(sequence(probe.id()), sequence(&before.response.id) + 1);
    let following = progress_at_least(
        &mut output,
        &current,
        republished.response.root_info.visits + 1,
    )
    .await;
    assert_eq!(following.response.id, before.response.id);
    within(engine.terminate(probe.id().to_owned()))
        .await
        .unwrap();
    stop(handle, engine).await;
}

#[tokio::test]
async fn revision_only_change_immediately_republishes_the_last_completed_snapshot() {
    let engine = engine(&["--visits-per-sec", "10000"]);
    let (handle, mut output) = coordinator(&engine, live(40));
    let old = focus(1, 1);
    within(handle.focus(old.clone())).await.unwrap();
    let last = final_snapshot(&mut output, &old).await;
    let current = focus(2, 1);
    within(handle.focus(current.clone())).await.unwrap();
    let republished = immediate_snapshot(&mut output).await;
    belongs_to(&republished, &current);
    assert_eq!(republished.response, last.response);
    assert_eq!(republished.source, last.source);
    assert_eq!(*handle.status().borrow(), AnalysisStatus::Idle);
    let probe = within(engine.submit(barrier_request())).await.unwrap();
    assert_eq!(sequence(probe.id()), sequence(&last.response.id) + 1);
    within(engine.terminate(probe.id().to_owned()))
        .await
        .unwrap();
    stop(handle, engine).await;
}

#[tokio::test]
async fn pause_and_resume_preserve_the_display_visit_floor() {
    let engine = engine(&["--threads", "1"]);
    let (handle, mut output) = coordinator(&engine, live(1_000_000));
    let current = focus(1, 0);
    within(handle.focus(current.clone())).await.unwrap();
    let before = progress_at_least(&mut output, &current, 300).await;
    within(handle.set_pondering(false)).await.unwrap();
    barrier(&engine).await;
    within(handle.set_pondering(true)).await.unwrap();
    let after = snapshot(&mut output).await;
    belongs_to(&after, &current);
    assert_ne!(after.response.id, before.response.id);
    assert!(after.response.root_info.visits >= before.response.root_info.visits);
    stop(handle, engine).await;
}

#[tokio::test]
async fn white_to_play_preserves_all_black_perspective_engine_values() {
    let engine = engine(&["--visits-per-sec", "10000"]);
    let mut settings = live(30);
    settings.include_policy = true;
    let (handle, mut output) = coordinator(&engine, settings);
    let current = focus(1, 1);
    within(handle.focus(current.clone())).await.unwrap();
    let actual = final_snapshot(&mut output, &current).await.response;
    assert_eq!(actual.root_info.current_player, Player::White);
    let request: AnalysisRequest = serde_json::from_value(json!({
        "moves": [["B", "pass"]], "initialStones": [], "initialPlayer": "B",
        "rules": "chinese", "komi": 7.5, "boardXSize": 19, "boardYSize": 19,
        "maxVisits": 30, "includeOwnership": true, "includePolicy": true,
    }))
    .unwrap();
    let mut direct = within(engine.submit(request)).await.unwrap();
    let mut expected = within(async {
        loop {
            match direct.recv().await {
                Some(QueryEvent::Final(response)) => return response,
                Some(QueryEvent::Partial(_)) => {}
                other => panic!("direct comparison request failed: {other:?}"),
            }
        }
    })
    .await;
    // Compare the entire payload, including every candidate, score, winrate,
    // ownership and policy entry. Only the session-allocated wire id differs.
    expected.id = actual.id.clone();
    assert_eq!(actual, expected);
    stop(handle, engine).await;
}

#[tokio::test]
async fn invalid_focus_keeps_the_previous_query_and_status_usable() {
    let engine = engine(&[]);
    let (handle, mut output) = coordinator(&engine, live(1_000_000));
    let current = focus(1, 1);
    within(handle.focus(current.clone())).await.unwrap();
    let before = snapshot(&mut output).await;
    belongs_to(&before, &current);
    let mut zero_interval = live(30);
    zero_interval.report_interval = Duration::ZERO;
    for invalid_settings in [live(0), zero_interval] {
        assert!(matches!(
            within(handle.set_live_settings(invalid_settings)).await,
            Err(AnalysisError::InvalidSettings(_))
        ));
        assert_eq!(*handle.status().borrow(), AnalysisStatus::Analyzing);
    }
    let mut wrong_key = focus(2, 2);
    wrong_key.key.0[0] ^= 1;
    assert_eq!(
        within(handle.focus(wrong_key)).await,
        Err(AnalysisError::KeyMismatch)
    );
    assert_eq!(*handle.status().borrow(), AnalysisStatus::Analyzing);
    let mut invalid = focus(2, 2);
    invalid.position.komi = f64::NAN;
    invalid.key = invalid.position.analysis_key();
    let result = within(handle.focus(invalid)).await;
    assert!(matches!(result, Err(AnalysisError::InvalidPosition(_))));
    assert_eq!(*handle.status().borrow(), AnalysisStatus::Analyzing);
    let after = snapshot(&mut output).await;
    belongs_to(&after, &current);
    assert_eq!(before.response.id, after.response.id);
    within(handle.set_live_settings(live(30))).await.unwrap();
    let result = final_snapshot(&mut output, &current).await;
    assert_eq!(result.response.root_info.visits, 30);
    stop(handle, engine).await;
}

#[tokio::test]
async fn same_focus_can_retry_after_session_capacity_is_released() {
    let engine = engine(&["--threads", "1"]);
    let mut occupied = Vec::new();
    for _ in 0..128 {
        let mut request = barrier_request();
        request.max_visits = Some(1_000_000);
        request.report_during_search_every = Some(0.01);
        occupied.push(within(engine.submit(request)).await.unwrap());
    }
    let (handle, mut output) = coordinator(&engine, live(30));
    let current = focus(1, 0);
    assert_eq!(
        within(handle.focus(current.clone())).await,
        Err(AnalysisError::Session(SessionError::Capacity))
    );
    for query in occupied {
        within(engine.terminate(query.id().to_owned()))
            .await
            .unwrap();
    }
    // This is exactly the same identity, position and settings as the failed
    // command. Idempotence must not suppress a retry when no query is active.
    within(handle.focus(current.clone())).await.unwrap();
    let result = final_snapshot(&mut output, &current).await;
    assert_eq!(result.response.root_info.visits, 30);
    stop(handle, engine).await;
}

#[tokio::test]
async fn query_triggered_terminal_failure_is_reported_without_fabricating_analysis() {
    let engine = engine_with_config(
        &["--crash-after-queries", "1"],
        SessionConfig {
            max_restarts: 0,
            ..SessionConfig::default()
        },
    );
    let (handle, mut output) = coordinator(&engine, live(30));
    within(handle.focus(focus(1, 0))).await.unwrap();
    let mut status = handle.status();
    within(async {
        loop {
            let value = status.borrow_and_update().clone();
            if matches!(value, AnalysisStatus::Failed(AnalysisError::Query(_))) {
                return;
            }
            assert!(!matches!(value, AnalysisStatus::Stopped));
            status.changed().await.unwrap();
        }
    })
    .await;
    tokio::select! {
        biased;
        value = output.recv() => panic!("failure fabricated analysis output: {value:?}"),
        _ = std::future::ready(()) => {}
    }
    within(handle.shutdown()).await.unwrap();
    assert!(within(output.recv()).await.is_none());
    within(engine.shutdown()).await.unwrap();
}
