#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{fmt::Write, future::Future, panic::Location, time::Duration};

use tokio::sync::{mpsc, oneshot, watch};
use tracing::{Subscriber, field::Visit, instrument::WithSubscriber};
use tracing_subscriber::{Layer, layer::Context, prelude::*};

use super::*;
use crate::{
    Settings,
    updates::{UpdateBackend, UpdateError, UpdateFuture, UpdateRelease},
};

// Keep the caller outside the async block, as in the integration-test helper.
#[track_caller]
fn within<T>(future: impl Future<Output = T>) -> impl Future<Output = T> {
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

const DAY: u64 = 86_400_000;
type CheckReply = oneshot::Sender<Result<Option<UpdateRelease>, UpdateError>>;

struct Backend {
    checks: mpsc::UnboundedSender<CheckReply>,
}

impl UpdateBackend for Backend {
    fn check(&mut self) -> UpdateFuture<'_, Option<UpdateRelease>> {
        Box::pin(async move {
            let (reply, response) = oneshot::channel();
            self.checks.send(reply).map_err(|_| UpdateError::Closed)?;
            response.await.map_err(|_| UpdateError::Closed)?
        })
    }

    fn download(&mut self, _: watch::Sender<UpdateDownloadStatus>) -> UpdateFuture<'_, Vec<u8>> {
        panic!("automatic checks must not download")
    }

    fn install(&mut self, _: Vec<u8>) -> UpdateFuture<'_, ()> {
        panic!("automatic checks must not install")
    }
}

async fn attach(core: &AppCore) -> mpsc::UnboundedReceiver<CheckReply> {
    let (checks, received) = mpsc::unbounded_channel();
    let handle = crate::updates::spawn(Box::new(Backend { checks }));
    core.attach_updates(handle).await.unwrap();
    received
}

fn available() -> UpdateRelease {
    UpdateRelease {
        view: UpdateCheck {
            available: true,
            version: Some("2.0.0".into()),
            notes: Some("new release".into()),
            published_at: Some("2026-11-12T00:00:00Z".into()),
        },
        sha256: [0; 32],
    }
}

struct Warnings(mpsc::UnboundedSender<String>);

impl<S: Subscriber> Layer<S> for Warnings {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        if *event.metadata().level() == tracing::Level::WARN
            && event.metadata().target() == "app_core::release"
        {
            let mut fields = Fields(String::new());
            event.record(&mut fields);
            let _ = self.0.send(fields.0);
        }
    }
}

struct Fields(String);

impl Visit for Fields {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let _ = write!(self.0, " {}={value:?}", field.name());
    }
}

fn warnings() -> (tracing::Dispatch, mpsc::UnboundedReceiver<String>) {
    let (sender, receiver) = mpsc::unbounded_channel();
    (
        tracing::Dispatch::new(tracing_subscriber::registry().with(Warnings(sender))),
        receiver,
    )
}

async fn one_warning(
    subscriber: tracing::Dispatch,
    receiver: &mut mpsc::UnboundedReceiver<String>,
    detail: &str,
) {
    let warning = within(receiver.recv())
        .await
        .expect("one warning from the automatic check");
    assert!(warning.contains("automatic update check failed"));
    assert!(warning.contains(detail), "unexpected warning: {warning}");
    // The check has returned. Keep its collector alive through the first
    // assertion, then close it to establish producer completion for the second.
    drop(subscriber);
    assert!(
        within(receiver.recv()).await.is_none(),
        "failure must be logged only once"
    );
}

