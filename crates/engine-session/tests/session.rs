#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::future::Future;
use std::path::PathBuf;
use std::time::Duration;

use engine_session::{
    AnalysisRequest, EngineHandle, EngineStatus, QueryEvent, QuerySubscription, SessionConfig,
    TransportSpec, spawn,
};
use katago_protocol::AnalysisResponse;
use serde_json::{Value, json};
use tokio::sync::watch;
use tokio::time::timeout;

const SAFETY: Duration = Duration::from_secs(5);
const NON_TARGET_TIMEOUT: Duration = Duration::from_secs(60);

fn configuration() -> SessionConfig {
    SessionConfig {
        startup_timeout: NON_TARGET_TIMEOUT,
        response_timeout: NON_TARGET_TIMEOUT,
        restart_backoff: Duration::from_millis(40),
        max_restart_backoff: Duration::from_millis(80),
        shutdown_timeout: NON_TARGET_TIMEOUT,
        write_timeout: NON_TARGET_TIMEOUT,
        max_restarts: 2,
        max_consecutive_bad_lines: 3,
    }
}

struct TestFile(PathBuf);

impl TestFile {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("engine-session-{}", uuid::Uuid::new_v4())))
    }

    fn argument(&self) -> &str {
        self.0.to_str().expect("temporary path must be Unicode")
    }

    fn release(&self) {
        std::fs::write(&self.0, []).expect("release the startup gate");
    }

    async fn wait_created(&self) {
        within(async {
            while !self.0.is_file() {
                tokio::task::yield_now().await;
            }
        })
        .await;
    }
}

impl Drop for TestFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

async fn advance_clock(duration: Duration) {
    // Only call after observing the fault/backoff barrier, with no active
    // safety timeout. Resume immediately: paused auto-advance must never race
    // real subprocess I/O, and healthy replay keeps its generous timeout.
    tokio::time::pause();
    tokio::time::advance(duration).await;
    tokio::time::resume();
}

fn engine(arguments: &[&str], config: SessionConfig) -> EngineHandle {
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
    spawn(
        TransportSpec::LocalProcess {
            program: fake_katago::binary_path().expect("build the fake engine"),
            args,
            cwd: None,
            env: BTreeMap::new(),
        },
        config,
    )
}

fn request(extra: Value) -> AnalysisRequest {
    let mut value = json!({
        "moves": [],
        "rules": "chinese",
        "komi": 7.5,
        "boardXSize": 19,
        "boardYSize": 19,
        "maxVisits": 20
    });
    value
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    serde_json::from_value(value).unwrap()
}

fn streaming(visits: u64) -> AnalysisRequest {
    request(json!({
        "maxVisits": visits,
        "reportDuringSearchEvery": 0.01,
        "firstReportDuringSearchAfter": 0.001
    }))
}

async fn within<T>(future: impl Future<Output = T>) -> T {
    timeout(SAFETY, future)
        .await
        .expect("event-driven operation exceeded its safety deadline")
}

async fn event(subscription: &mut QuerySubscription) -> QueryEvent {
    within(subscription.recv())
        .await
        .expect("subscription closed before the expected event")
}

async fn matching_status(
    status: &mut watch::Receiver<EngineStatus>,
    matches: impl Fn(&EngineStatus) -> bool,
) -> EngineStatus {
    within(async {
        loop {
            let value = status.borrow_and_update().clone();
            if matches(&value) {
                return value;
            }
            assert!(
                !matches!(value, EngineStatus::Failed { .. } | EngineStatus::Stopped),
                "unexpected terminal status while awaiting a transition: {value:?}"
            );
            status.changed().await.expect("status sender disappeared");
        }
    })
    .await
}

async fn ready(status: &mut watch::Receiver<EngineStatus>) {
    let value = matching_status(status, |value| matches!(value, EngineStatus::Ready { .. })).await;
    let EngineStatus::Ready { version } = value else {
        unreachable!()
    };
    assert_eq!(version, "1.16.5-fake");
}

async fn final_result(subscription: &mut QuerySubscription) -> AnalysisResponse {
    within(async {
        loop {
            match subscription.recv().await {
                Some(QueryEvent::Partial(_)) => {}
                Some(QueryEvent::Final(response)) => return response,
                other => panic!("expected ordinary analysis, got {other:?}"),
            }
        }
    })
    .await
}

