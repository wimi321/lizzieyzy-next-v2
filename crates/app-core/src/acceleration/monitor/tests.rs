#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use std::future::Future;

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .unwrap()
}

fn monitor() -> (broadcast::Sender<String>, Monitor) {
    let (sender, receiver) = broadcast::channel(256);
    (
        sender,
        Monitor::new(
            receiver,
            Instant::now(),
            Duration::from_secs(120),
            Duration::from_secs(1800),
        ),
    )
}

#[tokio::test(start_paused = true)]
async fn stderr_tuning_progress_is_throttled_to_five_hundred_milliseconds() {
    let (sender, mut monitor) = monitor();
    sender.send("Performing autotuning".into()).unwrap();
    assert!(within(monitor.next()).await);
    sender
        .send("Tuning xGemmDirect for 1x1 convolutions and matrix mult".into())
        .unwrap();
    assert!(!within(monitor.next()).await);
    within(tokio::time::advance(Duration::from_millis(499))).await;
    assert!(!monitor.progress(Instant::now()));
    within(tokio::time::advance(Duration::from_millis(1))).await;
    assert!(monitor.progress(Instant::now()));
    assert!(!monitor.progress(Instant::now()));
    assert_eq!(monitor.lines().len(), 2);
}

#[tokio::test]
async fn bounded_tail_preserves_backend_and_continues_after_a_lagged_receiver() {
    let (sender, mut monitor) = monitor();
    let backend = "OpenCL backend thread 0: Model version 15";
    sender.send(backend.into()).unwrap();
    within(monitor.next()).await;
    for index in 0..1000 {
        sender.send(format!("diagnostic {index}")).unwrap();
    }
    within(monitor.next()).await;
    monitor.drain();
    let tail = monitor.lines();
    assert_eq!(tail.len(), 20);
    assert_eq!(tail[0], "diagnostic 980");
    assert_eq!(tail[19], "diagnostic 999");
    assert_eq!(monitor.backend_line.as_deref(), Some(backend));
}

#[tokio::test(start_paused = true)]
async fn trial_uses_the_shared_non_additive_tensorrt_and_opencl_deadline() {
    let (sender, mut monitor) = monitor();
    let started = Instant::now();
    within(tokio::time::advance(Duration::from_secs(60))).await;
    sender.send("Creating new plan cache".into()).unwrap();
    within(monitor.next()).await;
    assert_eq!(monitor.clock.deadline(), started + Duration::from_secs(660));
    within(tokio::time::advance(Duration::from_secs(67))).await;
    sender.send("Tuning 40/55 ...".into()).unwrap();
    within(monitor.next()).await;
    assert_eq!(monitor.clock.deadline(), started + Duration::from_secs(660));
    sender
        .send("Tuning dummy thread numeric total: 3.87805e+09".into())
        .unwrap();
    within(monitor.next()).await;
    assert_eq!(monitor.clock.deadline(), started + Duration::from_secs(660));
}

#[tokio::test(start_paused = true)]
async fn ready_session_keeps_late_stderr_without_extending_or_emitting_tuning() {
    let (sender, mut monitor) = monitor();
    monitor.finish();
    let deadline = monitor.clock.deadline();
    within(tokio::time::advance(Duration::from_secs(90))).await;
    sender
        .send("OpenCL backend thread 0: Model version 15".into())
        .unwrap();
    assert!(!within(monitor.next()).await);
    sender.send("Performing autotuning".into()).unwrap();
    assert!(!within(monitor.next()).await);
    assert_eq!(monitor.clock.deadline(), deadline);
    assert_eq!(
        monitor.backend_line.as_deref(),
        Some("OpenCL backend thread 0: Model version 15")
    );
    assert_eq!(monitor.lines().len(), 2);
}

#[tokio::test(start_paused = true)]
async fn ordinary_trial_restart_recomputes_its_attempt_deadline_once() {
    let (_, mut monitor) = monitor();
    within(tokio::time::advance(Duration::from_secs(100))).await;
    let restarted = Instant::now();
    monitor.restarted(1, restarted);
    assert_eq!(
        monitor.clock.deadline(),
        restarted + Duration::from_secs(120)
    );
    within(tokio::time::advance(Duration::from_secs(30))).await;
    monitor.restarted(1, Instant::now());
    assert_eq!(
        monitor.clock.deadline(),
        restarted + Duration::from_secs(120)
    );
    monitor.restarted(2, Instant::now());
    assert_eq!(
        monitor.clock.deadline(),
        Instant::now() + Duration::from_secs(120)
    );
}
