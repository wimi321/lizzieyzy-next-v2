use tauri::{DragDropEvent, Manager, Runtime, Window, WindowEvent};

/// Native drops never cross IPC or grant the webview filesystem permissions.
pub(crate) fn on_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if window.label() != "main" {
        return;
    }
    let WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) = event else {
        return;
    };
    crate::system_open::enqueue(window.app_handle(), paths.clone());
}
