//! Flush native placements without blocking Tauri's UI event loop.
use std::sync::atomic::{AtomicU8, Ordering};
use tauri::{Manager, Runtime};

#[derive(Default)]
pub(crate) struct ShutdownState(AtomicU8);

pub(crate) fn on_run_event<R: Runtime>(app: &tauri::AppHandle<R>, event: &tauri::RunEvent) {
    let tauri::RunEvent::ExitRequested { api, code, .. } = event else {
        return;
    };
    let Some(state) = app.try_state::<ShutdownState>() else {
        return;
    };
    if state.0.load(Ordering::Acquire) == 2 {
        return;
    }
    api.prevent_exit();
    if state
        .0
        .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    let core = app.state::<crate::DesktopState>().core.clone();
    let windows = app
        .try_state::<crate::floating::FloatingWindows>()
        .map(|state| state.inner().clone());
    let app = app.clone();
    let code = code.unwrap_or(0);
    tauri::async_runtime::spawn(async move {
        if let Err(error) = core.stop_web_board().await {
            eprintln!("stop web board at shutdown: {error}");
        }
        if let Some(windows) = windows
            && let Err(error) = windows.apply(crate::floating::Action::Flush).await
        {
            eprintln!("save windows at shutdown: {error}");
        }
        if let Err(error) = core.shutdown().await {
            eprintln!("application shutdown: {error}");
        }
        app.state::<ShutdownState>().0.store(2, Ordering::Release);
        app.exit(code);
    });
}
