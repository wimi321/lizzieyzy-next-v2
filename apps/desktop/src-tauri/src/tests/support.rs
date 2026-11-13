use super::{Harness, json, local_url, request};
use crate::reveal::{RevealPlatform, RevealService};
use std::path::{Path, PathBuf};
use tauri::Manager;
use tokio::sync::mpsc;

struct FakePlatform {
    calls: mpsc::UnboundedSender<PathBuf>,
    result: Result<(), String>,
}

impl RevealPlatform for FakePlatform {
    fn reveal(&self, path: &Path) -> Result<(), String> {
        self.calls.send(path.to_owned()).unwrap();
        self.result.clone()
    }
}

fn platform(harness: &Harness, result: Result<(), String>) -> mpsc::UnboundedReceiver<PathBuf> {
    let (calls, received) = mpsc::unbounded_channel();
    assert!(
        harness
            .app
            .manage(RevealService::with_platform(FakePlatform { calls, result }))
    );
    received
}

#[tokio::test]
async fn log_directory_ipc_returns_the_configured_location_without_creating_it() {
    let data = tempfile::TempDir::new().unwrap();
    let root = data.path().join("application data");
    let harness = Harness::with_config(app_core::AppConfig {
        cache_directory: Some(root.clone()),
        ..Default::default()
    })
    .await;
    let logs = root.join("logs");
    assert!(!logs.exists());
    assert_eq!(
        harness.invoke("log_directory", json!({})).await,
        json!(logs)
    );
    assert!(!logs.exists());
    assert!(!harness.directory.path().join("settings.json").exists());
    harness.finish().await;
}

#[tokio::test]
async fn reveal_ipc_passes_only_canonical_internal_paths_to_the_platform() {
    let harness = Harness::new().await;
    let mut calls = platform(&harness, Ok(()));
    let directory = harness
        .directory
        .path()
        .join("日志 spaces,comma;$(literal)");
    std::fs::create_dir_all(directory.join("nested")).unwrap();
    let file = directory.join("诊断, with spaces.zip");
    std::fs::write(&file, b"test archive").unwrap();
    for input in [directory.join("nested/../诊断, with spaces.zip"), directory] {
        assert_eq!(
            harness
                .invoke("reveal_in_folder", json!({ "path": input }))
                .await,
            json!(null)
        );
        // The IPC reply follows the platform call, so no timing or polling is needed.
        assert_eq!(
            calls.try_recv().unwrap(),
            std::fs::canonicalize(input).unwrap()
        );
    }
    let exported = harness.invoke("export_diagnostics", json!({})).await;
    harness
        .invoke("reveal_in_folder", json!({ "path": exported }))
        .await;
    assert_eq!(
        calls.try_recv().unwrap(),
        std::fs::canonicalize(exported.as_str().unwrap()).unwrap()
    );
    assert!(calls.try_recv().is_err());
    harness.finish().await;
}

#[tokio::test]
async fn reveal_ipc_rejects_outside_relative_parent_and_missing_paths_before_platform_call() {
    let harness = Harness::new().await;
    let mut calls = platform(&harness, Ok(()));
    let outside = tempfile::TempDir::new().unwrap();
    let outside_file = outside.path().join("outside.zip");
    std::fs::write(&outside_file, b"test").unwrap();
    for input in [
        outside_file,
        harness.directory.path().join(".."),
        PathBuf::from("logs/relative.zip"),
    ] {
        let error = request(
            &harness.main,
            "reveal_in_folder",
            json!({ "path": input }),
            local_url(),
        )
        .await
        .unwrap_err();
        assert_eq!(error, json!({ "type": "RevealOutsideDataDirectory" }));
        assert!(calls.try_recv().is_err());
    }
    let error = request(
        &harness.main,
        "reveal_in_folder",
        json!({ "path": harness.directory.path().join("missing.zip") }),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(error["type"], "RevealUnavailable");
    assert!(calls.try_recv().is_err());
    harness.finish().await;
}

#[cfg(unix)]
#[tokio::test]
async fn reveal_ipc_rejects_symlink_escape_before_platform_call() {
    let harness = Harness::new().await;
    let mut calls = platform(&harness, Ok(()));
    let outside = tempfile::TempDir::new().unwrap();
    let link = harness.directory.path().join("outside-link");
    std::os::unix::fs::symlink(outside.path(), &link).unwrap();
    let error = request(
        &harness.main,
        "reveal_in_folder",
        json!({ "path": link }),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(error, json!({ "type": "RevealOutsideDataDirectory" }));
    assert!(calls.try_recv().is_err());
    harness.finish().await;
}

#[tokio::test]
async fn reveal_ipc_preserves_typed_platform_failure() {
    let harness = Harness::new().await;
    let mut calls = platform(&harness, Err("file manager unavailable".into()));
    let file = harness.directory.path().join("diagnostics.zip");
    std::fs::write(&file, b"test").unwrap();
    let error = request(
        &harness.main,
        "reveal_in_folder",
        json!({ "path": file }),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(
        error,
        json!({ "type": "RevealUnavailable", "data": "file manager unavailable" })
    );
    assert_eq!(
        calls.try_recv().unwrap(),
        std::fs::canonicalize(file).unwrap()
    );
    assert!(calls.try_recv().is_err());
    harness.finish().await;
}

#[tokio::test]
async fn support_path_commands_deny_other_windows_remote_origins_and_generic_opener() {
    let harness = Harness::new().await;
    let mut calls = platform(&harness, Ok(()));
    let other = tauri::WebviewWindowBuilder::new(&harness.app, "support-other", Default::default())
        .build()
        .unwrap();
    for (window, url) in [
        (&other, local_url()),
        (&harness.main, "https://example.com/"),
    ] {
        for command in ["log_directory", "reveal_in_folder"] {
            let error = request(
                window,
                command,
                json!({ "path": harness.directory.path() }),
                url,
            )
            .await
            .unwrap_err();
            assert!(
                error
                    .as_str()
                    .is_some_and(|message| message.contains("not allowed")),
                "{command}: {error}"
            );
        }
    }
    let error = request(
        &harness.main,
        "plugin:opener|reveal_item_in_dir",
        json!({ "paths": [harness.directory.path()] }),
        local_url(),
    )
    .await
    .unwrap_err();
    assert!(
        error
            .as_str()
            .is_some_and(|message| message.contains("not allowed"))
    );
    assert!(calls.try_recv().is_err());
    harness.finish().await;
}
