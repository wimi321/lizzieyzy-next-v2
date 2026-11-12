#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{AppCore, AppEvent, AppSubscription, DocumentView, NewGameOptions, Settings};
use std::{future::Future, time::Duration};

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("safety deadline")
}

async fn document(events: &mut AppSubscription) -> DocumentView {
    within(async {
        loop {
            if let AppEvent::DocumentChanged { view, .. } = events.recv().await.unwrap() {
                return view;
            }
        }
    })
    .await
}

async fn start(directory: &std::path::Path) -> (AppCore, AppSubscription) {
    let (app, mut events) = within(app_core::spawn(directory)).await.unwrap();
    document(&mut events).await;
    (app, events)
}

#[tokio::test]
async fn recent_files_include_open_drop_and_save_deduplicate_limit_and_persist() {
    let dir = tempfile::tempdir().unwrap();
    let (app, mut events) = start(dir.path()).await;
    assert!(within(app.recent_files()).await.unwrap().is_empty());
    let paths: Vec<_> = (0..7)
        .map(|i| dir.path().join(format!("game-{i}.sgf")))
        .collect();
    for path in &paths {
        std::fs::write(path, "(;SZ[9];B[aa])").unwrap();
    }
    for path in &paths[..5] {
        within(app.open_path(path)).await.unwrap();
        document(&mut events).await;
    }
    within(app.open_dropped_paths(vec![paths[5].clone(), paths[6].clone()]))
        .await
        .unwrap();
    document(&mut events).await;
    let to_strings = |indices: &[usize]| {
        indices
            .iter()
            .map(|i| paths[*i].to_string_lossy().into_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        within(app.recent_files()).await.unwrap(),
        to_strings(&[5, 4, 3, 2, 1])
    );
    within(app.open_path(&paths[3])).await.unwrap();
    document(&mut events).await;
    assert_eq!(
        within(app.recent_files()).await.unwrap(),
        to_strings(&[3, 5, 4, 2, 1])
    );
    within(app.save(&paths[6])).await.unwrap();
    document(&mut events).await;
    let expected = to_strings(&[6, 3, 5, 4, 2]);
    assert_eq!(within(app.recent_files()).await.unwrap(), expected);
    // Bytes are the route used by downloaded Fox games, and carry no file path.
    within(app.open_bytes(b"(;AP[foxwq]KM[375]RU[chinese])".to_vec()))
        .await
        .unwrap();
    let downloaded = document(&mut events).await;
    assert!(downloaded.file_path.is_none());
    assert!(downloaded.file_name.is_none());
    assert!(!downloaded.modified);
    within(app.new_game(NewGameOptions::default()))
        .await
        .unwrap();
    document(&mut events).await;
    assert_eq!(within(app.recent_files()).await.unwrap(), expected);
    within(app.shutdown()).await.unwrap();
    assert_eq!(
        within(Settings::load(dir.path()))
            .await
            .unwrap()
            .recent_files,
        expected
    );
    let (app, _) = start(dir.path()).await;
    assert_eq!(within(app.recent_files()).await.unwrap(), expected);
    within(app.clear_recent_files()).await.unwrap();
    assert!(within(app.recent_files()).await.unwrap().is_empty());
    within(app.clear_recent_files()).await.unwrap();
    within(app.shutdown()).await.unwrap();
    assert!(
        within(Settings::load(dir.path()))
            .await
            .unwrap()
            .recent_files
            .is_empty()
    );
}

#[tokio::test]
async fn missing_or_invalid_recent_files_are_removed_and_publish_open_failed() {
    for broken_content in [None, Some("not SGF")] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("recent.sgf");
        std::fs::write(&path, "(;C[keep];B[aa])").unwrap();
        let (app, mut events) = start(dir.path()).await;
        within(app.open_path(&path)).await.unwrap();
        let original = document(&mut events).await;
        match broken_content {
            Some(text) => std::fs::write(&path, text).unwrap(),
            None => std::fs::remove_file(&path).unwrap(),
        }
        assert!(within(app.open_path(&path)).await.is_err());
        let (failed_path, reason) = within(async {
            loop {
                if let AppEvent::OpenFailed { path, reason } = events.recv().await.unwrap() {
                    return (path, reason);
                }
            }
        })
        .await;
        assert_eq!(failed_path, path.to_string_lossy());
        assert!(!reason.is_empty());
        assert!(within(app.recent_files()).await.unwrap().is_empty());
        assert!(
            within(Settings::load(dir.path()))
                .await
                .unwrap()
                .recent_files
                .is_empty()
        );
        within(app.step(1)).await.unwrap();
        let preserved = document(&mut events).await;
        assert_eq!(preserved.game_info, original.game_info);
        assert_eq!(preserved.file_path, original.file_path);
        assert!(!preserved.modified);
        within(app.shutdown()).await.unwrap();
    }
}

#[tokio::test]
async fn legacy_settings_default_recent_files_and_loaded_lists_are_normalized() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, r#"{"version":1}"#).unwrap();
    assert!(
        within(Settings::load(dir.path()))
            .await
            .unwrap()
            .recent_files
            .is_empty()
    );
    std::fs::write(
        &path,
        r#"{"version":1,"recent_files":["a","b","a","","c","d","e","f"]}"#,
    )
    .unwrap();
    assert_eq!(
        within(Settings::load(dir.path()))
            .await
            .unwrap()
            .recent_files,
        ["a", "b", "c", "d", "e"]
    );
}

#[tokio::test]
async fn recent_settings_failures_preserve_failed_open_and_report_partial_save() {
    let dir = tempfile::tempdir().unwrap();
    let settings_dir = dir.path().join("settings");
    let (app, mut events) = start(&settings_dir).await;
    within(app.set_comment("unsaved".into())).await.unwrap();
    let before = document(&mut events).await;
    // A regular file cannot become the settings directory on any supported OS.
    std::fs::write(&settings_dir, "blocked").unwrap();
    let source = dir.path().join("source.sgf");
    std::fs::write(&source, "(;C[replace])").unwrap();
    assert!(within(app.open_path(&source)).await.is_err());
    let target = dir.path().join("save.sgf");
    assert!(matches!(
        within(app.save(&target)).await,
        Err(app_core::AppError::RecentFilesAfterSave { .. })
    ));
    // The SGF write succeeded: publish that true baseline even when recent
    // history persistence fails, and return a typed partial-success error.
    assert!(target.is_file());
    let saved = document(&mut events).await;
    assert_eq!(saved.file_path.as_deref(), target.to_str());
    assert_eq!(saved.file_name.as_deref(), Some("save.sgf"));
    assert!(!saved.modified);
    assert_eq!(saved.comment, before.comment);
    assert!(within(app.recent_files()).await.unwrap().is_empty());
    within(app.set_comment("another".into())).await.unwrap();
    let after = document(&mut events).await;
    assert_eq!(after.file_path, saved.file_path);
    assert_eq!(after.file_name, saved.file_name);
    assert!(after.modified);
    within(app.shutdown()).await.unwrap();
}