async fn stop(engine: EngineHandle, status: &mut watch::Receiver<EngineStatus>) {
    within(engine.shutdown()).await.unwrap();
    assert!(matches!(*status.borrow(), EngineStatus::Stopped));
}

#[tokio::test]
async fn readiness_streams_partial_then_final_and_allocates_unique_ids() {
    let engine = engine(&[], configuration());
    let mut status = engine.status();
    assert!(matches!(*status.borrow(), EngineStatus::Starting));
    ready(&mut status).await;

    let mut first = within(engine.submit(streaming(60))).await.unwrap();
    let mut second = within(engine.submit(streaming(60))).await.unwrap();
    assert_ne!(first.id(), second.id());
    let first_id = first.id().to_owned();
    let second_id = second.id().to_owned();
    assert!(first_id.rsplit_once('-').is_some());
    assert!(second_id.rsplit_once('-').is_some());
    for (subscription, id) in [(&mut first, first_id), (&mut second, second_id)] {
        let QueryEvent::Partial(partial) = event(subscription).await else {
            panic!("first analysis event must be a partial");
        };
        assert_eq!(partial.id, id);
        assert!(partial.is_during_search);
        let final_value = final_result(subscription).await;
        assert_eq!(final_value.id, id);
        assert!(!final_value.is_during_search);
        assert_eq!(final_value.root_info.visits, 60);
        assert!(within(subscription.recv()).await.is_none());
    }
    stop(engine, &mut status).await;
}

#[tokio::test]
async fn separate_sessions_never_share_query_ids() {
    let first = engine(&[], configuration());
    let second = engine(&[], configuration());
    let mut first_status = first.status();
    let mut second_status = second.status();
    ready(&mut first_status).await;
    ready(&mut second_status).await;
    let mut a = first.submit(request(json!({}))).await.unwrap();
    let mut b = second.submit(request(json!({}))).await.unwrap();
    assert_ne!(a.id(), b.id());
    final_result(&mut a).await;
    final_result(&mut b).await;
    stop(first, &mut first_status).await;
    stop(second, &mut second_status).await;
}

#[tokio::test]
async fn running_termination_discards_buffered_and_late_results() {
    let engine = engine(&["--threads", "1"], configuration());
    let mut status = engine.status();
    ready(&mut status).await;
    let mut running = engine.submit(streaming(100_000)).await.unwrap();
    assert!(matches!(event(&mut running).await, QueryEvent::Partial(_)));
    within(engine.terminate(running.id().to_owned()))
        .await
        .unwrap();
    assert!(matches!(event(&mut running).await, QueryEvent::Terminated));
    assert!(within(running.recv()).await.is_none());

    // A later final proves the actor consumed the cancellation acknowledgement
    // and late running result which precede this next query on the same pipe.
    let mut barrier = engine.submit(request(json!({}))).await.unwrap();
    final_result(&mut barrier).await;
    assert!(within(running.recv()).await.is_none());
    stop(engine, &mut status).await;
}

#[tokio::test]
async fn queued_termination_accepts_no_results_before_acknowledgement() {
    let engine = engine(&["--threads", "1"], configuration());
    let mut status = engine.status();
    ready(&mut status).await;
    let mut blocker = engine.submit(streaming(100_000)).await.unwrap();
    assert!(matches!(event(&mut blocker).await, QueryEvent::Partial(_)));
    let mut queued = engine.submit(streaming(100)).await.unwrap();
    // An invalid query is rejected immediately even while the search slot is
    // occupied. Its response proves the earlier valid query crossed stdin and
    // is queued inside fake-katago, rather than still in the actor's outbox.
    let mut accepted_barrier = engine
        .submit(request(json!({"moves": [["B", "I4"]]})))
        .await
        .unwrap();
    assert!(matches!(
        event(&mut accepted_barrier).await,
        QueryEvent::Failed(_)
    ));
    within(engine.terminate(queued.id().to_owned()))
        .await
        .unwrap();
    assert!(matches!(event(&mut queued).await, QueryEvent::Terminated));
    assert!(within(queued.recv()).await.is_none());
    within(engine.terminate(blocker.id().to_owned()))
        .await
        .unwrap();
    assert!(matches!(event(&mut blocker).await, QueryEvent::Terminated));
    let mut barrier = engine.submit(request(json!({}))).await.unwrap();
    final_result(&mut barrier).await;
    assert!(within(queued.recv()).await.is_none());
    stop(engine, &mut status).await;
}

