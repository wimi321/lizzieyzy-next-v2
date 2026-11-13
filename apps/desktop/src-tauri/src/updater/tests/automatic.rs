use super::*;
use app_core::{AppCore, Settings};

async fn start(server: &Server, directory: &TempDir) -> (tauri::App<MockRuntime>, AppCore) {
    let (app, backend) = adapter(server, directory);
    let (core, _events) = app_core::spawn(directory.path()).await.unwrap();
    crate::release_startup::start_updates(&core, Ok(Box::new(backend)))
        .await
        .unwrap();
    (app, core)
}

#[tokio::test]
async fn automatic_check_disabled_or_already_claimed_makes_no_http_request() {
    for disabled in [true, false] {
        let directory = TempDir::new().unwrap();
        let mut server = Server::new(|_| {}, b"test".to_vec(), false).await;
        let (_app, core) = start(&server, &directory).await;
        if disabled {
            core.set_auto_check_updates(false).await.unwrap();
        } else {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            assert!(core.claim_daily_update_check(now).await.unwrap());
        }
        let before = core.settings().await.unwrap();
        assert_eq!(within(core.automatic_update_check()).await, None);
        assert_eq!(core.settings().await.unwrap(), before);
        within(core.shutdown()).await.unwrap();
        assert_eq!(Settings::load(directory.path()).await.unwrap(), before);
        server.assert_no_pending_requests().await;
    }
}

#[tokio::test]
async fn automatic_check_persists_due_claim_before_http_and_returns_the_available_release() {
    let directory = TempDir::new().unwrap();
    Settings {
        last_update_check_unix_ms: Some(0),
        ..Default::default()
    }
    .save(directory.path())
    .await
    .unwrap();
    let (release, gate) = oneshot::channel();
    let mut server =
        Server::with_manifest_response(|_| {}, b"test".to_vec(), false, Some(gate), false).await;
    let (_app, core) = start(&server, &directory).await;
    assert_eq!(
        core.settings().await.unwrap().last_update_check_unix_ms,
        Some(0)
    );
    let checking_core = core.clone();
    let checking = tokio::spawn(async move { checking_core.automatic_update_check().await });
    assert_eq!(
        within(server.requests.recv()).await.unwrap(),
        "/latest.json"
    );

    // The server has received the request but cannot answer before this gate.
    // Disk persistence therefore demonstrably precedes network completion.
    let claimed = Settings::load(directory.path()).await.unwrap();
    assert!(claimed.last_update_check_unix_ms.is_some_and(|at| at > 0));
    assert_eq!(core.settings().await.unwrap(), claimed);
    assert_eq!(within(core.automatic_update_check()).await, None);
    release.send(()).unwrap();
    let found = within(checking).await.unwrap().unwrap();
    assert!(found.available);
    assert_eq!(found.version.as_deref(), Some("2.0.0"));
    assert_eq!(found.notes.as_deref(), Some("A local signed update"));
    assert_eq!(within(core.automatic_update_check()).await, None);
    within(core.shutdown()).await.unwrap();
    assert_eq!(Settings::load(directory.path()).await.unwrap(), claimed);
    server.assert_no_pending_requests().await;
}

#[tokio::test]
async fn automatic_check_suppresses_no_release_manifest_and_transport_failures_without_retry() {
    for response in ["no-release", "invalid-manifest", "disconnect"] {
        let directory = TempDir::new().unwrap();
        let mut server = Server::with_manifest_response(
            |manifest| match response {
                "no-release" => manifest["version"] = json!("0.0.0"),
                "invalid-manifest" => manifest["version"] = json!({ "invalid": true }),
                _ => {}
            },
            b"test".to_vec(),
            false,
            None,
            response == "disconnect",
        )
        .await;
        let (_app, core) = start(&server, &directory).await;
        assert_eq!(
            within(core.automatic_update_check()).await,
            None,
            "{response}"
        );
        assert_eq!(
            within(server.requests.recv()).await.unwrap(),
            "/latest.json"
        );
        let status = core.update_download_status().await.unwrap();
        assert_eq!(status.phase, UpdatePhase::Idle, "{response}");
        assert_eq!(status.error, None, "{response}");
        let claimed = Settings::load(directory.path()).await.unwrap();
        assert!(claimed.last_update_check_unix_ms.is_some());
        assert_eq!(within(core.automatic_update_check()).await, None);
        assert_eq!(core.update_download_status().await.unwrap(), status);
        within(core.shutdown()).await.unwrap();
        assert_eq!(Settings::load(directory.path()).await.unwrap(), claimed);
        server.assert_no_pending_requests().await;
    }
}
