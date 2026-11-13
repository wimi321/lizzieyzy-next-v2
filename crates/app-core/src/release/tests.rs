#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fmt::Write;

use tokio::sync::{mpsc, oneshot, watch};
use tracing::{Subscriber, field::Visit, instrument::WithSubscriber};
use tracing_subscriber::{Layer, layer::Context, prelude::*};

use super::*;
use crate::{
    Settings,
    updates::{UpdateBackend, UpdateError, UpdateFuture, UpdateRelease},
};

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

fn warnings() -> (
    impl Subscriber + Send + Sync,
    mpsc::UnboundedReceiver<String>,
) {
    let (sender, receiver) = mpsc::unbounded_channel();
    (
        tracing_subscriber::registry().with(Warnings(sender)),
        receiver,
    )
}

fn one_warning(receiver: &mut mpsc::UnboundedReceiver<String>, detail: &str) {
    let warning = receiver
        .try_recv()
        .expect("one warning from the automatic check");
    assert!(warning.contains("automatic update check failed"));
    assert!(warning.contains(detail), "unexpected warning: {warning}");
    assert!(
        receiver.try_recv().is_err(),
        "failure must be logged only once"
    );
}

#[tokio::test]
async fn automatic_check_without_a_signing_backend_does_not_read_clock_or_claim() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = crate::spawn(directory.path()).await.unwrap();
    let (subscriber, mut logged) = warnings();
    assert!(
        core.automatic_update_check_with_clock(|| panic!(
            "missing signing backend must be checked first"
        ))
        .with_subscriber(subscriber)
        .await
        .is_none()
    );
    assert_eq!(
        core.settings().await.unwrap().last_update_check_unix_ms,
        None
    );
    assert!(!directory.path().join("settings.json").exists());
    assert!(logged.try_recv().is_err());
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn automatic_check_disabled_does_not_claim_or_call_backend() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = crate::spawn(directory.path()).await.unwrap();
    let mut checks = attach(&core).await;
    core.set_auto_check_updates(false).await.unwrap();
    assert!(core.automatic_update_check_at(DAY).await.is_none());
    assert!(checks.try_recv().is_err());
    assert_eq!(
        Settings::load(directory.path())
            .await
            .unwrap()
            .last_update_check_unix_ms,
        None
    );
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn concurrent_automatic_checks_persist_one_claim_before_one_backend_call() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = crate::spawn(directory.path()).await.unwrap();
    let baseline = core.settings().await.unwrap();
    let mut checks = attach(&core).await;
    let mut pending = Vec::new();
    for _ in 0..4 {
        let core = core.clone();
        pending.push(tokio::spawn(async move {
            core.automatic_update_check_at(DAY).await
        }));
    }
    // The backend cannot proceed until this test responds. Observing this call
    // therefore proves persistence preceded check(), without a timing race.
    let reply = checks.recv().await.unwrap();
    assert_eq!(
        Settings::load(directory.path())
            .await
            .unwrap()
            .last_update_check_unix_ms,
        Some(DAY)
    );
    assert!(core.automatic_update_check_at(DAY + 1).await.is_none());
    assert_eq!(core.settings().await.unwrap().pondering, baseline.pondering);
    assert_eq!(core.settings().await.unwrap().engine, baseline.engine);
    reply.send(Ok(Some(available()))).unwrap();
    let mut results = Vec::new();
    for pending in pending {
        results.push(pending.await.unwrap());
    }
    assert_eq!(results.iter().filter(|result| result.is_some()).count(), 1);
    assert_eq!(
        results.into_iter().flatten().next().unwrap(),
        available().view
    );
    assert!(checks.try_recv().is_err());
    assert!(core.automatic_update_check_at(DAY - 1).await.is_none());
    assert!(core.automatic_update_check_at(2 * DAY - 1).await.is_none());
    assert!(checks.try_recv().is_err());
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn automatic_no_update_returns_none_and_next_window_starts_after_twenty_four_hours() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = crate::spawn(directory.path()).await.unwrap();
    let mut checks = attach(&core).await;
    let (result, ()) = tokio::join!(core.automatic_update_check_at(DAY), async {
        checks.recv().await.unwrap().send(Ok(None)).unwrap();
    });
    assert!(result.is_none());
    assert_eq!(
        Settings::load(directory.path())
            .await
            .unwrap()
            .last_update_check_unix_ms,
        Some(DAY)
    );
    let (result, ()) = tokio::join!(core.automatic_update_check_at(2 * DAY), async {
        checks
            .recv()
            .await
            .unwrap()
            .send(Ok(Some(available())))
            .unwrap();
    });
    assert_eq!(result, Some(available().view));
    assert_eq!(
        Settings::load(directory.path())
            .await
            .unwrap()
            .last_update_check_unix_ms,
        Some(2 * DAY)
    );
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn failed_backend_returns_none_logs_once_and_keeps_claim_across_restart() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = crate::spawn(directory.path()).await.unwrap();
    let mut checks = attach(&core).await;
    let (subscriber, mut logged) = warnings();
    let (result, ()) = tokio::join!(
        core.automatic_update_check_at(DAY)
            .with_subscriber(subscriber),
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
    one_warning(&mut logged, "scripted manifest failure");
    assert_eq!(
        core.update_download_status().await.unwrap(),
        UpdateDownloadStatus::default()
    );
    assert_eq!(
        Settings::load(directory.path())
            .await
            .unwrap()
            .last_update_check_unix_ms,
        Some(DAY)
    );
    core.shutdown().await.unwrap();
    let (core, _) = crate::spawn(directory.path()).await.unwrap();
    let mut checks = attach(&core).await;
    assert!(core.automatic_update_check_at(DAY + 1).await.is_none());
    assert!(checks.try_recv().is_err());
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn automatic_failure_never_publishes_failed_progress_to_app_subscription() {
    let directory = tempfile::tempdir().unwrap();
    let (core, mut events) = crate::spawn(directory.path()).await.unwrap();
    let mut checks = attach(&core).await;
    let (result, ()) = tokio::join!(core.automatic_update_check_at(DAY), async {
        checks
            .recv()
            .await
            .unwrap()
            .send(Err(UpdateError::Backend("silent network failure".into())))
            .unwrap();
    });
    assert!(result.is_none());
    assert_eq!(
        core.update_download_status().await.unwrap(),
        UpdateDownloadStatus::default()
    );
    // Closing the core gives a deterministic end to the event stream; no sleep
    // or negative timeout is needed to prove absence of a failure event.
    core.shutdown().await.unwrap();
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
    let (core, _) = crate::spawn(directory.path()).await.unwrap();
    let mut checks = attach(&core).await;
    // A directory at the settings filename deterministically rejects atomic
    // replacement on every platform; no permissions or I/O deadlines involved.
    std::fs::create_dir(directory.path().join("settings.json")).unwrap();
    let (subscriber, mut logged) = warnings();
    assert!(
        core.automatic_update_check_at(DAY)
            .with_subscriber(subscriber)
            .await
            .is_none()
    );
    one_warning(&mut logged, "settings");
    assert!(checks.try_recv().is_err());
    assert_eq!(
        core.settings().await.unwrap().last_update_check_unix_ms,
        None
    );
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn failed_clock_returns_none_logs_once_without_claiming_or_calling_backend() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = crate::spawn(directory.path()).await.unwrap();
    let mut checks = attach(&core).await;
    let (subscriber, mut logged) = warnings();
    assert!(
        core.automatic_update_check_with_clock(|| Err("clock unavailable".into()))
            .with_subscriber(subscriber)
            .await
            .is_none()
    );
    one_warning(&mut logged, "clock unavailable");
    assert!(checks.try_recv().is_err());
    assert_eq!(
        core.settings().await.unwrap().last_update_check_unix_ms,
        None
    );
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn closed_core_returns_none_and_logs_handle_failure_once() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = crate::spawn(directory.path()).await.unwrap();
    core.clone().shutdown().await.unwrap();
    let (subscriber, mut logged) = warnings();
    assert!(
        core.automatic_update_check_at(DAY)
            .with_subscriber(subscriber)
            .await
            .is_none()
    );
    one_warning(&mut logged, "closed");
}
