#![allow(clippy::unwrap_used, clippy::expect_used)]

use engine_session::{
    EngineHandle, EngineStatus, QueryEvent, SessionConfig, TransportSpec, spawn_with_stderr,
};
use std::{future::Future, path::PathBuf, sync::OnceLock, time::Duration};
use tokio::sync::broadcast;

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("stderr test safety deadline")
}

fn binary() -> PathBuf {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY
        .get_or_init(|| fake_katago::binary_path().unwrap())
        .clone()
}

fn config() -> SessionConfig {
    SessionConfig {
        startup_timeout: Duration::from_secs(60),
        response_timeout: Duration::from_secs(60),
        shutdown_timeout: Duration::from_millis(100),
        max_restarts: 0,
        ..Default::default()
    }
}

fn start(arguments: &[&str], config: SessionConfig) -> (EngineHandle, broadcast::Receiver<String>) {
    let program = binary();
    let mut args = vec!["analysis".into()];
    args.extend(arguments.iter().map(Into::into));
    spawn_with_stderr(
        TransportSpec::LocalProcess {
            program,
            args,
            cwd: None,
            env: Default::default(),
        },
        config,
    )
}

async fn ready(engine: &EngineHandle) {
    within(async {
        let mut status = engine.status();
        loop {
            match status.borrow_and_update().clone() {
                EngineStatus::Ready { .. } => return,
                EngineStatus::Failed { reason, .. } => panic!("unexpected failure: {reason}"),
                _ => {}
            }
            status.changed().await.unwrap();
        }
    })
    .await;
}

#[tokio::test]
async fn stderr_subscription_keeps_the_first_line_and_preserves_order() {
    let (engine, mut lines) = start(&["--backend", "opencl", "--tuning-lines", "3"], config());
    assert!(
        within(lines.recv())
            .await
            .unwrap()
            .starts_with("OpenCL backend thread")
    );
    for step in 1..=3 {
        let line = within(lines.recv()).await.unwrap();
        assert!(line.ends_with(&format!("(step {step})")), "{line}");
    }
    ready(&engine).await;
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn lagging_stderr_subscriber_drops_old_lines_without_blocking_readiness() {
    let (engine, mut lines) = start(&["--backend", "opencl", "--tuning-lines", "1000"], config());
    let mut observer = engine.subscribe_stderr();
    // The final stderr line is the barrier: stdout readiness alone cannot order
    // data from the independent stderr pipe. The tested receiver stays unread.
    within(async {
        loop {
            match observer.recv().await {
                Ok(line) if line.ends_with("(step 1000)") => return,
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(error) => panic!("stderr ended before the barrier: {error}"),
            }
        }
    })
    .await;
    ready(&engine).await;
    assert!(matches!(
        lines.try_recv(),
        Err(broadcast::error::TryRecvError::Lagged(_))
    ));
    let mut received = Vec::new();
    while let Ok(line) = lines.try_recv() {
        received.push(line);
    }
    assert_eq!(received.len(), 256);
    assert!(received.last().unwrap().ends_with("(step 1000)"));
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn stderr_lines_are_utf8_bounded_in_stream_and_terminal_diagnostics() {
    let error = "显".repeat(3000);
    let (engine, mut lines) = start(&["--startup-error", &error], config());
    let line = within(lines.recv()).await.unwrap();
    assert!(line.len() <= 4096);
    assert!(line.ends_with('显'));
    let mut status = engine.status();
    let failure = within(status.wait_for(|status| matches!(status, EngineStatus::Failed { .. })))
        .await
        .unwrap()
        .clone();
    let EngineStatus::Failed { stderr_tail, .. } = failure else {
        unreachable!()
    };
    assert!(stderr_tail.iter().all(|line| line.len() <= 4096));
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn the_same_stderr_subscription_receives_output_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("crashed");
    let mut config = config();
    config.max_restarts = 1;
    config.restart_backoff = Duration::from_millis(1);
    config.max_restart_backoff = Duration::from_millis(1);
    let (engine, mut lines) = start(
        &[
            "--backend",
            "cuda",
            "--crash-after-queries",
            "1",
            "--fault-once-file",
            marker.to_str().unwrap(),
        ],
        config,
    );
    let initial = within(lines.recv()).await.unwrap();
    assert!(initial.starts_with("Cuda backend thread"));
    ready(&engine).await;
    let mut second = engine.subscribe_stderr();
    let request = serde_json::from_value(serde_json::json!({
        "moves": [], "rules": "chinese", "boardXSize": 19, "boardYSize": 19,
        "maxVisits": 1
    }))
    .unwrap();
    let mut query = within(engine.submit(request)).await.unwrap();
    within(async {
        loop {
            match query.recv().await {
                Some(QueryEvent::Final(_)) => return,
                Some(QueryEvent::Restarted | QueryEvent::Partial(_)) => {}
                event => panic!("unexpected query event: {event:?}"),
            }
        }
    })
    .await;
    for receiver in [&mut lines, &mut second] {
        within(async {
            loop {
                if receiver.recv().await.unwrap() == initial {
                    return;
                }
            }
        })
        .await;
    }
    within(engine.shutdown()).await.unwrap();
    within(tokio::task::spawn_blocking(move || directory.close()))
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn real_stderr_progress_extends_startup_beyond_its_base_timeout() {
    let directory = tempfile::tempdir().unwrap();
    let gate = directory.path().join("ready");
    let mut config = config();
    config.startup_timeout = Duration::from_millis(400);
    let (engine, mut lines) = start(
        &[
            "--tuning-lines",
            "1",
            "--tuning-interval-ms",
            "20",
            "--startup-gate-file",
            gate.to_str().unwrap(),
        ],
        config,
    );
    let line = within(lines.recv()).await.unwrap();
    assert!(line.starts_with("Performing autotuning"));
    let mut status = engine.status();
    // This timeout measures the feature: the gate remains shut beyond the base
    // deadline, and the session must still be starting thanks to actual stderr.
    assert!(
        within(tokio::time::timeout(
            Duration::from_millis(500),
            status.wait_for(|status| !matches!(status, EngineStatus::Starting)),
        ))
        .await
        .is_err()
    );
    std::fs::write(gate, "").unwrap();
    ready(&engine).await;
    within(engine.shutdown()).await.unwrap();
    within(tokio::task::spawn_blocking(move || directory.close()))
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn a_silent_startup_still_expires_at_its_base_timeout() {
    let mut config = config();
    config.startup_timeout = Duration::from_millis(100);
    let (engine, _) = start(&["--hang-after-queries", "0"], config);
    let mut status = engine.status();
    let failure = within(status.wait_for(|status| matches!(status, EngineStatus::Failed { .. })))
        .await
        .unwrap()
        .clone();
    assert!(matches!(failure, EngineStatus::Failed { reason, .. } if reason.contains("startup")));
    within(engine.shutdown()).await.unwrap();
}
