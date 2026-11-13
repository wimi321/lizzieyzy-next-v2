use super::{Harness, json, local_url, request};
use app_core::{Clipboard, ClipboardError, ClipboardFuture, ClipboardImage};
use std::sync::{Arc, Mutex};

mod png;

#[derive(Debug, Default)]
struct FakeClipboard {
    text: Mutex<Option<String>>,
    images: Mutex<Vec<ClipboardImage>>,
}

impl Clipboard for FakeClipboard {
    fn read_text(&self) -> ClipboardFuture<'_, Option<String>> {
        Box::pin(async { Ok(self.text.lock().unwrap().clone()) })
    }

    fn write_text(&self, text: String) -> ClipboardFuture<'_, ()> {
        Box::pin(async move {
            *self.text.lock().unwrap() = Some(text);
            Ok(())
        })
    }

    fn write_image(&self, image: ClipboardImage) -> ClipboardFuture<'_, ()> {
        Box::pin(async move {
            self.images.lock().unwrap().push(image);
            Ok(())
        })
    }
}

#[tokio::test]
async fn clipboard_and_clean_export_commands_use_the_injected_core_boundary() {
    let clipboard = Arc::new(FakeClipboard::default());
    let harness = Harness::with_config(app_core::AppConfig {
        clipboard: Some(clipboard.clone()),
        ..Default::default()
    })
    .await;
    harness
        .invoke(
            "open_bytes",
            json!({ "bytes": b"(;SZ[9]C[root];B[ab](;W[bc])(;W[cd]))".as_slice() }),
        )
        .await;
    harness.invoke("copy_sgf", json!({})).await;
    let copied = clipboard.text.lock().unwrap().clone().unwrap();
    assert!(copied.contains("B[ab]"));
    assert!(copied.contains("C[root]"));
    harness.invoke("paste_sgf", json!({})).await;
    harness.invoke("copy_sgf", json!({})).await;
    assert_eq!(
        clipboard.text.lock().unwrap().as_deref(),
        Some(copied.as_str())
    );
    let target = harness.directory.path().join("clean.sgf");
    harness
        .invoke(
            "save_clean_sgf",
            json!({ "path": target, "keepComments": false, "currentBranch": false }),
        )
        .await;
    let clean = std::fs::read_to_string(target).unwrap();
    assert!(!clean.contains("C[root]"));
    assert!(clean.contains("W[bc]"));
    assert!(clean.contains("W[cd]"));
    harness
        .invoke("transform_board", json!({ "transform": "RotateRight" }))
        .await;
    harness.invoke("copy_sgf", json!({})).await;
    assert_ne!(
        clipboard.text.lock().unwrap().as_deref(),
        Some(copied.as_str())
    );
    harness.invoke("undo", json!({})).await;
    harness.invoke("copy_sgf", json!({})).await;
    assert_eq!(
        clipboard.text.lock().unwrap().as_deref(),
        Some(copied.as_str())
    );
    harness.finish().await;
}

#[tokio::test]
async fn empty_clipboard_keeps_its_typed_ipc_error() {
    let clipboard = Arc::new(FakeClipboard::default());
    let harness = Harness::with_config(app_core::AppConfig {
        clipboard: Some(clipboard),
        ..Default::default()
    })
    .await;
    let error = request(&harness.main, "paste_sgf", json!({}), local_url())
        .await
        .unwrap_err();
    assert_eq!(
        error,
        json!({ "type": "Clipboard", "data": ClipboardError::EmptyText })
    );
    harness.finish().await;
}

#[tokio::test]
async fn everyday_commands_are_restricted_to_the_local_main_window() {
    let harness = Harness::new().await;
    let other =
        tauri::WebviewWindowBuilder::new(&harness.app, "everyday-other", Default::default())
            .build()
            .unwrap();
    for (window, url) in [
        (&other, local_url()),
        (&harness.main, "https://example.com/"),
    ] {
        for command in [
            "copy_sgf",
            "paste_sgf",
            "save_clean_sgf",
            "save_png",
            "copy_png",
            "transform_board",
            "always_on_top",
            "set_always_on_top",
            "ui_scale",
            "set_ui_scale",
            "play_sound",
            "set_play_sound",
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
    for command in [
        "plugin:clipboard-manager|read_text",
        "plugin:clipboard-manager|write_image",
    ] {
        let error = request(&harness.main, command, json!({}), local_url())
            .await
            .unwrap_err();
        assert!(
            error
                .as_str()
                .is_some_and(|text| text.contains("not allowed")),
            "{command}: {error}"
        );
    }
    harness.finish().await;
}
