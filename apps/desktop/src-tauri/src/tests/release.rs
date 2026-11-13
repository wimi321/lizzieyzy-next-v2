use super::{Harness, json, local_url, request, within};
use app_core::{
    AppEvent, Settings, UiLanguage, UpdateCheck, UpdateDownloadStatus,
    updates::{UpdateBackend, UpdateError, UpdateFuture, UpdatePhase, UpdateRelease},
};
use tokio::sync::{mpsc, oneshot, watch};

#[tokio::test]
async fn unsigned_build_rejects_update_ipc_with_a_typed_reason() {
    let harness = Harness::new().await;
    let before = harness.core.settings().await.unwrap();
    for command in ["check_update", "download_update", "install_update"] {
        let error = request(&harness.main, command, json!({}), local_url())
            .await
            .unwrap_err();
        assert_eq!(error, json!({ "type": "UpdateSigningNotConfigured" }));
    }
    assert_eq!(
        harness.invoke("automatic_update_check", json!({})).await,
        json!(null)
    );
    assert_eq!(harness.core.settings().await.unwrap(), before);
    assert!(!harness.directory.path().join("settings.json").exists());
    harness.finish().await;
}

#[tokio::test]
async fn release_preferences_round_trip_through_ipc_without_changing_legacy_defaults() {
    let harness = Harness::new().await;
    let initial = harness.invoke("settings", json!({})).await;
    assert_eq!(initial["language"], "System");
    assert_eq!(initial["auto_check_updates"], true);
    assert!(initial["last_update_check_unix_ms"].is_null());
    assert!(!harness.directory.path().join("settings.json").exists());

    for language in [UiLanguage::En, UiLanguage::ZhCn, UiLanguage::System] {
        assert_eq!(
            harness
                .invoke("set_language", json!({ "language": language }))
                .await,
            json!(language)
        );
        assert_eq!(
            harness.invoke("settings", json!({})).await["language"],
            json!(language)
        );
        assert_eq!(
            Settings::load(harness.directory.path())
                .await
                .unwrap()
                .language,
            language
        );
    }
    assert_eq!(
        harness
            .invoke("set_auto_check_updates", json!({ "enabled": false }))
            .await,
        false
    );
    assert_eq!(
        harness.invoke("settings", json!({})).await["auto_check_updates"],
        false
    );
    assert!(
        !Settings::load(harness.directory.path())
            .await
            .unwrap()
            .auto_check_updates
    );
    let before = harness.invoke("settings", json!({})).await;
    assert!(
        request(
            &harness.main,
            "set_language",
            json!({ "language": "unsupported" }),
            local_url(),
        )
        .await
        .is_err()
    );
    assert_eq!(harness.invoke("settings", json!({})).await, before);
    harness.finish().await;
}

#[tokio::test]
async fn full_logging_and_diagnostic_export_are_available_through_local_ipc() {
    let harness = Harness::new().await;
    let inactive = json!({ "enabled": false, "remaining_seconds": 0 });
    assert_eq!(harness.invoke("full_log_status", json!({})).await, inactive);
    let started = harness.invoke("start_full_log", json!({})).await;
    assert_eq!(started["enabled"], true);
    assert!((1..=1800).contains(&started["remaining_seconds"].as_u64().unwrap()));
    assert_eq!(
        harness.invoke("full_log_status", json!({})).await["enabled"],
        true
    );
    assert_eq!(harness.invoke("stop_full_log", json!({})).await, inactive);
    assert_eq!(harness.invoke("full_log_status", json!({})).await, inactive);
    // Persist an explicit preference first, then prove export is read-only for
    // settings. The archive content and redaction are covered in app-core.
    harness
        .invoke("set_auto_check_updates", json!({ "enabled": false }))
        .await;
    let settings_path = harness.directory.path().join("settings.json");
    let before = std::fs::read(&settings_path).unwrap();
    let exported = harness.invoke("export_diagnostics", json!({})).await;
    let path = std::path::Path::new(exported.as_str().unwrap());
    assert_eq!(
        path.parent(),
        Some(harness.directory.path().join("diagnostics").as_path())
    );
    assert_eq!(path.extension().unwrap(), "zip");
    assert!(path.is_file());
    assert!(std::fs::read(path).unwrap().starts_with(b"PK\x03\x04"));
    assert_eq!(std::fs::read(settings_path).unwrap(), before);
    harness.finish().await;
}

struct PendingUpdate {
    gate: Option<oneshot::Receiver<()>>,
    calls: mpsc::UnboundedSender<&'static str>,
}

#[tokio::test]
async fn automatic_update_check_ipc_returns_one_available_release_then_null() {
    let harness = Harness::new().await;
    let (_release_gate, gate) = oneshot::channel();
    let (calls, mut called) = mpsc::unbounded_channel();
    harness
        .core
        .attach_updates(app_core::updates::spawn(Box::new(PendingUpdate {
            gate: Some(gate),
            calls,
        })))
        .await
        .unwrap();
    assert_eq!(
        harness.invoke("automatic_update_check", json!({})).await,
        json!({
            "available": true, "version": "0.2.0", "notes": "Local test update",
            "published_at": "2026-11-12T00:00:00Z",
        })
    );
    assert_eq!(called.try_recv().unwrap(), "check");
    assert_eq!(
        harness.invoke("automatic_update_check", json!({})).await,
        json!(null)
    );
    assert!(called.try_recv().is_err());
    assert!(
        Settings::load(harness.directory.path())
            .await
            .unwrap()
            .last_update_check_unix_ms
            .is_some()
    );
    harness.finish().await;
}

