#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use std::{cell::Cell, collections::VecDeque, future::Future, task::Poll};

#[tokio::test(start_paused = true)]
async fn retries_only_linux_text_busy_and_stops_at_first_success() {
    let mut outcomes = VecDeque::from([
        Err(io::Error::from_raw_os_error(LINUX_ETXTBSY)),
        Err(io::Error::from_raw_os_error(LINUX_ETXTBSY)),
        Ok("spawned"),
    ]);
    let mut calls = 0;
    let start = Instant::now();
    let value = spawn_with(
        || {
            calls += 1;
            outcomes.pop_front().expect("no spawn after success")
        },
        true,
    )
    .await
    .unwrap();
    assert_eq!(value, "spawned");
    assert_eq!(calls, 3);
    assert_eq!(start.elapsed(), RETRY_DELAY * 2);
}

#[tokio::test(start_paused = true)]
async fn other_errors_and_other_platforms_are_returned_immediately_unchanged() {
    for linux in [true, false] {
        for code in [2, 5, 8, 13, 16, 23, 24, 26, 28] {
            if linux && code == LINUX_ETXTBSY {
                continue;
            }
            let mut calls = 0;
            let start = Instant::now();
            let error = spawn_with::<()>(
                || {
                    calls += 1;
                    Err(io::Error::from_raw_os_error(code))
                },
                linux,
            )
            .await
            .unwrap_err();
            assert_eq!(error.raw_os_error(), Some(code));
            assert_eq!(calls, 1);
            assert_eq!(start.elapsed(), Duration::ZERO);
        }
    }
    let mut calls = 0;
    let error = spawn_with::<()>(
        || {
            calls += 1;
            Err(io::Error::other("Text file busy (os error 26)"))
        },
        true,
    )
    .await
    .unwrap_err();
    assert_eq!(error.raw_os_error(), None);
    assert_eq!(
        calls, 1,
        "text similarity is not the operating system error code"
    );
}

#[tokio::test(start_paused = true)]
async fn permanent_text_busy_stops_after_eight_attempts() {
    let mut calls = 0;
    let start = Instant::now();
    let error = spawn_with::<()>(
        || {
            calls += 1;
            Err(io::Error::from_raw_os_error(LINUX_ETXTBSY))
        },
        true,
    )
    .await
    .unwrap_err();
    assert_eq!(error.raw_os_error(), Some(LINUX_ETXTBSY));
    assert_eq!(calls, MAX_ATTEMPTS);
    assert_eq!(start.elapsed(), RETRY_DELAY * (MAX_ATTEMPTS as u32 - 1));
    assert!(start.elapsed() < RETRY_BUDGET);
}

#[tokio::test(start_paused = true)]
async fn retry_stops_immediately_if_a_later_attempt_has_another_error() {
    let mut calls = 0;
    let error = spawn_with::<()>(
        || {
            calls += 1;
            Err(io::Error::from_raw_os_error(if calls == 1 {
                LINUX_ETXTBSY
            } else {
                13
            }))
        },
        true,
    )
    .await
    .unwrap_err();
    assert_eq!(error.raw_os_error(), Some(13));
    assert_eq!(calls, 2);
}

