//! All native file entry points converge before entering the AppCore actor.

#[cfg(any(target_os = "windows", target_os = "linux", test))]
use std::ffi::OsString;
use std::path::PathBuf;

use app_core::{AppCore, AppError};
use tauri::{AppHandle, Manager, Runtime};
use tokio::sync::mpsc;

#[cfg(any(target_os = "windows", target_os = "linux", test))]
use crate::startup::StartupOptions;

/// Installed before plugins initialize, so an early second launch can queue
/// its request while the first instance is still loading its settings.
pub(crate) struct SystemOpener(mpsc::UnboundedSender<Vec<PathBuf>>);

impl SystemOpener {
    pub(crate) fn channel() -> (Self, mpsc::UnboundedReceiver<Vec<PathBuf>>) {
        let (send, receive) = mpsc::unbounded_channel();
        (Self(send), receive)
    }

    pub(crate) fn enqueue(&self, paths: Vec<PathBuf>) {
        if !paths.is_empty() && self.0.send(paths).is_err() {
            eprintln!("system file open: application is closed");
        }
    }
}

/// The same batch policy as native drops: ignore unsupported extensions and
/// open the first successful file. AppCore publishes OpenFailed for failures.
pub(crate) async fn open_paths(core: &AppCore, paths: Vec<PathBuf>) -> Result<(), AppError> {
    let paths = paths.into_iter().map(file_url_path).collect();
    core.open_dropped_paths(paths).await.map(|_| ())
}

// Linux launchers may pass file:// URIs for an Exec %U field. Only convert an
// explicit local file URL; ordinary native paths keep their original encoding.
fn file_url_path(path: PathBuf) -> PathBuf {
    path.to_str()
        .filter(|text| text.starts_with("file://"))
        .and_then(|text| tauri::Url::parse(text).ok())
        .and_then(|url| url.to_file_path().ok())
        .unwrap_or(path)
}

pub(crate) async fn serve(core: AppCore, mut incoming: mpsc::UnboundedReceiver<Vec<PathBuf>>) {
    while let Some(paths) = incoming.recv().await {
        if let Err(error) = open_paths(&core, paths).await {
            eprintln!("system file open: {error}");
        }
    }
}

pub(crate) fn enqueue<R: Runtime>(app: &AppHandle<R>, paths: Vec<PathBuf>) {
    app.state::<SystemOpener>().enqueue(paths);
}

pub(crate) fn on_run_event<R: Runtime>(app: &AppHandle<R>, event: &tauri::RunEvent) {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    if let tauri::RunEvent::Opened { urls } = event {
        // URL decoding handles spaces, Unicode and percent signs without
        // treating remote URLs as local filesystem paths.
        let paths = urls
            .iter()
            .filter_map(|url| url.to_file_path().ok())
            .collect();
        enqueue(app, paths);
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    let _ = (app, event);
}

/// The plugin includes argv[0]. Relative paths belong to the second process's
/// working directory; its config-dir must never replace the running settings.
#[cfg(any(target_os = "windows", target_os = "linux", test))]
pub(crate) fn on_second_instance<R: Runtime>(app: &AppHandle<R>, args: Vec<String>, cwd: String) {
    match StartupOptions::parse(args.into_iter().skip(1).map(OsString::from)) {
        Ok(options) => {
            if let Some(path) = options.document {
                let path = file_url_path(path);
                let path = if path.is_absolute() {
                    path
                } else {
                    PathBuf::from(cwd).join(path)
                };
                enqueue(app, vec![path]);
            }
        }
        Err(error) => eprintln!("second-instance arguments: {error}"),
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub(crate) fn single_instance<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    // macOS LaunchServices already delivers subsequent opens through Opened.
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(on_second_instance));
    builder
}
