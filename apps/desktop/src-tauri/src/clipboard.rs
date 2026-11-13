//! Native clipboard access stays in Rust and never runs on the UI thread.
use app_core::{Clipboard, ClipboardError, ClipboardFuture, ClipboardImage};
use tauri::Runtime;
use tauri_plugin_clipboard_manager::ClipboardExt;

pub(crate) struct NativeClipboard<R: Runtime>(pub tauri::AppHandle<R>);

impl<R: Runtime> std::fmt::Debug for NativeClipboard<R> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("NativeClipboard")
    }
}

impl<R: Runtime> Clipboard for NativeClipboard<R> {
    fn read_text(&self) -> ClipboardFuture<'_, Option<String>> {
        let app = self.0.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                app.clipboard()
                    .read_text()
                    .map(Some)
                    .map_err(platform_error)
            })
            .await
            .map_err(|error| ClipboardError::Platform(error.to_string()))?
        })
    }

    fn write_text(&self, text: String) -> ClipboardFuture<'_, ()> {
        let app = self.0.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                app.clipboard().write_text(text).map_err(platform_error)
            })
            .await
            .map_err(|error| ClipboardError::Platform(error.to_string()))?
        })
    }

    fn write_image(&self, image: ClipboardImage) -> ClipboardFuture<'_, ()> {
        let app = self.0.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let image = tauri::image::Image::new_owned(image.rgba, image.width, image.height);
                app.clipboard().write_image(&image).map_err(platform_error)
            })
            .await
            .map_err(|error| ClipboardError::Platform(error.to_string()))?
        })
    }
}

fn platform_error(error: tauri_plugin_clipboard_manager::Error) -> ClipboardError {
    // The official plugin exposes native read failures as text. Keep them
    // typed as platform failures instead of guessing from localized messages.
    ClipboardError::Platform(error.to_string())
}
