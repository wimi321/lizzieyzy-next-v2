//! Desktop adapters; application state and all business decisions live in AppCore.
pub mod bindings;
pub mod commands;
pub mod error;
pub mod events;
pub mod startup;

use app_core::{AppCore, AppSubscription};
use tauri::{Manager, Runtime};

pub struct DesktopState {
    pub core: AppCore,
    pub events: events::EventBridge,
}

impl DesktopState {
    pub fn new(core: AppCore, subscription: AppSubscription) -> Self {
        Self {
            core,
            events: events::EventBridge::new(subscription),
        }
    }
}

/// Shared command registration for the desktop and the Tauri mock runtime.
pub fn commands<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::document::open_bytes,
            commands::document::open_path,
            commands::document::save,
            commands::document::play,
            commands::document::step,
            commands::document::go_to,
            commands::document::undo,
            commands::document::redo,
            commands::analysis::set_pondering,
            commands::analysis::subscribe,
            commands::analysis::unsubscribe,
            commands::engine::configure_engine,
            commands::settings::settings,
        ])
}

fn context<R: Runtime>() -> tauri::Context<R> {
    tauri::generate_context!()
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let options = startup::StartupOptions::parse(std::env::args_os().skip(1))?;
    let app = commands(tauri::Builder::default())
        .setup(move |app| {
            let default_dir = match &options.config_dir {
                Some(directory) => directory.clone(),
                None => app.path().app_config_dir()?,
            };
            let (core, subscription) =
                tauri::async_runtime::block_on(startup::start(options, default_dir))?;
            app.manage(DesktopState::new(core, subscription));
            Ok(())
        })
        .build(context())?;
    app.run(|app, event| {
        if matches!(event, tauri::RunEvent::ExitRequested { .. }) {
            // Keep the runtime alive until AppCore has cancelled analysis and
            // reaped its child. Merely dropping managed state at exit is too late.
            let core = app.state::<DesktopState>().core.clone();
            if let Err(error) = tauri::async_runtime::block_on(core.shutdown()) {
                eprintln!("application shutdown: {error}");
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests;