#[tokio::test]
async fn automatic_check_without_a_signing_backend_does_not_read_clock_or_claim() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = within(crate::spawn(directory.path())).await.unwrap();
    let (subscriber, mut logged) = warnings();
    assert!(
        within(core.automatic_update_check_with_clock(|| panic!(
            "missing signing backend must be checked first"
        )))
        .with_subscriber(subscriber.clone())
        .await
        .is_none()
    );
    assert_eq!(
        within(core.settings())
            .await
            .unwrap()
            .last_update_check_unix_ms,
        None
    );
    assert!(!directory.path().join("settings.json").exists());
    drop(subscriber);
    assert!(within(logged.recv()).await.is_none());
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn automatic_check_disabled_does_not_claim_or_call_backend() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = within(crate::spawn(directory.path())).await.unwrap();
    let mut checks = within(attach(&core)).await;
    within(core.set_auto_check_updates(false)).await.unwrap();
    assert!(within(core.automatic_update_check_at(DAY)).await.is_none());
    assert!(checks.try_recv().is_err());
    assert_eq!(
        within(Settings::load(directory.path()))
            .await
            .unwrap()
            .last_update_check_unix_ms,
        None
    );
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn concurrent_automatic_checks_persist_one_claim_before_one_backend_call() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = within(crate::spawn(directory.path())).await.unwrap();
    let baseline = within(core.settings()).await.unwrap();
    let mut checks = within(attach(&core)).await;
    let mut pending = Vec::new();
    for _ in 0..4 {
        let core = core.clone();
        pending.push(tokio::spawn(async move {
            core.automatic_update_check_at(DAY).await
        }));
    }
    // The backend cannot proceed until this test responds. Observing this call
    // therefore proves persistence preceded check(), without a timing race.
    let reply = within(checks.recv()).await.unwrap();
    assert_eq!(
        within(Settings::load(directory.path()))
            .await
            .unwrap()
            .last_update_check_unix_ms,
        Some(DAY)
    );
    assert!(
        within(core.automatic_update_check_at(DAY + 1))
            .await
            .is_none()
    );
    assert_eq!(
        within(core.settings()).await.unwrap().pondering,
        baseline.pondering
    );
    assert_eq!(
        within(core.settings()).await.unwrap().engine,
        baseline.engine
    );
    reply.send(Ok(Some(available()))).unwrap();
    let mut results = Vec::new();
    for pending in pending {
        // Start the completion deadline only after releasing the backend.
        // Persistence assertions above deliberately keep the first call pending.
        results.push(within(pending).await.unwrap());
    }
    assert_eq!(results.iter().filter(|result| result.is_some()).count(), 1);
    assert_eq!(
        results.into_iter().flatten().next().unwrap(),
        available().view
    );
    assert!(checks.try_recv().is_err());
    assert!(
        within(core.automatic_update_check_at(DAY - 1))
            .await
            .is_none()
    );
    assert!(
        within(core.automatic_update_check_at(2 * DAY - 1))
            .await
            .is_none()
    );
    assert!(checks.try_recv().is_err());
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn automatic_no_update_returns_none_and_next_window_starts_after_twenty_four_hours() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = within(crate::spawn(directory.path())).await.unwrap();
    let mut checks = within(attach(&core)).await;
    let (result, ()) = tokio::join!(within(core.automatic_update_check_at(DAY)), async {
        checks.recv().await.unwrap().send(Ok(None)).unwrap();
    });
    assert!(result.is_none());
    assert_eq!(
        within(Settings::load(directory.path()))
            .await
            .unwrap()
            .last_update_check_unix_ms,
        Some(DAY)
    );
    let (result, ()) = tokio::join!(within(core.automatic_update_check_at(2 * DAY)), async {
        checks
            .recv()
            .await
            .unwrap()
            .send(Ok(Some(available())))
            .unwrap();
    });
    assert_eq!(result, Some(available().view));
    assert_eq!(
        within(Settings::load(directory.path()))
            .await
            .unwrap()
            .last_update_check_unix_ms,
        Some(2 * DAY)
    );
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn failed_backend_returns_none_logs_once_and_keeps_claim_across_restart() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = within(crate::spawn(directory.path())).await.unwrap();
    let mut checks = within(attach(&core)).await;
    let (subscriber, mut logged) = warnings();
    let (result, ()) = tokio::join!(
        within(core.automatic_update_check_at(DAY)).with_subscriber(subscriber.clone()),
        async {
            checks
                .recv()
                .await
                .unwrap()
                .send(Err(UpdateError::Backend(
                    "scripted manifest failure".into(),
                )))
                .unwrap();
        }
    );
    assert!(result.is_none());
    one_warning(subscriber, &mut logged, "scripted manifest failure").await;
    assert_eq!(
        within(core.update_download_status()).await.unwrap(),
        UpdateDownloadStatus::default()
    );
    assert_eq!(
        within(Settings::load(directory.path()))
            .await
            .unwrap()
            .last_update_check_unix_ms,
        Some(DAY)
    );
    within(core.shutdown()).await.unwrap();
    let (core, _) = within(crate::spawn(directory.path())).await.unwrap();
    let mut checks = within(attach(&core)).await;
    assert!(
        within(core.automatic_update_check_at(DAY + 1))
            .await
            .is_none()
    );
    assert!(checks.try_recv().is_err());
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn automatic_failure_never_publishes_failed_progress_to_app_subscription() {
    let directory = tempfile::tempdir().unwrap();
    let (core, mut events) = within(crate::spawn(directory.path())).await.unwrap();
    let mut checks = within(attach(&core)).await;
    // Every failure path installs a collector, including this event-stream test.
    // Otherwise first use here can cache this shared tracing callsite as disabled
    // while a parallel warning test is waiting under its scoped dispatcher.
    let (subscriber, mut logged) = warnings();
    let (result, ()) = tokio::join!(
        within(core.automatic_update_check_at(DAY)).with_subscriber(subscriber.clone()),
        async {
            checks
                .recv()
                .await
                .unwrap()
                .send(Err(UpdateError::Backend("silent network failure".into())))
                .unwrap();
        }
    );
    assert!(result.is_none());
    one_warning(subscriber, &mut logged, "silent network failure").await;
    assert_eq!(
        within(core.update_download_status()).await.unwrap(),
        UpdateDownloadStatus::default()
    );
    // Closing the core gives a deterministic end to the event stream; no sleep
    // or negative timeout is needed to prove absence of a failure event.
    within(core.shutdown()).await.unwrap();
    let mut saw_update_status = false;
    while let Some(event) = events.recv().await {
        if let crate::AppEvent::UpdateDownloadProgress(status) = event {
            saw_update_status = true;
            assert_ne!(status.phase, crate::updates::UpdatePhase::Failed);
            assert!(status.error.is_none());
        }
    }
    assert!(saw_update_status);
}

