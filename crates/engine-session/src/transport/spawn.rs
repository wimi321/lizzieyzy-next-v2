//! Bounded recovery for a transient Linux executable-write race. A sibling
//! fork can inherit a recently closed writer until that child execs, producing
//! ETXTBSY even when this caller has finished publishing the executable.

use std::{io, time::Duration};
use tokio::{
    process::{Child, Command},
    time::{Instant, sleep_until},
};

const MAX_ATTEMPTS: usize = 8;
const RETRY_DELAY: Duration = Duration::from_millis(20);
const RETRY_BUDGET: Duration = Duration::from_millis(250);
const LINUX_ETXTBSY: i32 = 26;

/// Spawn without a shell or shared lock. Only Linux's exact ETXTBSY errno is
/// retried: at most eight attempts, with 20 ms between attempts and no retry
/// admitted after 250 ms. All other errors return immediately and unchanged.
///
/// The budget bounds retries; it cannot interrupt an OS spawn syscall. Callers
/// with an operation deadline should include this future in that same deadline.
pub async fn spawn_process(command: &mut Command) -> io::Result<Child> {
    spawn_with(|| command.spawn(), cfg!(target_os = "linux")).await
}

async fn spawn_with<T>(mut spawn: impl FnMut() -> io::Result<T>, linux: bool) -> io::Result<T> {
    let deadline = Instant::now() + RETRY_BUDGET;
    let mut attempt = 0;
    loop {
        attempt += 1;
        match spawn() {
            Ok(child) => return Ok(child),
            Err(error) => {
                if !linux || error.raw_os_error() != Some(LINUX_ETXTBSY) || attempt == MAX_ATTEMPTS
                {
                    return Err(error);
                }
                let retry_at = Instant::now() + RETRY_DELAY;
                if retry_at >= deadline {
                    return Err(error);
                }
                sleep_until(retry_at).await;
                if Instant::now() >= deadline {
                    return Err(error);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
