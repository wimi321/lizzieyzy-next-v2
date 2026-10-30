#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::ffi::OsString;

const SAFETY_TIMEOUT: Duration = Duration::from_secs(5);
const NON_TARGET_TIMEOUT: Duration = Duration::from_secs(60);

fn spec(extra: &[&str]) -> TransportSpec {
    TransportSpec::LocalProcess {
        program: fake_katago::binary_path().expect("build the fake engine"),
        args: [
            "analysis",
            "-config",
            "unused.cfg",
            "-model",
            "unused.bin.gz",
        ]
        .into_iter()
        .chain(extra.iter().copied())
        .map(OsString::from)
        .collect(),
        cwd: None,
        env: BTreeMap::new(),
    }
}

fn config() -> SessionConfig {
    SessionConfig {
        startup_timeout: NON_TARGET_TIMEOUT,
        response_timeout: NON_TARGET_TIMEOUT,
        write_timeout: NON_TARGET_TIMEOUT,
        shutdown_timeout: NON_TARGET_TIMEOUT,
        ..SessionConfig::default()
    }
}

async fn next_line(incoming: &mut mpsc::Receiver<Output>) -> Value {
    match incoming.recv().await.expect("engine output") {
        Output::Stdout(line) => serde_json::from_str(&line).unwrap(),
        Output::Stderr(line) => panic!("unexpected stderr: {line}"),
        Output::Fault(error) => panic!("unexpected I/O error: {error}"),
    }
}

async fn drain(incoming: &mut mpsc::Receiver<Output>) -> Vec<String> {
    let mut stderr = Vec::new();
    while let Some(line) = incoming.recv().await {
        match line {
            Output::Stdout(_) => {}
            Output::Stderr(line) => stderr.push(line),
            Output::Fault(error) => panic!("unexpected I/O error: {error}"),
        }
    }
    stderr
}

#[tokio::test]
async fn graceful_stop_waits_for_successful_exit_and_closes_channels() {
    let transport = start(&spec(&[]), &config()).unwrap();
    timeout(SAFETY_TIMEOUT, async move {
        let Transport {
            outgoing,
            mut incoming,
            exited,
            stop,
            driver,
        } = transport;
        let task = tokio::spawn(driver);
        outgoing
            .send(json!({"id":"ready", "action":"query_version"}).to_string())
            .await
            .unwrap();
        assert_eq!(next_line(&mut incoming).await["version"], "1.16.5-fake");
        stop.unwrap().send(()).unwrap();
        assert!(drain(&mut incoming).await.is_empty());
        let report = exited.await.unwrap();
        assert_eq!(report.status.unwrap().code(), Some(0));
        assert!(report.stderr_tail.is_empty());
        task.await.unwrap();
        assert!(outgoing.is_closed());
    })
    .await
    .expect("graceful stop must reap within the safety deadline");
}

#[tokio::test]
async fn hung_process_is_killed_and_reaped_after_shutdown_deadline() {
    let cfg = SessionConfig {
        // This is the only short production deadline in these transport tests:
        // the injected hang cannot exit on stdin EOF, so forced shutdown is required.
        shutdown_timeout: Duration::from_millis(100),
        ..config()
    };
    let transport = start(&spec(&["--hang-after-queries", "0"]), &cfg).unwrap();
    timeout(SAFETY_TIMEOUT, async move {
        let Transport {
            outgoing: _,
            mut incoming,
            exited,
            stop,
            driver,
        } = transport;
        let task = tokio::spawn(driver);
        let stopping_at = Instant::now();
        stop.unwrap().send(()).unwrap();
        assert!(drain(&mut incoming).await.is_empty());
        let report = exited.await.unwrap();
        let status = report.status.unwrap();
        assert!(!status.success(), "a hung process cannot exit gracefully");
        assert!(stopping_at.elapsed() >= cfg.shutdown_timeout);
        task.await.unwrap();
    })
    .await
    .expect("forced stop must return a real wait result within five seconds");
}