#[tokio::test(start_paused = true)]
async fn no_retry_runs_after_total_deadline_even_when_the_task_was_delayed() {
    let calls = Cell::new(0);
    let attempt = spawn_with::<()>(
        || {
            calls.set(calls.get() + 1);
            Err(io::Error::from_raw_os_error(LINUX_ETXTBSY))
        },
        true,
    );
    tokio::pin!(attempt);
    std::future::poll_fn(|cx| {
        assert!(attempt.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    // This is virtual scheduling delay after the first observed failure, not
    // a disk/network-speed assumption or sleep used to synchronize a process.
    tokio::time::advance(RETRY_BUDGET + Duration::from_millis(1)).await;
    assert_eq!(
        attempt.await.unwrap_err().raw_os_error(),
        Some(LINUX_ETXTBSY)
    );
    assert_eq!(calls.get(), 1);
}

#[tokio::test(start_paused = true)]
async fn caller_deadline_cancels_pending_retry_without_another_spawn() {
    let mut calls = 0;
    let result = tokio::time::timeout(
        Duration::from_millis(1),
        spawn_with::<()>(
            || {
                calls += 1;
                Err(io::Error::from_raw_os_error(LINUX_ETXTBSY))
            },
            true,
        ),
    )
    .await;
    assert!(result.is_err());
    assert_eq!(calls, 1);
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::{io::Write, os::unix::fs::PermissionsExt, path::PathBuf, process::Stdio};

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target")
                .join(format!("spawn-etxtbsy-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn executable_with_writer(&self) -> (Command, std::fs::File) {
            let path = self.0.join("held-writer.sh");
            let mut writer = std::fs::File::create(&path).unwrap();
            writer.write_all(b"#!/bin/sh\nexit 0\n").unwrap();
            writer.flush().unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            let mut command = Command::new(path);
            command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true);
            (command, writer)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[tokio::test]
    async fn actual_linux_open_writer_reproduces_etxtbsy_and_release_allows_retry() {
        let fixture = Fixture::new();
        let (mut command, writer) = fixture.executable_with_writer();
        assert_eq!(
            command.spawn().unwrap_err().raw_os_error(),
            Some(LINUX_ETXTBSY)
        );
        tokio::time::timeout(Duration::from_secs(5), async {
            let attempt = spawn_process(&mut command);
            tokio::pin!(attempt);
            std::future::poll_fn(|cx| {
                assert!(attempt.as_mut().poll(cx).is_pending());
                Poll::Ready(())
            })
            .await;
            // The first real exec attempt has failed with the writer held.
            // Closing that exact handle now makes the next attempt executable.
            drop(writer);
            let mut child = attempt.await.unwrap();
            assert!(child.wait().await.unwrap().success());
        })
        .await
        .expect("process reaping safety deadline");
    }

    #[tokio::test(start_paused = true)]
    async fn actual_linux_persistent_writer_exhausts_bounded_retries() {
        let fixture = Fixture::new();
        let (mut command, _writer) = fixture.executable_with_writer();
        let start = Instant::now();
        let error = spawn_process(&mut command).await.unwrap_err();
        assert_eq!(error.raw_os_error(), Some(LINUX_ETXTBSY));
        assert_eq!(start.elapsed(), RETRY_DELAY * (MAX_ATTEMPTS as u32 - 1));
    }

    #[tokio::test]
    async fn concurrent_linux_write_then_exec_recovers_for_every_process() {
        // Prepare all files before the safety timer. Every worker retains both
        // writers while other workers fork, exercising inherited-write-handle
        // overlap as well as the deterministic writer held by each caller.
        let cases: Vec<Vec<_>> = (0..8)
            .map(|_| {
                (0..2)
                    .map(|_| {
                        let fixture = Fixture::new();
                        let (mut command, writer) = fixture.executable_with_writer();
                        assert_eq!(
                            command.spawn().unwrap_err().raw_os_error(),
                            Some(LINUX_ETXTBSY)
                        );
                        (fixture, command, writer)
                    })
                    .collect()
            })
            .collect();
        tokio::time::timeout(Duration::from_secs(5), async {
            let mut workers = tokio::task::JoinSet::new();
            for cases in cases {
                workers.spawn(async move {
                    for (_fixture, mut command, writer) in cases {
                        let attempt = spawn_process(&mut command);
                        tokio::pin!(attempt);
                        std::future::poll_fn(|cx| {
                            assert!(attempt.as_mut().poll(cx).is_pending());
                            Poll::Ready(())
                        })
                        .await;
                        // The retry timer proves this specific exec attempt
                        // observed ETXTBSY; no timing guess releases the writer.
                        drop(writer);
                        let mut child = attempt.await.unwrap();
                        assert!(child.wait().await.unwrap().success());
                    }
                });
            }
            while let Some(result) = workers.join_next().await {
                result.unwrap();
            }
        })
        .await
        .expect("concurrent exec/reaping safety deadline");
    }
}
