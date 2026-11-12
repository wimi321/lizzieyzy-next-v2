use super::*;
use crate::{
    startup::{StartupOptions, start},
    system_open,
};

async fn startup_view(path: &Path, config: &Path) -> DocumentView {
    let options =
        StartupOptions::parse([std::ffi::OsString::from("--"), path.as_os_str().to_owned()])
            .unwrap();
    let (core, mut output) = within(start(options, config.to_owned())).await.unwrap();
    let view = within(async {
        loop {
            if let AppEvent::DocumentChanged { revision: 1, view } = output.recv().await.unwrap() {
                break view;
            }
        }
    })
    .await;
    within(core.shutdown()).await.unwrap();
    view
}

#[tokio::test]
async fn startup_and_second_instance_open_same_document_and_resolve_sender_cwd() {
    let mut harness = Harness::new().await;
    harness.subscribe(301).await;
    harness.document(301, 0).await;
    let path = harness.directory.path().join("棋谱 with space.SGF");
    std::fs::write(&path, "(;SZ[9]PB[black]PW[white]C[system opening];B[aa])").unwrap();
    let startup = startup_view(&path, &harness.directory.path().join("startup settings")).await;
    let unused_settings = harness.directory.path().join("must not replace settings");
    system_open::on_second_instance(
        harness.app.handle(),
        vec![
            "lizzieyzy-desktop".into(),
            format!("--config-dir={}", unused_settings.display()),
            "--open=棋谱 with space.SGF".into(),
        ],
        harness.directory.path().to_string_lossy().into_owned(),
    );
    let second = harness.document(301, 1).await;
    assert_eq!(second, startup);
    assert!(!unused_settings.exists());
    assert_eq!(
        harness.core.recent_files().await.unwrap(),
        vec![path.to_string_lossy()]
    );
    harness.finish().await;
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn opened_event_and_startup_use_same_open_path_and_emit_failures() {
    let mut harness = Harness::new().await;
    harness.subscribe(302).await;
    harness.document(302, 0).await;
    let path = harness.directory.path().join("棋谱 space %2F.SGF");
    std::fs::write(&path, "(;SZ[13]C[opened URL];B[aa])").unwrap();
    let startup = startup_view(&path, &harness.directory.path().join("startup settings")).await;
    system_open::on_run_event(
        harness.app.handle(),
        &tauri::RunEvent::Opened {
            urls: vec![
                "https://example.com/not-local.sgf".parse().unwrap(),
                tauri::Url::from_file_path(&path).unwrap(),
            ],
        },
    );
    assert_eq!(harness.document(302, 1).await, startup);
    let missing = harness.directory.path().join("missing.sgf");
    system_open::on_run_event(
        harness.app.handle(),
        &tauri::RunEvent::Opened {
            urls: vec![tauri::Url::from_file_path(&missing).unwrap()],
        },
    );
    within(async {
        loop {
            if let AppEvent::OpenFailed { path, reason } = harness.next().await.event {
                assert_eq!(path, missing.to_string_lossy());
                assert!(!reason.is_empty());
                break;
            }
        }
    })
    .await;
    // A failed event leaves the previous document and its revision intact.
    assert_eq!(harness.core.step(1).await.unwrap(), 2);
    assert_eq!(harness.document(302, 2).await.comment, None);
    harness.finish().await;
}

#[tokio::test]
async fn second_instance_failure_uses_open_failed_and_keeps_existing_document() {
    let mut harness = Harness::new().await;
    harness.subscribe(303).await;
    harness.document(303, 0).await;
    let missing = harness.directory.path().join("missing.gib");
    system_open::on_second_instance(
        harness.app.handle(),
        vec!["lizzieyzy-desktop".into(), "missing.gib".into()],
        harness.directory.path().to_string_lossy().into_owned(),
    );
    within(async {
        loop {
            if let AppEvent::OpenFailed { path, reason } = harness.next().await.event {
                assert_eq!(path, missing.to_string_lossy());
                assert!(!reason.is_empty());
                break;
            }
        }
    })
    .await;
    assert_eq!(harness.core.step(1).await.unwrap(), 0);
    harness.finish().await;
}

#[tokio::test]
async fn second_instance_accepts_local_file_urls_from_linux_launchers() {
    let mut harness = Harness::new().await;
    harness.subscribe(304).await;
    harness.document(304, 0).await;
    let path = harness.directory.path().join("game % space.sgf");
    std::fs::write(&path, "(;C[file URL argument])").unwrap();
    let url = tauri::Url::from_file_path(&path).unwrap();
    system_open::on_second_instance(
        harness.app.handle(),
        vec!["lizzieyzy-desktop".into(), url.to_string()],
        harness
            .directory
            .path()
            .join("different cwd")
            .to_string_lossy()
            .into_owned(),
    );
    let view = harness.document(304, 1).await;
    assert_eq!(view.comment.as_deref(), Some("file URL argument"));
    assert_eq!(view.file_path.as_deref(), path.to_str());
    harness.finish().await;
}

#[tokio::test]
async fn gib_system_launch_imports_the_same_document_without_a_save_target() {
    let mut harness = Harness::new().await;
    harness.subscribe(305).await;
    harness.document(305, 0).await;
    let path = harness.directory.path().join("import.GIB");
    std::fs::write(
        &path,
        include_bytes!("../../../../../fixtures/gib/utf8-even.gib"),
    )
    .unwrap();
    let startup = startup_view(&path, &harness.directory.path().join("startup settings")).await;
    system_open::on_second_instance(
        harness.app.handle(),
        vec![
            "lizzieyzy-desktop".into(),
            path.to_string_lossy().into_owned(),
        ],
        harness.directory.path().to_string_lossy().into_owned(),
    );
    let view = harness.document(305, 1).await;
    assert_eq!(view, startup);
    assert_eq!(view.game_info.black_name.as_deref(), Some("Black Player"));
    assert_eq!(view.file_path, None);
    assert_eq!(view.tree.main_line.len(), 5);
    harness.finish().await;
}

#[tokio::test]
async fn native_request_queued_before_core_is_ready_is_not_lost() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("early.sgf");
    std::fs::write(&path, "(;C[queued before setup])").unwrap();
    let (opener, incoming) = system_open::SystemOpener::channel();
    opener.enqueue(vec![path]);
    let (core, mut output) = within(app_core::spawn(directory.path())).await.unwrap();
    let worker = tokio::spawn(system_open::serve(core.clone(), incoming));
    within(async {
        loop {
            if let AppEvent::DocumentChanged { revision: 1, view } = output.recv().await.unwrap() {
                assert_eq!(view.comment.as_deref(), Some("queued before setup"));
                break;
            }
        }
    })
    .await;
    drop(opener);
    within(worker).await.unwrap();
    within(core.shutdown()).await.unwrap();
}
