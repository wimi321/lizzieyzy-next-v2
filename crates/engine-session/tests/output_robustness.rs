#![allow(clippy::unwrap_used, clippy::expect_used)]

use engine_session::{
    AnalysisRequest, EngineHandle, EngineStatus, QueryEvent, SessionConfig, TransportSpec,
    spawn_with_stderr,
};
use std::{future::Future, time::Duration};
use tokio::sync::broadcast;

const SAFETY: Duration = Duration::from_secs(5);
const MIB: usize = 1024 * 1024;
const GBK_LOG: &[u8] = b"Loading model C:\\Users\\\xD5\xC5\xC8\xFD\\katago\\model.bin.gz";

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(SAFETY, future)
        .await
        .expect("output robustness operation exceeded five-second safety deadline")
}

fn start(
    arguments: &[&str],
    tolerated_bad_lines: u32,
) -> (EngineHandle, broadcast::Receiver<String>) {
    // Build before entering any deadline: these tests measure pipe handling,
    // not compilation time on a clean CI runner.
    let program = fake_katago::binary_path().expect("build fake-katago");
    let args = std::iter::once("analysis")
        .chain(arguments.iter().copied())
        .map(Into::into)
        .collect();
    spawn_with_stderr(
        TransportSpec::LocalProcess {
            program,
            args,
            cwd: None,
            env: Default::default(),
        },
        SessionConfig {
            startup_timeout: Duration::from_secs(60),
            response_timeout: Duration::from_secs(60),
            write_timeout: Duration::from_secs(60),
            shutdown_timeout: Duration::from_millis(100),
            max_restarts: 0,
            max_consecutive_bad_lines: tolerated_bad_lines,
            ..Default::default()
        },
    )
}

async fn ready(engine: &EngineHandle) {
    within(async {
        let mut status = engine.status();
        loop {
            match status.borrow_and_update().clone() {
                EngineStatus::Ready { .. } => return,
                EngineStatus::Failed { reason, .. } => panic!("readiness failed: {reason}"),
                _ => {}
            }
            status.changed().await.unwrap();
        }
    })
    .await;
}

fn request() -> AnalysisRequest {
    serde_json::from_value(serde_json::json!({
        "moves": [], "rules": "chinese", "boardXSize": 19, "boardYSize": 19,
        "maxVisits": 1
    }))
    .unwrap()
}

async fn final_without_restart(engine: &EngineHandle) {
    within(async {
        let mut query = engine.submit(request()).await.unwrap();
        match query.recv().await {
            Some(QueryEvent::Final(response)) => assert_eq!(response.root_info.visits, 1),
            event => panic!("expected a final reply without restarting, got {event:?}"),
        }
        assert!(query.recv().await.is_none());
        assert!(matches!(
            *engine.status().borrow(),
            EngineStatus::Ready { .. }
        ));
    })
    .await;
}

async fn failure(engine: &EngineHandle) -> (String, Vec<String>) {
    within(async {
        let mut status = engine.status();
        loop {
            if let EngineStatus::Failed {
                reason,
                stderr_tail,
            } = status.borrow_and_update().clone()
            {
                return (reason, stderr_tail);
            }
            status.changed().await.unwrap();
        }
    })
    .await
}

