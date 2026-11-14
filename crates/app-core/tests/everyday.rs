#![allow(clippy::unwrap_used, clippy::expect_used)]
mod everyday_support;

use app_core::{
    AppConfig, AppCore, AppError, AppEvent, AppSubscription, BoardTransform, ClipboardError,
    ClipboardImage, DocumentView, EngineProfile, EngineStatus, PngError, Settings,
};
use everyday_support::{MemoryClipboard, png};
use std::{sync::Arc, time::Duration};

async fn document(events: &mut AppSubscription) -> DocumentView {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let AppEvent::DocumentChanged { view, .. } = events.recv().await.unwrap() {
                return view;
            }
        }
    })
    .await
    .unwrap()
}
async fn start(
    directory: &std::path::Path,
    clipboard: &MemoryClipboard,
) -> (AppCore, AppSubscription) {
    app_core::spawn_with_config(
        directory,
        AppConfig {
            clipboard: Some(Arc::new(clipboard.clone())),
            ..AppConfig::default()
        },
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn clipboard_roundtrip_preserves_properties_branches_and_is_unsaved() {
    let directory = tempfile::tempdir().unwrap();
    let clipboard = MemoryClipboard::new();
    let (app, mut events) = start(directory.path(), &clipboard).await;
    let source = directory.path().join("source.sgf");
    let text = "(;FF[4]CA[UTF-8]SZ[9]PB[黑方]C[root]XX[unknown];B[ab](;W[cc]C[first])(;W[de]LB[ef:Label]))";
    std::fs::write(&source, text).unwrap();
    app.open_path(&source).await.unwrap();
    let original = document(&mut events).await;
    assert!(!original.modified);
    app.copy_sgf().await.unwrap();
    let copied = clipboard.contents().await.text.unwrap();
    assert_eq!(
        sgf::parse(copied.as_bytes()).unwrap().games,
        sgf::parse(text.as_bytes()).unwrap().games
    );
    app.paste_sgf().await.unwrap();
    let pasted = document(&mut events).await;
    assert!(pasted.modified);
    assert_eq!(pasted.file_name, None);
    assert_eq!(pasted.file_path, None);
    assert!(!pasted.can_undo);
    assert_eq!(pasted.board, original.board);
    app.copy_sgf().await.unwrap();
    assert_eq!(clipboard.contents().await.text.unwrap(), copied);
    let saved = directory.path().join("saved.sgf");
    app.save(&saved).await.unwrap();
    assert!(!document(&mut events).await.modified);
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn clipboard_invalid_or_empty_does_not_change_document_or_history() {
    let directory = tempfile::tempdir().unwrap();
    let clipboard = MemoryClipboard::new();
    let (app, mut events) = start(directory.path(), &clipboard).await;
    app.open_bytes(b"(;SZ[9];B[aa])".to_vec()).await.unwrap();
    app.set_comment("keep".into()).await.unwrap();
    let before = document(&mut events).await;
    let tree = app.variation_tree().await.unwrap();
    for text in [
        None,
        Some(String::new()),
        Some(" \n\t".into()),
        Some("not a game".into()),
    ] {
        let empty = text.as_ref().is_none_or(|text| text.trim().is_empty());
        clipboard.set(text).await;
        let error = app.paste_sgf().await.unwrap_err();
        if empty {
            assert!(matches!(
                error,
                AppError::Clipboard(ClipboardError::EmptyText)
            ));
        } else {
            assert!(matches!(error, AppError::Sgf(_)));
        }
        assert_eq!(app.variation_tree().await.unwrap(), tree);
        app.copy_sgf().await.unwrap();
        assert!(clipboard.contents().await.text.unwrap().contains("C[keep]"));
    }
    app.undo().await.unwrap();
    let after = document(&mut events).await;
    assert_eq!(after.board, before.board);
    assert!(after.can_redo);
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn clipboard_accepts_existing_gib_detection_and_preserves_saved_analysis() {
    let directory = tempfile::tempdir().unwrap();
    let clipboard = MemoryClipboard::new();
    let (app, _events) = start(directory.path(), &clipboard).await;
    clipboard
        .set(Some(
            String::from_utf8(include_bytes!("../../../fixtures/lz/java-analysis.sgf").to_vec())
                .unwrap(),
        ))
        .await;
    app.paste_sgf().await.unwrap();
    let saved = app.whole_game_result(0).await.unwrap().unwrap();
    assert_eq!(saved.source, app_core::AnalysisSource::Saved);
    app.copy_sgf().await.unwrap();
    let copied = clipboard.contents().await.text.unwrap();
    assert!(copied.contains("LZ["));
    clipboard
        .set(Some("\\HS\n\\HE\nINI 0 1 0\nSTO 0 1 1 3 3\n".into()))
        .await;
    app.paste_sgf().await.unwrap();
    assert_eq!(app.variation_tree().await.unwrap().nodes.len(), 2);
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn clean_exports_preserve_file_identity_and_saved_or_modified_state() {
    let directory = tempfile::tempdir().unwrap();
    let clipboard = MemoryClipboard::new();
    let (app, mut events) = start(directory.path(), &clipboard).await;
    let source = directory.path().join("original.sgf");
    std::fs::write(
        &source,
        b"(;SZ[9]LZ[broken]C[root];B[aa](;W[bb];B[cc])(;W[dd]C[branch];B[ee]))",
    )
    .unwrap();
    app.open_path(&source).await.unwrap();
    let original = document(&mut events).await;
    app.go_to(4).await.unwrap();
    let before = document(&mut events).await;
    for (comments, branch, name, expected) in [
        (
            false,
            false,
            "clean.sgf",
            "(;SZ[9]CA[UTF-8];B[aa](;W[bb];B[cc])(;W[dd];B[ee]))",
        ),
        (
            true,
            false,
            "comments.sgf",
            "(;SZ[9]C[root]CA[UTF-8];B[aa](;W[bb];B[cc])(;W[dd]C[branch];B[ee]))",
        ),
        (
            false,
            true,
            "branch.sgf",
            "(;SZ[9]CA[UTF-8];B[aa];W[dd];B[ee])",
        ),
    ] {
        let target = directory.path().join(name);
        app.save_clean_sgf(&target, comments, branch).await.unwrap();
        assert_eq!(
            std::fs::read_to_string(target).unwrap(),
            format!("{expected}\n")
        );
    }
    // A no-op navigation publishes no event; an actual navigation proves export
    // kept the original path and clean baseline, independent of export target.
    app.go_to(0).await.unwrap();
    let after = document(&mut events).await;
    assert_eq!(after.file_path, original.file_path);
    assert_eq!(after.file_name, original.file_name);
    assert!(!after.modified);
    assert_eq!(after.can_undo, before.can_undo);
    app.set_comment("changed".into()).await.unwrap();
    let changed = document(&mut events).await;
    app.save_clean_sgf(directory.path().join("modified.sgf"), false, false)
        .await
        .unwrap();
    app.step(1).await.unwrap();
    let exported = document(&mut events).await;
    assert!(exported.modified);
    assert_eq!(exported.file_path, changed.file_path);
    assert_eq!(exported.file_name, changed.file_name);
    assert_eq!(exported.can_undo, changed.can_undo);
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn png_export_replaces_atomically_and_fake_clipboard_receives_pixels() {
    let directory = tempfile::tempdir().unwrap();
    let clipboard = MemoryClipboard::new();
    let (app, _events) = start(directory.path(), &clipboard).await;
    let target = directory.path().join("board.png");
    let link = directory.path().join("previous-inode");
    std::fs::write(&target, b"old image").unwrap();
    std::fs::hard_link(&target, &link).unwrap();
    let bytes = png();
    app.save_png(&target, bytes.clone()).await.unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), bytes);
    assert_eq!(std::fs::read(&link).unwrap(), b"old image");
    app.copy_png(bytes.clone()).await.unwrap();
    assert_eq!(
        clipboard.contents().await.image.unwrap(),
        ClipboardImage {
            width: 1,
            height: 1,
            rgba: vec![12, 34, 56, 78]
        }
    );
    for invalid in [b"not PNG".to_vec(), vec![0; app_core::MAX_PNG_BYTES + 1]] {
        assert!(matches!(
            app.save_png(&target, invalid.clone()).await,
            Err(AppError::Png(_))
        ));
        assert!(matches!(app.copy_png(invalid).await, Err(AppError::Png(_))));
        assert_eq!(std::fs::read(&target).unwrap(), bytes);
        assert_eq!(
            clipboard.contents().await.image.unwrap().rgba,
            [12, 34, 56, 78]
        );
    }
    let blocked = directory.path().join("directory.png");
    std::fs::create_dir(&blocked).unwrap();
    std::fs::write(blocked.join("keep"), b"keep").unwrap();
    let before = std::fs::read_dir(directory.path()).unwrap().count();
    assert!(matches!(
        app.save_png(&blocked, bytes).await,
        Err(AppError::Io(_))
    ));
    assert_eq!(std::fs::read(blocked.join("keep")).unwrap(), b"keep");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), before);
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn headless_clipboard_is_explicitly_unavailable() {
    let directory = tempfile::tempdir().unwrap();
    let (app, _) = app_core::spawn(directory.path()).await.unwrap();
    assert!(matches!(
        app.copy_sgf().await,
        Err(AppError::Clipboard(ClipboardError::Unavailable))
    ));
    assert!(matches!(
        app.paste_sgf().await,
        Err(AppError::Clipboard(ClipboardError::Unavailable))
    ));
    assert!(matches!(
        app.save_png(
            directory.path().join("a.png"),
            vec![0; app_core::MAX_PNG_BYTES + 1]
        )
        .await,
        Err(AppError::Png(PngError::TooLarge))
    ));
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn pasted_setup_is_analyzable_and_transform_never_reuses_old_coordinates() {
    let directory = tempfile::tempdir().unwrap();
    let clipboard = MemoryClipboard::new();
    let program = fake_katago::binary_path().unwrap();
    let model = directory.path().join("fake-model");
    let config = directory.path().join("fake-config");
    std::fs::write(&model, b"model").unwrap();
    std::fs::write(&config, b"config").unwrap();
    Settings {
        engine: Some(EngineProfile {
            human_model: None,
            program,
            model,
            config,
            extra_args: vec![],
            canonical_model_name: None,
        }),
        cache_enabled: true,
        auto_quick_analyze_on_load: false,
        ..Settings::default()
    }
    .save(directory.path())
    .await
    .unwrap();
    let mut config = AppConfig {
        clipboard: Some(Arc::new(clipboard.clone())),
        ..AppConfig::default()
    };
    config.analysis.live.max_visits = 1;
    config.session.write_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(60);
    let (app, mut events) = app_core::spawn_with_config(directory.path(), config)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::EngineStatusChanged(EngineStatus::Ready { .. }) => break,
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("{reason}")
                }
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    clipboard
        .set(Some("(;SZ[9]AB[ab][bc]AW[fg]PL[W])".into()))
        .await;
    let revision = app.paste_sgf().await.unwrap();
    let pasted = document(&mut events).await;
    assert_eq!(
        pasted.board.as_ref().unwrap().to_play,
        app_core::ColorView::White
    );
    final_analysis(&mut events, revision).await;
    let before = app.whole_game_analysis(0).await.unwrap().unwrap();
    app.set_pondering(false).await.unwrap();
    let revision = app
        .transform_board(BoardTransform::RotateRight)
        .await
        .unwrap();
    let transformed = document(&mut events).await;
    assert!(transformed.graph.points.iter().all(|p| p.summary.is_none()));
    assert!(app.whole_game_result(0).await.unwrap().is_none());
    app.set_pondering(true).await.unwrap();
    final_analysis(&mut events, revision).await;
    let after = app.whole_game_analysis(0).await.unwrap().unwrap();
    assert_ne!(before.key, after.key);
    assert_ne!(before.response.move_infos, after.response.move_infos);
    app.shutdown().await.unwrap();
}

async fn final_analysis(events: &mut AppSubscription, revision: u64) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::AnalysisUpdated(view) => {
                    assert_eq!(view.revision, revision);
                    if !view.is_during_search {
                        assert!(view.visits > 0);
                        break;
                    }
                }
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("{reason}")
                }
                _ => {}
            }
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn transform_saved_candidates_and_undo_redo_preserve_source_and_clears() {
    let directory = tempfile::tempdir().unwrap();
    let clipboard = MemoryClipboard::new();
    let (app, mut events) = start(directory.path(), &clipboard).await;
    let source = include_bytes!("../../../fixtures/lz/java-analysis.sgf");
    // Move the first candidate off the centre so a coordinate change is visible.
    let source = String::from_utf8(source.to_vec())
        .unwrap()
        .replace("move C3", "move B3")
        .replace("pv C3", "pv B3");
    let mut collection = sgf::parse(source.as_bytes()).unwrap();
    collection.games[0].sequence[0]
        .properties
        .retain(|p| p.identifier != "LZ2");
    app.open_bytes(sgf::serialize(&collection).into_bytes())
        .await
        .unwrap();
    let original_doc = document(&mut events).await;
    let before = app.whole_game_result(0).await.unwrap().unwrap();
    app.transform_board(BoardTransform::RotateRight)
        .await
        .unwrap();
    assert!(document(&mut events).await.modified);
    let rotated = app.whole_game_result(0).await.unwrap().unwrap();
    assert_ne!(
        before.candidates[0].coordinate,
        rotated.candidates[0].coordinate
    );
    assert_eq!(rotated.source, app_core::AnalysisSource::Saved);
    app.undo().await.unwrap();
    let undone = document(&mut events).await;
    assert_eq!(undone.modified, original_doc.modified);
    assert_eq!(
        app.whole_game_result(0).await.unwrap().unwrap().candidates,
        before.candidates
    );
    app.redo().await.unwrap();
    assert_eq!(
        app.whole_game_result(0).await.unwrap().unwrap().candidates,
        rotated.candidates
    );
    app.clear_analysis(true).await.unwrap();
    app.transform_board(BoardTransform::FlipHorizontal)
        .await
        .unwrap();
    assert!(app.whole_game_result(0).await.unwrap().is_none());
    app.undo().await.unwrap();
    assert!(app.whole_game_result(0).await.unwrap().is_none());
    app.shutdown().await.unwrap();
}

#[tokio::test]
async fn pasted_moves_start_the_same_automatic_curve_as_file_open() {
    let directory = tempfile::tempdir().unwrap();
    let clipboard = MemoryClipboard::new();
    let program = fake_katago::binary_path().unwrap();
    let model = directory.path().join("fake-model");
    let config = directory.path().join("fake-config");
    std::fs::write(&model, b"model").unwrap();
    std::fs::write(&config, b"config").unwrap();
    Settings {
        engine: Some(EngineProfile {
            human_model: None,
            program,
            model,
            config,
            extra_args: vec![],
            canonical_model_name: None,
        }),
        pondering: false,
        auto_quick_analyze_on_load: true,
        cache_enabled: false,
        ..Settings::default()
    }
    .save(directory.path())
    .await
    .unwrap();
    let (app, mut events) = start(directory.path(), &clipboard).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::EngineStatusChanged(EngineStatus::Ready { .. }) => break,
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("{reason}")
                }
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    clipboard.set(Some("(;SZ[9];B[aa];W[bb])".into())).await;
    app.paste_sgf().await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::WholeGameProgress(progress) => {
                    assert_eq!(progress.source, app_core::WholeGameSource::AutomaticQuick);
                    if progress.status == app_core::WholeGameStatus::Completed {
                        assert_eq!((progress.completed, progress.total), (3, 3));
                        break;
                    }
                    if let app_core::WholeGameStatus::Error { reason } = progress.status {
                        panic!("{reason}");
                    }
                }
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("{reason}")
                }
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    assert!(!app.settings().await.unwrap().pondering);
    app.shutdown().await.unwrap();
}