#[tokio::test]
async fn crash_restarts_and_replays_only_unfinished_queries_with_new_ids() {
    let mut config = configuration();
    config.restart_backoff = NON_TARGET_TIMEOUT;
    config.max_restart_backoff = NON_TARGET_TIMEOUT;
    let engine = engine(&["--crash-after-queries", "2"], config);
    let mut status = engine.status();
    ready(&mut status).await;
    let mut completed = engine.submit(request(json!({}))).await.unwrap();
    final_result(&mut completed).await;
    assert!(within(completed.recv()).await.is_none());

    let mut interrupted = engine.submit(streaming(80)).await.unwrap();
    let original_id = interrupted.id().to_owned();
    // Hold the backoff longer than the safety deadline so observing Restarting
    // never depends on how quickly the test is scheduled.
    matching_status(&mut status, |value| {
        matches!(value, EngineStatus::Restarting { attempt: 1, .. })
    })
    .await;
    advance_clock(NON_TARGET_TIMEOUT + Duration::from_secs(1)).await;
    assert!(matches!(
        event(&mut interrupted).await,
        QueryEvent::Restarted
    ));
    ready(&mut status).await;
    let QueryEvent::Partial(partial) = event(&mut interrupted).await else {
        panic!("replay must continue streaming after Restarted");
    };
    assert_ne!(partial.id, original_id);
    let final_value = final_result(&mut interrupted).await;
    assert_eq!(final_value.id, partial.id);
    assert_eq!(interrupted.id(), original_id);
    assert!(within(interrupted.recv()).await.is_none());
    assert!(within(completed.recv()).await.is_none());
    stop(engine, &mut status).await;
}

#[tokio::test]
async fn startup_failure_is_terminal_and_preserves_stderr() {
    let engine = engine(&["--fail-startup"], configuration());
    let mut status = engine.status();
    let failed = matching_status(&mut status, |value| {
        matches!(value, EngineStatus::Failed { .. })
    })
    .await;
    let EngineStatus::Failed {
        reason,
        stderr_tail,
    } = failed
    else {
        unreachable!()
    };
    assert!(!reason.is_empty());
    assert!(stderr_tail.iter().any(|line| line.contains("模型找不到")));
    assert!(stderr_tail.len() <= 200);
    assert!(engine.submit(request(json!({}))).await.is_err());
    stop(engine, &mut status).await;
}

#[tokio::test]
async fn watchdog_restarts_a_silent_process_and_replays_the_query() {
    let mut config = configuration();
    config.write_timeout = Duration::from_secs(3600);
    config.shutdown_timeout = Duration::from_millis(100);
    let response_timeout = config.response_timeout;
    let marker = TestFile::new();
    let engine = engine(
        &[
            "--hang-after-queries",
            "1",
            "--fault-once-file",
            marker.argument(),
        ],
        config,
    );
    let mut status = engine.status();
    ready(&mut status).await;
    let mut hung = engine.submit(streaming(60)).await.unwrap();
    let old_id = hung.id().to_owned();
    // The marker proves the first request was accepted and deliberately hung.
    // Only then expire its watchdog; no successful warmup/replay needs to beat
    // a short wall-clock deadline on an overloaded machine.
    marker.wait_created().await;
    advance_clock(response_timeout + Duration::from_secs(1)).await;
    assert!(matches!(event(&mut hung).await, QueryEvent::Restarted));
    ready(&mut status).await;
    let result = final_result(&mut hung).await;
    assert_ne!(result.id, old_id);
    assert_eq!(result.root_info.visits, 60);
    stop(engine, &mut status).await;
}

#[tokio::test]
async fn isolated_garbage_below_threshold_does_not_restart() {
    let mut config = configuration();
    config.max_consecutive_bad_lines = 1;
    let engine = engine(&["--garbage-every", "1"], config);
    let mut status = engine.status();
    ready(&mut status).await;
    let mut query = engine.submit(streaming(80)).await.unwrap();
    let original_id = query.id().to_owned();
    let result = final_result(&mut query).await;
    assert_eq!(result.id, original_id);
    assert!(within(query.recv()).await.is_none());
    assert!(matches!(*status.borrow(), EngineStatus::Ready { .. }));
    stop(engine, &mut status).await;
}