impl UpdateBackend for PendingUpdate {
    fn check(&mut self) -> UpdateFuture<'_, Option<UpdateRelease>> {
        Box::pin(async move {
            self.calls.send("check").unwrap();
            Ok(Some(UpdateRelease {
                view: UpdateCheck {
                    available: true,
                    version: Some("0.2.0".into()),
                    notes: Some("Local test update".into()),
                    published_at: Some("2026-11-12T00:00:00Z".into()),
                },
                // This transfer is cancelled before producing package bytes.
                sha256: [0; 32],
            }))
        })
    }

    fn download(
        &mut self,
        progress: watch::Sender<UpdateDownloadStatus>,
    ) -> UpdateFuture<'_, Vec<u8>> {
        Box::pin(async move {
            self.calls.send("download").unwrap();
            progress.send_modify(|status| {
                status.downloaded_bytes = 2;
                status.total_bytes = Some(4);
            });
            let _ = self.gate.take().unwrap().await;
            Ok(b"test".to_vec())
        })
    }

    fn install(&mut self, _bytes: Vec<u8>) -> UpdateFuture<'_, ()> {
        Box::pin(async move {
            self.calls.send("install").unwrap();
            Err(UpdateError::Backend("unexpected installation".into()))
        })
    }
}

async fn progress(
    harness: &mut Harness,
    callback: u32,
    predicate: impl Fn(&UpdateDownloadStatus) -> bool,
) -> UpdateDownloadStatus {
    within(async {
        loop {
            let message = harness.next().await;
            assert_eq!(message.callback, callback);
            if let AppEvent::UpdateDownloadProgress(status) = message.event
                && predicate(&status)
            {
                return status;
            }
        }
    })
    .await
}

#[tokio::test]
async fn update_ipc_forwards_commands_and_progress_over_the_existing_ordered_channel() {
    let mut harness = Harness::new().await;
    let (release_gate, gate) = oneshot::channel();
    let (calls, mut called) = mpsc::unbounded_channel();
    harness
        .core
        .attach_updates(app_core::updates::spawn(Box::new(PendingUpdate {
            gate: Some(gate),
            calls,
        })))
        .await
        .unwrap();
    let subscription = harness.subscribe(141).await;
    harness.document(141, 0).await;
    assert_eq!(
        harness.invoke("update_download_status", json!({})).await["phase"],
        "Idle"
    );
    let checked = harness.invoke("check_update", json!({})).await;
    assert_eq!(
        checked,
        json!({
            "available": true, "version": "0.2.0", "notes": "Local test update",
            "published_at": "2026-11-12T00:00:00Z",
        })
    );
    assert_eq!(within(called.recv()).await, Some("check"));
    assert_eq!(
        harness.invoke("download_update", json!({})).await["phase"],
        "Downloading"
    );
    assert_eq!(within(called.recv()).await, Some("download"));
    let received = progress(&mut harness, 141, |status| status.downloaded_bytes == 2).await;
    assert_eq!(received.phase, UpdatePhase::Downloading);
    assert_eq!(received.total_bytes, Some(4));
    assert_eq!(received.version.as_deref(), Some("0.2.0"));
    assert_eq!(
        harness.invoke("update_download_status", json!({})).await,
        json!(received)
    );
    // Waiting on the transfer gate must not block document commands.
    assert_eq!(harness.invoke("step", json!({ "count": 0 })).await, 0);
    let cancelled = harness.invoke("cancel_update_download", json!({})).await;
    assert_eq!(cancelled["phase"], "Cancelled");
    assert!(
        release_gate.is_closed(),
        "cancellation must drop the transfer"
    );
    assert_eq!(
        json!(
            progress(&mut harness, 141, |status| status.phase
                == UpdatePhase::Cancelled)
            .await
        ),
        cancelled
    );
    // Only exercise the error path: it must return before the shell requests a
    // restart, and the fake installer must never be called for cancelled bytes.
    let rejected = request(&harness.main, "install_update", json!({}), local_url())
        .await
        .unwrap_err();
    assert_eq!(rejected["type"], "App");
    assert!(called.try_recv().is_err());
    assert!(!harness.directory.path().join("settings.json").exists());
    harness
        .invoke("unsubscribe", json!({ "id": subscription }))
        .await;
    harness.finish().await;
}

#[tokio::test]
async fn release_commands_are_denied_to_other_windows_and_remote_origins() {
    let harness = Harness::new().await;
    let other = tauri::WebviewWindowBuilder::new(&harness.app, "release-other", Default::default())
        .build()
        .unwrap();
    for (window, url) in [
        (&other, local_url()),
        (&harness.main, "https://example.com/"),
    ] {
        for command in [
            "set_language",
            "set_auto_check_updates",
            "check_update",
            "automatic_update_check",
            "download_update",
            "cancel_update_download",
            "update_download_status",
            "install_update",
            "full_log_status",
            "start_full_log",
            "stop_full_log",
            "export_diagnostics",
        ] {
            let error = request(window, command, json!({}), url).await.unwrap_err();
            assert!(
                error
                    .as_str()
                    .is_some_and(|message| message.contains("not allowed")),
                "{command}: {error}"
            );
        }
    }
    harness.finish().await;
}
