#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use std::time::Duration;

fn actor() -> (Actor, broadcast::Receiver<String>) {
    let (_, commands) = mpsc::channel(1);
    let (status, _) = watch::channel(EngineStatus::Starting);
    let (stderr, lines) = broadcast::channel(256);
    let cfg = SessionConfig::default();
    let startup = StartupDeadline::new(
        Instant::now(),
        cfg.startup_timeout,
        cfg.max_startup_duration,
    );
    (
        Actor {
            remote: false,
            cfg,
            commands,
            status,
            stderr,
            startup,
            pending: BTreeMap::new(),
            outgoing: VecDeque::new(),
            instance: uuid::Uuid::new_v4(),
            sequence: 0,
            attempt: 0,
            ready: false,
            version_id: "version".into(),
            last_output: Instant::now(),
            bad_lines: 0,
            total_bad_lines: 0,
            shutdown: None,
            stopping: false,
            cleanup_error: None,
        },
        lines,
    )
}

#[tokio::test(start_paused = true)]
async fn actual_actor_stderr_extends_startup_only_before_ready() {
    let (mut actor, mut lines) = actor();
    let started = Instant::now();
    tokio::time::advance(Duration::from_secs(100)).await;
    actor.output(transport::Output::Stderr("Tuning 40/55 ...".into()), true);
    assert_eq!(actor.startup.deadline(), started + Duration::from_secs(220));
    assert_eq!(lines.try_recv().unwrap(), "Tuning 40/55 ...");
    actor.output(
        transport::Output::Stdout(
            r#"{"id":"version","action":"query_version","version":"fake","git_hash":"fake"}"#
                .into(),
        ),
        true,
    );
    assert!(actor.ready);
    actor.output(
        transport::Output::Stderr("Creating new plan cache".into()),
        true,
    );
    assert_eq!(actor.startup.deadline(), started + Duration::from_secs(220));
    assert_eq!(lines.try_recv().unwrap(), "Creating new plan cache");
}

#[tokio::test(start_paused = true)]
async fn cleanup_stderr_is_broadcast_without_extending_a_failed_attempt() {
    let (mut actor, mut lines) = actor();
    let initial = actor.startup.deadline();
    tokio::time::advance(Duration::from_secs(100)).await;
    actor.output(
        transport::Output::Stderr("Performing autotuning".into()),
        false,
    );
    assert_eq!(actor.startup.deadline(), initial);
    assert_eq!(lines.try_recv().unwrap(), "Performing autotuning");
}
