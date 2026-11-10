use crate::DesktopState;
use tauri::{DragDropEvent, Manager, Runtime, Window, WindowEvent};

/// Native drops never cross IPC or grant the webview filesystem permissions.
pub(crate) fn on_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if window.label() != "main" {
        return;
    }
    let WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) = event else {
        return;
    };
    let Some(state) = window.try_state::<DesktopState>() else {
        return;
    };
    let core = state.core.clone();
    let paths = paths.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = core.open_dropped_paths(paths).await {
            eprintln!("file drop: {error}");
        }
    });
}