#[tokio::test]
async fn garbage_above_threshold_restarts_then_stops_at_retry_limit() {
    let mut config = configuration();
    // fake-katago inserts one malformed line between otherwise valid lines.
    config.max_consecutive_bad_lines = 0;
    config.max_restarts = 1;
    config.restart_backoff = NON_TARGET_TIMEOUT;
    config.max_restart_backoff = NON_TARGET_TIMEOUT;
    let engine = engine(&["--garbage-every", "1"], config);
    let mut status = engine.status();
    let restarting = matching_status(&mut status, |value| {
        matches!(value, EngineStatus::Restarting { attempt: 1, .. })
    })
    .await;
    assert!(
        matches!(restarting, EngineStatus::Restarting { reason, .. } if reason.contains("malformed"))
    );
    advance_clock(NON_TARGET_TIMEOUT + Duration::from_secs(1)).await;
    let failed = matching_status(&mut status, |value| {
        matches!(value, EngineStatus::Failed { .. })
    })
    .await;
    assert!(matches!(failed, EngineStatus::Failed { reason, .. } if reason.contains("malformed")));
    stop(engine, &mut status).await;
}

#[tokio::test]
async fn slow_subscriber_gets_only_latest_partial_and_keeps_the_final() {
    let engine = engine(&["--threads", "1"], configuration());
    let mut status = engine.status();
    ready(&mut status).await;
    let mut slow = engine.submit(streaming(120)).await.unwrap();
    let mut barrier = engine
        .submit(request(json!({"maxVisits": 1})))
        .await
        .unwrap();
    // With one search slot, this query cannot finish until every output from
    // the first query has passed through the actor. No timed wait is needed.
    final_result(&mut barrier).await;
    let QueryEvent::Partial(latest) = event(&mut slow).await else {
        panic!("the latest unconsumed partial must be retained");
    };
    assert!(latest.root_info.visits >= 100);
    assert!(latest.root_info.visits < 120);
    let QueryEvent::Final(result) = event(&mut slow).await else {
        panic!("old partials must be replaced, not queued");
    };
    assert_eq!(result.root_info.visits, 120);
    assert!(within(slow.recv()).await.is_none());
    stop(engine, &mut status).await;
}

#[tokio::test]
async fn queued_high_priority_finishes_before_low_priority() {
    let engine = engine(&["--threads", "1"], configuration());
    let mut status = engine.status();
    ready(&mut status).await;
    let mut blocker = engine.submit(streaming(100_000)).await.unwrap();
    assert!(matches!(event(&mut blocker).await, QueryEvent::Partial(_)));
    let mut low_request = streaming(100_000);
    low_request.priority = Some(-100);
    let mut low = engine.submit(low_request).await.unwrap();
    let mut high = engine
        .submit(request(json!({"priority": 100, "maxVisits": 1})))
        .await
        .unwrap();
    engine.terminate(blocker.id().to_owned()).await.unwrap();
    assert!(matches!(event(&mut blocker).await, QueryEvent::Terminated));
    // If low starts first it occupies the only slot for 100 seconds, so high
    // cannot finish within the safety deadline. Consumer polling order cannot
    // produce a false failure when both subscriptions have buffered events.
    final_result(&mut high).await;
    assert!(matches!(event(&mut low).await, QueryEvent::Partial(_)));
    engine.terminate(low.id().to_owned()).await.unwrap();
    assert!(matches!(event(&mut low).await, QueryEvent::Terminated));
    stop(engine, &mut status).await;
}

#[tokio::test]
async fn repeated_crashes_exhaust_retries_and_reliably_fail_the_query() {
    let mut config = configuration();
    config.max_restarts = 2;
    let engine = engine(&["--crash-after-queries", "1"], config);
    let mut status = engine.status();
    ready(&mut status).await;
    let mut query = engine.submit(request(json!({}))).await.unwrap();
    // Deliberately do not consume query events until the session has failed.
    matching_status(&mut status, |value| {
        matches!(value, EngineStatus::Failed { .. })
    })
    .await;
    assert!(matches!(event(&mut query).await, QueryEvent::Restarted));
    assert!(matches!(event(&mut query).await, QueryEvent::Restarted));
    assert!(matches!(event(&mut query).await, QueryEvent::Failed(_)));
    assert!(within(query.recv()).await.is_none());
    stop(engine, &mut status).await;
}

