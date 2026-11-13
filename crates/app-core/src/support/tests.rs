use std::{collections::BTreeMap, io::Read};

use serde_json::json;
use tracing_subscriber::layer::SubscriberExt;

use super::*;

fn capture(handle: &SupportHandle, action: impl FnOnce()) {
    let subscriber = tracing_subscriber::registry().with(handle.layer());
    tracing::subscriber::with_default(subscriber, action);
}

async fn contents(handle: &SupportHandle, root: &std::path::Path) -> String {
    handle.flush().await.unwrap();
    std::fs::read_to_string(root.join("logs/app.log")).unwrap_or_default()
}

#[tokio::test]
async fn default_info_logging_is_lazy_and_records_tracing_events() {
    let directory = tempfile::tempdir().unwrap();
    let handle = spawn(directory.path().to_owned()).await.unwrap();
    assert_eq!(
        handle.full_log_status().await.unwrap(),
        FullLogStatus::default()
    );
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    capture(&handle, || {
        tracing::trace!("default-trace-hidden");
        tracing::debug!("default-debug-hidden");
        tracing::info!("default-info-visible");
        tracing::warn!("default-warn-visible");
    });
    let logs = contents(&handle, directory.path()).await;
    assert!(!logs.contains("default-trace-hidden"));
    assert!(!logs.contains("default-debug-hidden"));
    assert!(logs.contains("default-info-visible"));
    assert!(logs.contains("default-warn-visible"));
    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn log_directory_query_is_absolute_and_does_not_create_missing_directories() {
    let unique = tempfile::tempdir().unwrap();
    // A random relative name in the current directory, without changing cwd or
    // creating that name. The temporary directory only supplies a unique name.
    let relative =
        std::path::PathBuf::from(unique.path().file_name().unwrap()).join("uncreated-data");
    let absolute = std::path::absolute(&relative).unwrap();
    assert!(!absolute.exists());
    let handle = spawn(relative).await.unwrap();
    let logs = handle.log_directory().await.unwrap();
    assert_eq!(logs, absolute.join("logs"));
    assert!(logs.is_absolute());
    assert!(!absolute.exists());
    assert!(matches!(
        handle.resolve_reveal_path(logs).await,
        Err(SupportError::Io(_))
    ));
    assert!(!absolute.exists());
    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn relative_data_root_exports_an_absolute_revealable_archive() {
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    std::fs::create_dir_all(&target).unwrap();
    let target = std::fs::canonicalize(target).unwrap();
    let scratch = tempfile::Builder::new()
        .prefix("support-relative-")
        .tempdir_in(target)
        .unwrap();
    let directory = scratch.path().join("new-data");
    let cwd = std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap();
    let mut parents = std::path::PathBuf::new();
    let relative = cwd
        .ancestors()
        .find_map(|ancestor| {
            if let Ok(tail) = directory.strip_prefix(ancestor) {
                Some(parents.join(tail))
            } else {
                parents.push("..");
                None
            }
        })
        .expect("test workspace and its target directory share a filesystem root");
    assert!(!relative.is_absolute());
    let handle = spawn(relative).await.unwrap();
    assert!(!directory.exists());
    let exported = handle
        .export_bundle(
            Settings::default(),
            EngineDiagnostics {
                profile: None,
                checks: Vec::new(),
            },
            "test".into(),
        )
        .await
        .unwrap();
    assert!(exported.is_absolute());
    assert_eq!(
        handle.resolve_reveal_path(exported.clone()).await.unwrap(),
        std::fs::canonicalize(exported).unwrap()
    );
    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn reveal_accepts_existing_files_directories_root_and_normalizes_parent_components() {
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("data");
    std::fs::create_dir_all(data.join("logs")).unwrap();
    let file = data.join("diagnostic.zip");
    std::fs::write(&file, "example").unwrap();
    let handle = spawn(data.clone()).await.unwrap();
    for path in [
        data.clone(),
        data.join("logs"),
        file.clone(),
        data.join("logs/../diagnostic.zip"),
    ] {
        assert_eq!(
            handle.resolve_reveal_path(path.clone()).await.unwrap(),
            std::fs::canonicalize(path).unwrap()
        );
    }
    assert!(matches!(
        handle.resolve_reveal_path(data.join("missing")).await,
        Err(SupportError::Io(_))
    ));
    assert!(matches!(
        handle
            .resolve_reveal_path(std::path::PathBuf::from("logs"))
            .await,
        Err(SupportError::RevealOutsideDataDirectory)
    ));
    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn reveal_rejects_same_prefix_siblings_and_parent_escapes() {
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("data");
    let sibling = directory.path().join("data-other");
    std::fs::create_dir(&data).unwrap();
    std::fs::create_dir(&sibling).unwrap();
    std::fs::write(sibling.join("private.zip"), "unchanged").unwrap();
    let handle = spawn(data.clone()).await.unwrap();
    for path in [
        sibling.clone(),
        sibling.join("private.zip"),
        data.join("../data-other/private.zip"),
        data.join(".."),
    ] {
        assert!(matches!(
            handle.resolve_reveal_path(path).await,
            Err(SupportError::RevealOutsideDataDirectory)
        ));
    }
    assert_eq!(
        std::fs::read_to_string(sibling.join("private.zip")).unwrap(),
        "unchanged"
    );
    handle.shutdown().await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn reveal_resolves_target_and_data_root_symlinks_before_authorization() {
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("data");
    let outside = directory.path().join("outside");
    let alias = directory.path().join("data-alias");
    std::fs::create_dir(&data).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(data.join("inside.zip"), "inside").unwrap();
    std::fs::write(outside.join("private.zip"), "outside").unwrap();
    std::os::unix::fs::symlink(&data, &alias).unwrap();
    std::os::unix::fs::symlink(data.join("inside.zip"), data.join("internal-link")).unwrap();
    std::os::unix::fs::symlink(&outside, data.join("external-directory-link")).unwrap();
    std::os::unix::fs::symlink(outside.join("private.zip"), data.join("external-file-link"))
        .unwrap();
    let handle = spawn(alias.clone()).await.unwrap();
    assert_eq!(
        handle
            .resolve_reveal_path(alias.join("internal-link"))
            .await
            .unwrap(),
        std::fs::canonicalize(data.join("inside.zip")).unwrap()
    );
    for path in [
        alias.join("external-directory-link"),
        alias.join("external-directory-link/private.zip"),
        alias.join("external-file-link"),
    ] {
        assert!(matches!(
            handle.resolve_reveal_path(path).await,
            Err(SupportError::RevealOutsideDataDirectory)
        ));
    }
    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn app_core_reveal_uses_the_actual_support_data_directory() {
    let settings = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let (core, _events) = crate::spawn_with_config(
        settings.path(),
        crate::AppConfig {
            cache_directory: Some(data.path().to_owned()),
            ..crate::AppConfig::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        core.log_directory().await.unwrap(),
        data.path().join("logs")
    );
    assert!(!data.path().join("logs").exists());
    let file = data.path().join("diagnostic.zip");
    std::fs::write(&file, "example").unwrap();
    assert_eq!(
        core.resolve_reveal_path(file.clone()).await.unwrap(),
        std::fs::canonicalize(file).unwrap()
    );
    assert!(matches!(
        core.resolve_reveal_path(settings.path().to_owned()).await,
        Err(crate::AppError::Support(
            SupportError::RevealOutsideDataDirectory
        ))
    ));
    core.shutdown().await.unwrap();
}

#[tokio::test]
async fn full_logging_expires_after_thirty_minutes_using_virtual_time() {
    let directory = tempfile::tempdir().unwrap();
    let handle = spawn(directory.path().to_owned()).await.unwrap();
    let started = handle.start_full_log().await.unwrap();
    assert!(started.enabled);
    assert_eq!(started.remaining_seconds, 1800);
    // Pause only time, never wait for a filesystem operation under a timeout.
    capture(&handle, || tracing::trace!("trace-before-expiration"));
    handle.flush().await.unwrap();
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(1799)).await;
    assert_eq!(handle.full_log_status().await.unwrap().remaining_seconds, 1);
    tokio::time::advance(Duration::from_secs(1)).await;
    assert_eq!(
        handle.full_log_status().await.unwrap(),
        FullLogStatus::default()
    );
    tokio::time::resume();
    capture(&handle, || {
        tracing::trace!("trace-after-expiration");
        tracing::info!("info-after-expiration");
    });
    let logs = contents(&handle, directory.path()).await;
    assert!(logs.contains("trace-before-expiration"));
    assert!(!logs.contains("trace-after-expiration"));
    assert!(logs.contains("info-after-expiration"));
    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn stopping_full_logging_immediately_restores_info_and_restart_renews_deadline() {
    let directory = tempfile::tempdir().unwrap();
    let handle = spawn(directory.path().to_owned()).await.unwrap();
    handle.start_full_log().await.unwrap();
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(600)).await;
    assert_eq!(
        handle.start_full_log().await.unwrap().remaining_seconds,
        1800
    );
    tokio::time::resume();
    capture(&handle, || tracing::debug!("debug-before-stop"));
    assert_eq!(
        handle.stop_full_log().await.unwrap(),
        FullLogStatus::default()
    );
    capture(&handle, || tracing::debug!("debug-after-stop"));
    let logs = contents(&handle, directory.path()).await;
    assert!(logs.contains("debug-before-stop"));
    assert!(!logs.contains("debug-after-stop"));
    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn expiration_uses_capture_time_for_queued_trace_records() {
    let directory = tempfile::tempdir().unwrap();
    let handle = spawn(directory.path().to_owned()).await.unwrap();
    tokio::time::pause();
    handle.start_full_log().await.unwrap();
    let before_expiration = Instant::now();
    tokio::time::advance(FULL_LOG_DURATION).await;
    assert!(!handle.full_log_status().await.unwrap().enabled);
    let after_expiration = Instant::now();
    tokio::time::resume();
    // Inject delayed delivery deterministically rather than making a disk slow.
    for (at, text) in [
        (before_expiration, "accepted-before-deadline\n"),
        (after_expiration, "rejected-at-deadline\n"),
    ] {
        handle
            .send(Message::Record {
                level: tracing::Level::TRACE,
                at,
                text: text.into(),
            })
            .await
            .unwrap();
    }
    let logs = contents(&handle, directory.path()).await;
    assert!(logs.contains("accepted-before-deadline"));
    assert!(!logs.contains("rejected-at-deadline"));
    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn rolling_files_and_oversized_record_are_bounded() {
    let directory = tempfile::tempdir().unwrap();
    let handle = spawn_with_limits(
        directory.path().to_owned(),
        rolling::Limits {
            file_bytes: 128,
            files: 3,
        },
    );
    for index in 0..12 {
        capture(&handle, || tracing::info!(index, "rotation-visible"));
        handle.flush().await.unwrap();
    }
    let logs = std::fs::read_dir(directory.path().join("logs"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(logs.len(), 3);
    assert!(
        logs.iter()
            .all(|path| path.metadata().unwrap().len() <= 128)
    );
    let giant = "字".repeat(MAX_RECORD_BYTES);
    capture(&handle, || tracing::info!(%giant));
    handle.flush().await.unwrap();
    assert!(
        logs.iter()
            .all(|path| path.metadata().unwrap().len() <= 128)
    );
    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn startup_enforces_cap_on_existing_oversized_logs() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("logs")).unwrap();
    for name in ["app.log", "app.log.1", "app.log.2"] {
        std::fs::write(directory.path().join("logs").join(name), vec![b'x'; 4096]).unwrap();
    }
    let handle = spawn_with_limits(
        directory.path().to_owned(),
        rolling::Limits {
            file_bytes: 128,
            files: 3,
        },
    );
    capture(&handle, || tracing::info!("first-new-record"));
    handle.flush().await.unwrap();
    for entry in std::fs::read_dir(directory.path().join("logs")).unwrap() {
        assert!(entry.unwrap().metadata().unwrap().len() <= 128);
    }
    assert!(
        contents(&handle, directory.path())
            .await
            .contains("first-new-record")
    );
    handle.shutdown().await.unwrap();
}

#[tokio::test]
async fn export_includes_sanitized_settings_engine_system_logs_and_never_reads_sgf() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("private-game.sgf"),
        "(;PB[Private player]C[Private notes];B[aa])",
    )
    .unwrap();
    std::fs::write(
        directory.path().join("settings.json"),
        "original-settings-must-not-change",
    )
    .unwrap();
    std::fs::write(
        directory.path().join("analysis-cache.sqlite3"),
        b"original-cache",
    )
    .unwrap();
    let original = std::fs::read(directory.path().join("settings.json")).unwrap();
    let handle = spawn(directory.path().to_owned()).await.unwrap();
    let settings: Settings = serde_json::from_value(json!({
        "engine": {"program": "/Users/Alice/katago", "model": "C:\\Users\\Alice\\net.bin.gz", "config": "/home/alice/analysis.cfg", "extra_args": ["--password", "hunter2"]},
        "recent_files": ["/Users/Alice/private-game.sgf"],
        "fox_history": [{"uid": "12345678", "nickname": "Private player"}],
    })).unwrap();
    let diagnostics: EngineDiagnostics = serde_json::from_value(json!({
        "profile": settings.engine,
        "checks": [{"kind": "ProgramExists", "status": "Error", "path": "/Users/Alice/katago", "detail": "Authorization: Bearer private-token", "repairs": []}]
    })).unwrap();
    capture(&handle, || {
        tracing::info!("safe lifecycle event");
        tracing::info!("account=PrivatePlayer token=private-token");
        tracing::warn!("failed opening /Users/Alice/private-game.sgf");
        tracing::debug!("disabled-debug");
    });
    let path = handle
        .export_bundle(settings, diagnostics, "0.1.0-test".into())
        .await
        .unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
    let mut entries = BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        let mut text = String::new();
        entry.read_to_string(&mut text).unwrap();
        entries.insert(entry.name().unwrap().into_owned(), text);
    }
    assert_eq!(
        entries.keys().cloned().collect::<Vec<_>>(),
        vec![
            "engine.json",
            "logs/app.log",
            "settings.json",
            "system.json"
        ]
    );
    assert!(entries["logs/app.log"].contains("safe lifecycle event"));
    let all = entries.values().cloned().collect::<String>();
    for sensitive in [
        "Alice",
        "alice",
        "hunter2",
        "12345678",
        "Private player",
        "PrivatePlayer",
        "private-token",
        "private-game",
        "Private notes",
        "disabled-debug",
    ] {
        assert!(!all.contains(sensitive), "export leaked {sensitive}");
    }
    let system: serde_json::Value = serde_json::from_str(&entries["system.json"]).unwrap();
    assert_eq!(system["version"], "0.1.0-test");
    assert_eq!(system["os"], std::env::consts::OS);
    assert_eq!(
        std::fs::read(directory.path().join("settings.json")).unwrap(),
        original
    );
    assert_eq!(
        std::fs::read(directory.path().join("analysis-cache.sqlite3")).unwrap(),
        b"original-cache"
    );
    assert!(
        std::fs::read_to_string(directory.path().join("private-game.sgf"))
            .unwrap()
            .contains("Private notes")
    );
    handle.shutdown().await.unwrap();
}

#[test]
fn java_privacy_categories_and_encoded_credentials_fail_closed() {
    for input in [
        "password=contains spaces and\ncontinued credential",
        "Authorization: Custom scheme with spaces",
        "Cookie: a=b",
        "--api-key private-key",
        "--pw secret-value",
        "username=Alice",
        "player=Alice",
        "roomId=452",
        "live-room:452",
        "unite-board:abc",
        "/mnt/c/Users/Alice/config",
        "/home/Alice/config",
        "../private/config",
        r"C:\Users\Alice\config",
        r"\\server\share\private",
        "https://user:pass@example.test/private",
        "token%3dprivate-token",
        "%2574oken%253dprivate-token",
        r"\u0074oken\u003dprivate-token",
        "(;PB[Alice]\n;B[aa])",
    ] {
        let safe = sanitize::text(input);
        assert!(
            safe.starts_with("<redact"),
            "did not redact {input}: {safe}"
        );
        assert!(!safe.contains("Alice"));
        assert!(!safe.contains("private-token"));
    }
    assert_eq!(
        sanitize::text("KataGo 1.18.2 ready\nvisits=35\n"),
        "KataGo 1.18.2 ready\nvisits=35\n"
    );
    let safe = sanitize::json(json!({
        "nested": {"password": "two words", "api_key": "private-key", "uid": 12345},
        "arguments": ["--cookie", "raw-cookie"],
        "/Users/Alice/key": "value",
        "tagged": {"privacy": "secret", "value": "sensitive-value"},
        "unknown": {"privacy": "new-privacy", "value": "future-secret"},
        "enabled": true,
    }));
    let encoded = serde_json::to_string(&safe).unwrap();
    for sensitive in [
        "two words",
        "private-key",
        "12345",
        "raw-cookie",
        "Alice",
        "sensitive-value",
        "future-secret",
    ] {
        assert!(!encoded.contains(sensitive));
    }
    assert_eq!(safe["enabled"], true);
}

#[test]
fn export_sanitizes_framed_log_records_independently_and_unframed_text_as_a_unit() {
    let framed = "1 INFO app_core message=useful-before\n2 WARN app_core password=hidden\n3 INFO app_core message=useful-after\n";
    let safe = sanitize::log_file(framed);
    assert!(safe.contains("useful-before"));
    assert!(safe.contains("useful-after"));
    assert!(!safe.contains("hidden"));
    let multiline = "1 INFO app_core password=\"first-part\ncontinuation-of-credential\"\n2 INFO app_core message=safe\n";
    let safe = sanitize::log_file(multiline);
    assert!(!safe.contains("first-part"));
    assert!(!safe.contains("continuation-of-credential"));
}

#[cfg(unix)]
#[tokio::test]
async fn support_symlinks_are_not_followed() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), directory.path().join("logs")).unwrap();
    let handle = spawn(directory.path().to_owned()).await.unwrap();
    capture(&handle, || tracing::info!("must not follow symlink"));
    assert!(handle.flush().await.is_err());
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
    assert!(handle.shutdown().await.is_err());

    let second = tempfile::tempdir().unwrap();
    std::fs::create_dir(second.path().join("logs")).unwrap();
    let target = outside.path().join("important.txt");
    std::fs::write(&target, "unchanged").unwrap();
    std::os::unix::fs::symlink(&target, second.path().join("logs/app.log")).unwrap();
    let handle = spawn(second.path().to_owned()).await.unwrap();
    capture(&handle, || tracing::info!("must not follow file symlink"));
    assert!(handle.flush().await.is_err());
    assert_eq!(std::fs::read_to_string(target).unwrap(), "unchanged");
    assert!(handle.shutdown().await.is_err());
}
