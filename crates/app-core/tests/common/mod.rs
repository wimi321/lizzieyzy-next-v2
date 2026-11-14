use std::{future::Future, panic::Location, time::Duration};

// Capture the caller before creating the future: track_caller has no effect on
// an async fn, so the wrapper must be synchronous.
#[track_caller]
pub fn within<T>(future: impl Future<Output = T>) -> impl Future<Output = T> {
    let caller = Location::caller();
    async move {
        tokio::time::timeout(Duration::from_secs(5), future)
            .await
            .unwrap_or_else(|_| {
                panic!(
                    "operation exceeded safety deadline (5s) at {}:{}",
                    caller.file(),
                    caller.line()
                )
            })
    }
}
