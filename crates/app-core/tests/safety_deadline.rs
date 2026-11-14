#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::within;
use std::{any::Any, future, time::Duration};

fn panic_message(payload: &(dyn Any + Send)) -> &str {
    if let Some(message) = payload.downcast_ref::<String>() {
        message
    } else {
        payload.downcast_ref::<&str>().unwrap()
    }
}

#[tokio::test]
async fn ready_future_returns_its_value() {
    assert_eq!(within(future::ready(42)).await, 42);
}

#[tokio::test(start_paused = true)]
async fn timeout_reports_exact_call_site_after_five_seconds() {
    let started = tokio::time::Instant::now();
    let expected_line = line!() + 1;
    let task = tokio::spawn(within(future::pending::<()>()));
    let error = task.await.expect_err("pending future must time out");
    assert!(error.is_panic());
    assert_eq!(started.elapsed(), Duration::from_secs(5));
    assert_eq!(
        panic_message(error.into_panic().as_ref()),
        format!(
            "operation exceeded safety deadline (5s) at {}:{expected_line}",
            file!()
        )
    );
}

#[tokio::test(start_paused = true)]
async fn inner_panic_is_not_replaced_by_deadline_message() {
    let task = tokio::spawn(within(async { panic!("inner future panic") }));
    let error = task.await.expect_err("inner panic must propagate");
    assert!(error.is_panic());
    assert_eq!(
        panic_message(error.into_panic().as_ref()),
        "inner future panic"
    );
}
