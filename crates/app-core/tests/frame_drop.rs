#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{AppCore, AppEvent, AppSubscription, DocumentView};
use std::{future::Future, time::Duration};

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("frame operation exceeded safety deadline")
}

async fn document(events: &mut AppSubscription) -> (u64, DocumentView) {
    within(async {
        loop {
            if let AppEvent::DocumentChanged { revision, view } = events.recv().await.unwrap() {
                return (revision, view);
            }
        }
    })
    .await
}

async fn start(directory: &std::path::Path) -> (AppCore, AppSubscription) {
    let (core, mut events) = within(app_core::spawn(directory)).await.unwrap();
    assert_eq!(document(&mut events).await.0, 0);
    (core, events)
}

#[tokio::test]
async fn drop_opens_single_sgf_and_accepts_mixed_case_extension() {
    for name in ["game.sgf", "棋谱.SGF", "game.sGf"] {
        let directory = tempfile::tempdir().unwrap();
        let (core, mut events) = start(directory.path()).await;
        let path = directory.path().join(name);
        std::fs::write(&path, "(;SZ[9]C[dropped];B[aa])").unwrap();
        assert_eq!(
            within(core.open_dropped_paths(vec![path])).await.unwrap(),
            1
        );
        let (revision, view) = document(&mut events).await;
        assert_eq!(revision, 1);
        assert_eq!(view.file_name.as_deref(), Some(name));
        assert_eq!(view.comment.as_deref(), Some("dropped"));
        assert_eq!(view.board.unwrap().width, 9);
        within(core.shutdown()).await.unwrap();
    }
}

#[tokio::test]
async fn drop_skips_unsupported_and_failed_files_then_stops_at_first_success() {
    let directory = tempfile::tempdir().unwrap();
    let (core, mut events) = start(directory.path()).await;
    let paths: Vec<_> = [
        "ignored.txt",
        "missing.sgf",
        "broken.sgf",
        "first.SGF",
        "second.sgf",
    ]
    .map(|name| directory.path().join(name))
    .into();
    std::fs::write(&paths[0], "(;C[unsupported])").unwrap();
    std::fs::write(&paths[2], "not SGF").unwrap();
    std::fs::write(&paths[3], "(;C[first])").unwrap();
    std::fs::write(&paths[4], "(;C[second])").unwrap();
    assert_eq!(
        within(core.open_dropped_paths(paths.clone()))
            .await
            .unwrap(),
        1
    );
    let mut failure = None;
    let view = within(async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::OpenFailed { path, reason } => failure = Some((path, reason)),
                AppEvent::DocumentChanged { revision, view } => {
                    assert_eq!(revision, 1);
                    break view;
                }
                _ => {}
            }
        }
    })
    .await;
    let (path, reason) = failure.unwrap();
    assert_eq!(path, paths[2].to_string_lossy());
    assert!(!reason.is_empty());
    assert_eq!(view.comment.as_deref(), Some("first"));
    assert_eq!(view.file_name.as_deref(), Some("first.SGF"));
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn unsupported_or_empty_drop_is_noop_and_all_failed_preserves_document() {
    let directory = tempfile::tempdir().unwrap();
    let (core, mut events) = start(directory.path()).await;
    within(core.open_bytes(b"(;C[keep])".to_vec()))
        .await
        .unwrap();
    let (_, original) = document(&mut events).await;
    let unsupported = directory.path().join("valid.sgf.txt");
    std::fs::write(&unsupported, "(;C[replace])").unwrap();
    for paths in [
        vec![],
        vec![unsupported, directory.path().join("no-extension")],
    ] {
        assert_eq!(within(core.open_dropped_paths(paths)).await.unwrap(), 1);
    }
    let missing = directory.path().join("missing.SGF");
    assert_eq!(
        within(core.open_dropped_paths(vec![missing.clone()]))
            .await
            .unwrap(),
        1
    );
    within(core.shutdown()).await.unwrap();
    let mut failures = Vec::new();
    while let Some(event) = within(events.recv()).await {
        match event {
            AppEvent::OpenFailed { path, reason } => failures.push((path, reason)),
            AppEvent::DocumentChanged { view, .. } => assert_eq!(view, original),
            _ => {}
        }
    }
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].0, missing.to_string_lossy());
    assert!(!failures[0].1.is_empty());
}