#[tokio::test]
async fn readiness_timeout_restarts_and_respects_the_retry_limit() {
    let mut config = configuration();
    config.startup_timeout = Duration::from_millis(100);
    config.shutdown_timeout = Duration::from_millis(100);
    config.restart_backoff = NON_TARGET_TIMEOUT;
    config.max_restart_backoff = NON_TARGET_TIMEOUT;
    config.max_restarts = 1;
    // This process never answers query_version, regardless of scheduling speed.
    let engine = engine(&["--hang-after-queries", "0"], config);
    let mut status = engine.status();
    let restarting = matching_status(&mut status, |value| {
        matches!(value, EngineStatus::Restarting { attempt: 1, .. })
    })
    .await;
    assert!(
        matches!(restarting, EngineStatus::Restarting { reason, .. } if reason.contains("startup"))
    );
    advance_clock(NON_TARGET_TIMEOUT + Duration::from_secs(1)).await;
    let failed = matching_status(&mut status, |value| {
        matches!(value, EngineStatus::Failed { .. })
    })
    .await;
    assert!(matches!(failed, EngineStatus::Failed { reason, .. } if reason.contains("startup")));
    stop(engine, &mut status).await;
}

#[tokio::test]
async fn illegal_query_fails_without_poisoning_the_session() {
    let engine = engine(&[], configuration());
    let mut status = engine.status();
    ready(&mut status).await;
    let mut illegal = engine
        .submit(request(json!({"moves": [["B", "I4"]]})))
        .await
        .unwrap();
    assert!(matches!(event(&mut illegal).await, QueryEvent::Failed(_)));
    assert!(within(illegal.recv()).await.is_none());
    let mut valid = engine.submit(request(json!({}))).await.unwrap();
    final_result(&mut valid).await;
    assert!(matches!(*status.borrow(), EngineStatus::Ready { .. }));
    stop(engine, &mut status).await;
}

#[tokio::test]
async fn request_extensions_cannot_forge_protocol_ids_or_actions() {
    let engine = engine(&[], configuration());
    let mut status = engine.status();
    ready(&mut status).await;
    for extra in [json!({"id": "forged"}), json!({"action": "query_version"})] {
        assert!(engine.submit(request(extra)).await.is_err());
    }
    let mut valid = engine.submit(request(json!({}))).await.unwrap();
    final_result(&mut valid).await;
    stop(engine, &mut status).await;
}

#[tokio::test]
async fn multiple_turns_retain_every_final_and_deduplicate_requested_turns() {
    let engine = engine(&["--threads", "1"], configuration());
    let mut status = engine.status();
    ready(&mut status).await;
    let mut query = engine
        .submit(request(json!({
            "moves": [["B", "pass"], ["W", "pass"], ["B", "pass"]],
            "analyzeTurns": [3, 1, 0, 2, 1],
            "priorities": [10, -1, -5, 0, 5],
            "maxVisits": 10
        })))
        .await
        .unwrap();
    let mut barrier = engine
        .submit(request(json!({"priority": -100})))
        .await
        .unwrap();
    final_result(&mut barrier).await;
    let mut turns = Vec::new();
    within(async {
        while let Some(value) = query.recv().await {
            let QueryEvent::Final(result) = value else {
                panic!("unexpected event for a nonstreaming query: {value:?}");
            };
            turns.push(result.turn_number);
        }
    })
    .await;
    assert_eq!(turns, [3, 1, 2, 0]);
    assert_eq!(turns.iter().copied().collect::<BTreeSet<_>>().len(), 4);
    stop(engine, &mut status).await;
}

#[tokio::test]
async fn submit_during_startup_waits_for_readiness_and_then_streams() {
    let gate = TestFile::new();
    let engine = engine(&["--startup-gate-file", gate.argument()], configuration());
    let mut status = engine.status();
    assert!(matches!(*status.borrow(), EngineStatus::Starting));
    let mut query = within(engine.submit(streaming(60))).await.unwrap();
    assert!(matches!(*status.borrow(), EngineStatus::Starting));
    gate.release();
    ready(&mut status).await;
    assert!(matches!(event(&mut query).await, QueryEvent::Partial(_)));
    final_result(&mut query).await;
    stop(engine, &mut status).await;
}

