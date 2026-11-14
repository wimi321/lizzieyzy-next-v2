#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;
mod everyday_support;

use app_core::{
    AppConfig, AppCore, AppSubscription, Clipboard, ClipboardError, ClipboardFuture, ClipboardImage,
};
use common::within;
use everyday_support::{MemoryClipboard, png};
use futures_util::FutureExt;
use std::{path::Path, sync::Arc};

async fn start(
    directory: &Path,
    clipboard: Option<Arc<dyn Clipboard>>,
) -> (AppCore, AppSubscription) {
    within(app_core::spawn_with_config(
        directory,
        AppConfig {
            clipboard,
            ..AppConfig::default()
        },
    ))
    .await
    .unwrap()
}

#[tokio::test]
async fn clipboard_link_accepts_http_https_and_trims_paired_quotes() {
    let directory = tempfile::tempdir().unwrap();
    let clipboard = MemoryClipboard::new();
    let (app, _) = start(directory.path(), Some(Arc::new(clipboard.clone()))).await;
    for (text, expected) in [
        ("http://example.invalid", "http://example.invalid"),
        (
            "https://example.invalid/game.sgf?chessid=123#move-42",
            "https://example.invalid/game.sgf?chessid=123#move-42",
        ),
        (
            " \t\"https://example.invalid/game.sgf\"\r\n",
            "https://example.invalid/game.sgf",
        ),
        (
            " \n'  http://example.invalid/game.sgf \t'  ",
            "http://example.invalid/game.sgf",
        ),
        (
            "\u{3000}https://example.invalid/棋谱\u{a0}",
            "https://example.invalid/棋谱",
        ),
        (
            "HTTPS://Example.Invalid:443/A%2fb?signature=a%2Bb#Section",
            "HTTPS://Example.Invalid:443/A%2fb?signature=a%2Bb#Section",
        ),
        (
            "http://127.0.0.1:8080/game.sgf",
            "http://127.0.0.1:8080/game.sgf",
        ),
        ("https://[::1]/game.sgf", "https://[::1]/game.sgf"),
        (
            "https://example.invalid/path@part?email=a@b.invalid",
            "https://example.invalid/path@part?email=a@b.invalid",
        ),
        (
            "https://example.invalid/a%20b.sgf",
            "https://example.invalid/a%20b.sgf",
        ),
    ] {
        within(clipboard.set(Some(text.into()))).await;
        assert_eq!(
            within(app.clipboard_link()).await.unwrap().as_deref(),
            Some(expected),
            "input: {text:?}"
        );
        assert_eq!(
            within(clipboard.contents()).await.text.as_deref(),
            Some(text)
        );
    }
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn clipboard_link_rejects_non_links_credentials_and_ambiguous_input() {
    let directory = tempfile::tempdir().unwrap();
    let clipboard = MemoryClipboard::new();
    let (app, _) = start(directory.path(), Some(Arc::new(clipboard.clone()))).await;
    for text in [
        "",
        " \r\n\t ",
        "ordinary text",
        "打开 https://example.invalid/game.sgf",
        "ftp://example.invalid/game.sgf",
        "file:///tmp/game.sgf",
        "javascript:alert(1)",
        "data:text/plain,https://example.invalid",
        "//example.invalid/game.sgf",
        "https:example.invalid",
        "https:/example.invalid",
        "https:///example.invalid",
        "https://",
        "https://?game.sgf",
        "https://#game.sgf",
        "https://:443/game.sgf",
        "https://[invalid]/game.sgf",
        "https://example.invalid:99999/game.sgf",
        "https://user@example.invalid/game.sgf",
        "https://user:password@example.invalid/game.sgf",
        "https://:password@example.invalid/game.sgf",
        "https://@example.invalid/game.sgf",
        "https://:@example.invalid/game.sgf",
        "https://user%40name@example.invalid/game.sgf",
        "https://example.invalid/a b.sgf",
        "https://example.invalid/a\tb.sgf",
        "https://example.invalid/a\nb.sgf",
        "https://example.invalid/a\rb.sgf",
        "https://example.invalid/a\u{a0}b.sgf",
        "https://example.invalid/a\u{3000}b.sgf",
        "https://example.invalid/a\0b.sgf",
        "https://example.invalid/a\u{7f}b.sgf",
        "https://example.invalid/one.sgf\nhttps://example.invalid/two.sgf",
        "https://example.invalid\\other/game.sgf",
        "\"https://example.invalid/game.sgf'",
        "\"https://example.invalid/game.sgf",
        "https://example.invalid/game.sgf\"",
        "'https://example.invalid/game.sgf",
        "https://example.invalid/game.sgf'",
        "\"\"",
        "''",
    ] {
        within(clipboard.set(Some(text.into()))).await;
        assert_eq!(
            within(app.clipboard_link()).await.unwrap(),
            None,
            "input: {text:?}"
        );
    }
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn clipboard_link_limits_the_trimmed_url_to_4096_utf8_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let clipboard = MemoryClipboard::new();
    let (app, _) = start(directory.path(), Some(Arc::new(clipboard.clone()))).await;
    let prefix = "https://example.invalid/";
    let ascii = format!("{prefix}{}", "a".repeat(4096 - prefix.len()));
    let remaining = 4096 - prefix.len();
    let unicode = format!(
        "{prefix}{}{}",
        "棋".repeat(remaining / 3),
        "a".repeat(remaining % 3)
    );
    assert_eq!(ascii.len(), 4096);
    assert_eq!(unicode.len(), 4096);
    for url in [ascii, unicode] {
        for text in [url.clone(), format!(" \t\" {url} \"\n")] {
            within(clipboard.set(Some(text))).await;
            assert_eq!(
                within(app.clipboard_link()).await.unwrap(),
                Some(url.clone())
            );
        }
        within(clipboard.set(Some(format!("{url}a")))).await;
        assert_eq!(within(app.clipboard_link()).await.unwrap(), None);
    }
    within(app.shutdown()).await.unwrap();
}

#[derive(Debug)]
struct FailedClipboard(ClipboardError);

impl Clipboard for FailedClipboard {
    fn read_text(&self) -> ClipboardFuture<'_, Option<String>> {
        Box::pin(async { Err(self.0.clone()) })
    }

    fn write_text(&self, _text: String) -> ClipboardFuture<'_, ()> {
        panic!("clipboard_link must not write text")
    }

    fn write_image(&self, _image: ClipboardImage) -> ClipboardFuture<'_, ()> {
        panic!("clipboard_link must not write images")
    }
}