#[tokio::test]
async fn startup_failure_drains_stderr_before_publishing_exit() {
    let transport = start(&spec(&["--fail-startup"]), &config()).unwrap();
    timeout(SAFETY_TIMEOUT, async move {
        let Transport {
            outgoing: _,
            mut incoming,
            exited,
            stop: _stop,
            driver,
        } = transport;
        let task = tokio::spawn(driver);
        let stderr = drain(&mut incoming).await;
        let report = exited.await.unwrap();
        assert_eq!(report.status.unwrap().code(), Some(1));
        assert!(stderr.iter().any(|line| line.contains("模型找不到")));
        assert_eq!(report.stderr_tail, stderr);
        task.await.unwrap();
    })
    .await
    .expect("startup exit and its stderr must arrive within five seconds");
}

#[tokio::test]
async fn closing_input_drains_the_last_analysis_before_exit() {
    let transport = start(&spec(&[]), &config()).unwrap();
    timeout(SAFETY_TIMEOUT, async move {
        let Transport {
            outgoing,
            mut incoming,
            exited,
            stop: _stop,
            driver,
        } = transport;
        let task = tokio::spawn(driver);
        outgoing
            .send(json!({"id":"ready", "action":"query_version"}).to_string())
            .await
            .unwrap();
        assert_eq!(next_line(&mut incoming).await["version"], "1.16.5-fake");
        outgoing
            .send(
                json!({
                    "id":"last", "moves":[], "rules":"chinese",
                    "boardXSize":19, "boardYSize":19, "maxVisits":1
                })
                .to_string(),
            )
            .await
            .unwrap();
        drop(outgoing);
        let final_result = next_line(&mut incoming).await;
        assert_eq!(final_result["id"], "last");
        assert_eq!(final_result["isDuringSearch"], false);
        assert_eq!(final_result["rootInfo"]["visits"], 1);
        assert!(drain(&mut incoming).await.is_empty());
        assert_eq!(exited.await.unwrap().status.unwrap().code(), Some(0));
        task.await.unwrap();
    })
    .await
    .expect("input EOF must preserve the final result and reap the process");
}

#[tokio::test]
async fn outgoing_queue_has_fixed_capacity_and_can_shutdown_while_full() {
    let transport = start(&spec(&[]), &config()).unwrap();
    timeout(SAFETY_TIMEOUT, async move {
        let Transport {
            outgoing,
            mut incoming,
            exited,
            stop,
            driver,
        } = transport;
        let line = json!({"id":"ready", "action":"query_version"}).to_string();
        for _ in 0..CHANNEL_CAPACITY {
            outgoing.try_send(line.clone()).unwrap();
        }
        assert!(matches!(
            outgoing.try_send(line),
            Err(mpsc::error::TrySendError::Full(_))
        ));
        let task = tokio::spawn(driver);
        stop.unwrap().send(()).unwrap();
        drain(&mut incoming).await;
        // Stop may preempt a queued write. The contract here is a reaped
        // process despite backpressure, not a platform-specific exit code.
        let _status = exited.await.unwrap().status.unwrap();
        task.await.unwrap();
    })
    .await
    .expect("a full outgoing queue must not prevent cleanup");
}

#[test]
fn nonexistent_working_directory_is_a_spawn_error() {
    let mut transport = spec(&[]);
    let TransportSpec::LocalProcess { cwd, .. } = &mut transport;
    let missing = std::env::temp_dir().join(format!("missing-{}", uuid::Uuid::new_v4()));
    assert!(!missing.exists(), "test cwd must not exist");
    *cwd = Some(missing);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _entered = runtime.enter();
    // Windows and Unix classify an invalid working directory differently.
    // The transport contract is that the spawn fails, independent of OS errno.
    assert!(
        start(&transport, &config()).is_err(),
        "invalid cwd must fail"
    );
}