#[tokio::test]
async fn stable_subscription_id_cancels_the_replayed_query() {
    let engine = engine(&["--crash-after-queries", "2"], configuration());
    let mut status = engine.status();
    ready(&mut status).await;
    let mut first = engine.submit(request(json!({}))).await.unwrap();
    final_result(&mut first).await;
    let mut query = engine.submit(streaming(100_000)).await.unwrap();
    let original_id = query.id().to_owned();
    assert!(matches!(event(&mut query).await, QueryEvent::Restarted));
    let QueryEvent::Partial(replayed) = event(&mut query).await else {
        panic!("replayed query should produce a partial");
    };
    assert_ne!(replayed.id, original_id);
    within(engine.terminate(original_id)).await.unwrap();
    assert!(matches!(event(&mut query).await, QueryEvent::Terminated));
    assert!(within(query.recv()).await.is_none());
    stop(engine, &mut status).await;
}

#[tokio::test]
async fn shutdown_is_not_blocked_by_an_unread_subscription() {
    let mut config = configuration();
    // Forcing an unfinished long search to exit is the behavior under test.
    config.shutdown_timeout = Duration::from_millis(100);
    let engine = engine(&[], config);
    let clone = engine.clone();
    let mut status = engine.status();
    ready(&mut status).await;
    let mut query = engine.submit(streaming(100_000)).await.unwrap();
    assert!(matches!(event(&mut query).await, QueryEvent::Partial(_)));
    stop(engine, &mut status).await;
    assert!(matches!(event(&mut query).await, QueryEvent::Failed(_)));
    assert!(within(query.recv()).await.is_none());
    assert!(within(clone.submit(request(json!({})))).await.is_err());
    within(clone.shutdown()).await.unwrap();
}

#[tokio::test]
async fn startup_queue_preserves_equal_priority_fifo_past_ten_requests() {
    let gate = TestFile::new();
    let engine = engine(
        &["--threads", "1", "--startup-gate-file", gate.argument()],
        configuration(),
    );
    let mut status = engine.status();
    let mut queries = Vec::new();
    for _ in 0..16 {
        queries.push(within(engine.submit(streaming(100_000))).await.unwrap());
    }
    assert!(matches!(*status.borrow(), EngineStatus::Starting));
    gate.release();
    ready(&mut status).await;
    // Each long search holds the only slot until cancelled. A wrong queue
    // order blocks the expected subscription; reader scheduling cannot hide it.
    for query in &mut queries {
        assert!(matches!(event(query).await, QueryEvent::Partial(_)));
        within(engine.terminate(query.id().to_owned()))
            .await
            .unwrap();
        assert!(matches!(event(query).await, QueryEvent::Terminated));
        assert!(within(query.recv()).await.is_none());
    }
    stop(engine, &mut status).await;
}

#[tokio::test]
async fn dropping_all_handles_stops_the_process_with_status_observer_retained() {
    let engine = engine(&[], configuration());
    let clone = engine.clone();
    let mut status = engine.status();
    ready(&mut status).await;
    drop(engine);
    assert!(matches!(*status.borrow(), EngineStatus::Ready { .. }));
    drop(clone);
    matching_status(&mut status, |value| matches!(value, EngineStatus::Stopped)).await;
}

#[tokio::test]
async fn terminating_during_restart_backoff_prevents_replay() {
    let mut config = configuration();
    config.restart_backoff = NON_TARGET_TIMEOUT;
    config.max_restart_backoff = NON_TARGET_TIMEOUT;
    let engine = engine(&["--crash-after-queries", "2"], config);
    let mut status = engine.status();
    ready(&mut status).await;
    let mut completed = engine.submit(request(json!({}))).await.unwrap();
    final_result(&mut completed).await;
    let mut interrupted = engine.submit(streaming(100_000)).await.unwrap();
    matching_status(&mut status, |value| {
        matches!(value, EngineStatus::Restarting { attempt: 1, .. })
    })
    .await;
    within(engine.terminate(interrupted.id().to_owned()))
        .await
        .unwrap();
    assert!(matches!(
        event(&mut interrupted).await,
        QueryEvent::Terminated
    ));
    assert!(within(interrupted.recv()).await.is_none());
    advance_clock(NON_TARGET_TIMEOUT + Duration::from_secs(1)).await;
    ready(&mut status).await;
    // If the cancelled query had been replayed, this would be the new process's
    // second query and crash it again instead of finishing without a restart.
    let mut barrier = engine.submit(request(json!({}))).await.unwrap();
    let expected_id = barrier.id().to_owned();
    assert_eq!(final_result(&mut barrier).await.id, expected_id);
    assert!(within(interrupted.recv()).await.is_none());
    stop(engine, &mut status).await;
}