#[tokio::test]
async fn clipboard_link_returns_none_for_missing_text_or_unavailable_clipboard() {
    for error in [
        None,
        Some(ClipboardError::Unavailable),
        Some(ClipboardError::EmptyText),
        Some(ClipboardError::Platform("read failed".into())),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let clipboard = error.map(|error| Arc::new(FailedClipboard(error)) as Arc<dyn Clipboard>);
        let (app, _) = start(directory.path(), clipboard).await;
        assert_eq!(within(app.clipboard_link()).await.unwrap(), None);
        within(app.shutdown()).await.unwrap();
    }

    let directory = tempfile::tempdir().unwrap();
    let clipboard = MemoryClipboard::new();
    let (app, _) = start(directory.path(), Some(Arc::new(clipboard.clone()))).await;
    assert_eq!(within(app.clipboard_link()).await.unwrap(), None);
    within(app.copy_png(png())).await.unwrap();
    let image = within(clipboard.contents()).await.image.unwrap();
    assert_eq!(within(app.clipboard_link()).await.unwrap(), None);
    let after = within(clipboard.contents()).await;
    assert_eq!(after.text, None);
    assert_eq!(after.image, Some(image));
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn clipboard_link_query_preserves_document_history_settings_and_clipboard() {
    let directory = tempfile::tempdir().unwrap();
    let clipboard = MemoryClipboard::new();
    let (app, mut events) = start(directory.path(), Some(Arc::new(clipboard.clone()))).await;
    within(app.open_bytes(b"(;SZ[9]C[original];B[aa])".to_vec()))
        .await
        .unwrap();
    within(app.set_comment("edited".into())).await.unwrap();
    let revision = within(app.undo()).await.unwrap();
    let tree = within(app.variation_tree()).await.unwrap();
    let settings = within(app.settings()).await.unwrap();
    while matches!(events.recv().now_or_never(), Some(Some(_))) {}
    for text in [
        Some(" \"https://example.invalid/game.sgf\" "),
        Some("not a link"),
        None,
    ] {
        within(clipboard.set(text.map(str::to_owned))).await;
        let _ = within(app.clipboard_link()).await.unwrap();
        assert_eq!(within(app.step(0)).await.unwrap(), revision);
        assert_eq!(within(app.variation_tree()).await.unwrap(), tree);
        assert_eq!(within(app.settings()).await.unwrap(), settings);
        assert_eq!(within(clipboard.contents()).await.text.as_deref(), text);
        assert!(
            events.recv().now_or_never().is_none(),
            "query published an event"
        );
    }
    // The read-only query must also leave the existing redo entry intact.
    within(app.redo()).await.unwrap();
    within(app.copy_sgf()).await.unwrap();
    assert!(
        within(clipboard.contents())
            .await
            .text
            .unwrap()
            .contains("C[edited]")
    );
    within(app.shutdown()).await.unwrap();
}