#[tokio::test]
async fn failed_claim_save_returns_none_logs_once_and_never_calls_backend() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = within(crate::spawn(directory.path())).await.unwrap();
    let mut checks = within(attach(&core)).await;
    // A directory at the settings filename deterministically rejects atomic
    // replacement on every platform; no permissions or I/O deadlines involved.
    std::fs::create_dir(directory.path().join("settings.json")).unwrap();
    let (subscriber, mut logged) = warnings();
    assert!(
        within(core.automatic_update_check_at(DAY))
            .with_subscriber(subscriber.clone())
            .await
            .is_none()
    );
    one_warning(subscriber, &mut logged, "settings").await;
    assert!(checks.try_recv().is_err());
    assert_eq!(
        within(core.settings())
            .await
            .unwrap()
            .last_update_check_unix_ms,
        None
    );
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn failed_clock_returns_none_logs_once_without_claiming_or_calling_backend() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = within(crate::spawn(directory.path())).await.unwrap();
    let mut checks = within(attach(&core)).await;
    let (subscriber, mut logged) = warnings();
    assert!(
        within(core.automatic_update_check_with_clock(|| Err("clock unavailable".into())))
            .with_subscriber(subscriber.clone())
            .await
            .is_none()
    );
    one_warning(subscriber, &mut logged, "clock unavailable").await;
    assert!(checks.try_recv().is_err());
    assert_eq!(
        within(core.settings())
            .await
            .unwrap()
            .last_update_check_unix_ms,
        None
    );
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn closed_core_returns_none_and_logs_handle_failure_once() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = within(crate::spawn(directory.path())).await.unwrap();
    within(core.clone().shutdown()).await.unwrap();
    let (subscriber, mut logged) = warnings();
    assert!(
        within(core.automatic_update_check_at(DAY))
            .with_subscriber(subscriber.clone())
            .await
            .is_none()
    );
    one_warning(subscriber, &mut logged, "closed").await;
}

#[tokio::test]
async fn warning_assertion_waits_for_first_message_and_producer_completion() {
    let (sender, mut receiver) = mpsc::unbounded_channel();
    let subscriber =
        tracing::Dispatch::new(tracing_subscriber::registry().with(Warnings(sender.clone())));
    let assertion = one_warning(subscriber, &mut receiver, "delayed warning");
    tokio::pin!(assertion);
    std::future::poll_fn(|context| {
        assert!(assertion.as_mut().poll(context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    sender
        .send("automatic update check failed: delayed warning".into())
        .unwrap();
    std::future::poll_fn(|context| {
        assert!(assertion.as_mut().poll(context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    drop(sender);
    within(assertion).await;
}

#[tokio::test]
async fn warning_assertion_rejects_a_second_message() {
    let (sender, mut receiver) = mpsc::unbounded_channel();
    let subscriber =
        tracing::Dispatch::new(tracing_subscriber::registry().with(Warnings(sender.clone())));
    for _ in 0..2 {
        sender
            .send("automatic update check failed: duplicate warning".into())
            .unwrap();
    }
    drop(sender);
    let assertion = tokio::spawn(async move {
        one_warning(subscriber, &mut receiver, "duplicate warning").await;
    });
    let error = within(assertion)
        .await
        .expect_err("duplicate warnings must fail the assertion");
    assert!(error.is_panic());
    let payload = error.into_panic();
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap();
    assert!(message.contains("failure must be logged only once"));
}
