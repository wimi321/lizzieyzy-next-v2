use super::{Harness, json, local_url, request, within};
use app_core::AppEvent;

#[test]
fn board_sync_resolves_only_the_bundled_executable() {
    let directory = tempfile::tempdir().unwrap();
    let environment = crate::board_sync::BoardSyncEnvironment::from_resource_dir(directory.path());
    assert_eq!(
        environment.program,
        directory.path().join("readboard").join("readboard.exe")
    );
}

#[tokio::test]
async fn board_sync_commands_report_status_typed_launch_errors_and_idempotent_stop() {
    let mut harness = Harness::new().await;
    harness.subscribe(91).await;
    harness.document(91, 0).await;
    let initial = harness.invoke("board_sync_status", json!({})).await;
    assert_eq!(initial["phase"], "Stopped");
    let error = request(
        &harness.main,
        "start_board_sync",
        json!({ "options": { "always_sync": true, "always_goto_last": true, "language": "Cn" } }),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(error["type"], "BoardSync");
    if cfg!(windows) {
        assert_eq!(error["data"]["type"], "MissingExecutable");
        let path: std::path::PathBuf =
            serde_json::from_value(error["data"]["data"]["path"].clone()).unwrap();
        assert_eq!(
            path,
            harness
                .directory
                .path()
                .join("resources/readboard/readboard.exe")
        );
    } else {
        assert_eq!(error["data"]["type"], "UnsupportedPlatform");
        assert_eq!(
            harness.invoke("board_sync_status", json!({})).await,
            initial
        );
    }
    let stopped = harness.invoke("stop_board_sync", json!({})).await;
    assert_eq!(stopped["phase"], "Stopped");
    assert_eq!(harness.invoke("stop_board_sync", json!({})).await, stopped);
    assert_eq!(
        harness.invoke("board_sync_status", json!({})).await,
        stopped
    );
    // The state event travels through the real bridge and is replayed on the
    // replacement Channel before its subscribe command completes.
    within(async {
        loop {
            if let AppEvent::BoardSyncStatusChanged(status) = harness.next().await.event
                && serde_json::to_value(status).unwrap() == stopped
            {
                break;
            }
        }
    })
    .await;
    harness.subscribe(92).await;
    within(async {
        loop {
            let message = harness.next().await;
            if message.callback == 92
                && let AppEvent::BoardSyncStatusChanged(status) = message.event
            {
                assert_eq!(serde_json::to_value(status).unwrap(), stopped);
                break;
            }
        }
    })
    .await;
    harness.finish().await;
}

#[tokio::test]
async fn board_sync_commands_are_denied_to_other_windows_and_remote_origins() {
    let harness = Harness::new().await;
    let other = tauri::WebviewWindowBuilder::new(&harness.app, "sync-other", Default::default())
        .build()
        .unwrap();
    for command in ["start_board_sync", "stop_board_sync", "board_sync_status"] {
        for (window, url) in [
            (&other, local_url()),
            (&harness.main, "https://example.com/"),
        ] {
            let error = request(window, command, json!({}), url).await.unwrap_err();
            assert!(
                error
                    .as_str()
                    .is_some_and(|text| text.contains("not allowed")),
                "{command}: {error}"
            );
        }
    }
    harness.finish().await;
}