#[cfg(not(windows))]
fn expected_gbk_log() -> String {
    String::from_utf8_lossy(GBK_LOG).into_owned()
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn expected_gbk_log() -> String {
    use windows_sys::Win32::Globalization::{CP_ACP, MB_ERR_INVALID_CHARS, MultiByteToWideChar};

    let mut decoded = vec![0u16; GBK_LOG.len()];
    // SAFETY: both slices remain alive, their lengths fit i32, and the output
    // buffer has one UTF-16 slot for each input byte. No pointers are retained.
    let count = unsafe {
        MultiByteToWideChar(
            CP_ACP,
            MB_ERR_INVALID_CHARS,
            GBK_LOG.as_ptr(),
            GBK_LOG.len() as i32,
            decoded.as_mut_ptr(),
            decoded.len() as i32,
        )
    };
    if count == 0 {
        // Windows may have UTF-8 configured as its active ANSI code page.
        String::from_utf8_lossy(GBK_LOG).into_owned()
    } else {
        String::from_utf16(&decoded[..count as usize]).unwrap()
    }
}

#[tokio::test]
async fn startup_gbk_path_becomes_ready_and_remains_in_stderr_tail() {
    let (engine, mut stderr) = start(&["--startup-gbk-path", "--crash-after-queries", "1"], 1);
    ready(&engine).await;
    let expected = expected_gbk_log();
    assert_eq!(within(stderr.recv()).await.unwrap(), expected);
    // Fail deliberately only after Ready, to expose the transport's final
    // diagnostic tail through the public session API.
    let _query = within(engine.submit(request())).await.unwrap();
    let (_, tail) = failure(&engine).await;
    assert!(
        tail.contains(&expected),
        "decoded startup log missing: {tail:?}"
    );
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn invalid_stderr_then_one_mib_is_drained_without_blocking_reply() {
    let (engine, mut stderr) = start(&["--stderr-invalid-flood-bytes", &(MIB).to_string()], 1);
    ready(&engine).await;
    final_without_restart(&engine).await;
    // Completion is a stream barrier, independent of stdout scheduling. The
    // middle line must retain only the allowed prefix, with no giant cache.
    let invalid = within(stderr.recv()).await.unwrap();
    assert!(!invalid.is_empty());
    assert_eq!(within(stderr.recv()).await.unwrap(), "f".repeat(4096));
    assert_eq!(
        within(stderr.recv()).await.unwrap(),
        "fake stderr flood complete"
    );
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn nine_mib_stdout_is_one_bad_line_and_next_reply_arrives_without_restart() {
    let (engine, _) = start(
        &[
            "--stdout-line-before-query",
            "1",
            "--stdout-line-bytes",
            &(9 * MIB).to_string(),
        ],
        1,
    );
    ready(&engine).await;
    final_without_restart(&engine).await;
    // A further request proves the stream and process remain usable.
    final_without_restart(&engine).await;
    within(engine.shutdown()).await.unwrap();
}

async fn assert_one_malformed_line(arguments: &[&str]) {
    let (engine, _) = start(arguments, 0);
    ready(&engine).await;
    let _query = within(engine.submit(request())).await.unwrap();
    let (reason, _) = failure(&engine).await;
    assert_eq!(reason, "too many consecutive malformed lines: 1");
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn nine_mib_valid_json_is_counted_as_malformed_instead_of_parsed() {
    // The fake emits valid padded JSON, so the old unlimited reader accepts
    // it. Setting the tolerance to zero makes actual bad-line accounting
    // observable; the companion test above allows exactly one malformed line.
    assert_one_malformed_line(&[
        "--stdout-line-before-query",
        "1",
        "--stdout-line-bytes",
        &(9 * MIB).to_string(),
    ])
    .await;
}

#[tokio::test]
async fn invalid_stdout_is_counted_as_malformed_instead_of_io_failure() {
    assert_one_malformed_line(&["--stdout-invalid-before-query", "1"]).await;
}

#[tokio::test]
async fn invalid_stdout_does_not_stop_following_replies() {
    let (engine, _) = start(&["--stdout-invalid-before-query", "1"], 1);
    ready(&engine).await;
    final_without_restart(&engine).await;
    final_without_restart(&engine).await;
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn exactly_eight_mib_valid_stdout_is_not_a_bad_line() {
    let (engine, _) = start(
        &[
            "--stdout-line-before-query",
            "1",
            "--stdout-line-bytes",
            &(8 * MIB).to_string(),
        ],
        0,
    );
    ready(&engine).await;
    final_without_restart(&engine).await;
    within(engine.shutdown()).await.unwrap();
}
