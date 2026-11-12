//! Desktop adapters; application state and all business decisions live in AppCore.
pub mod bindings;
pub mod board_sync;
pub mod commands;
mod drag_drop;
pub mod engine_setup;
pub mod error;
pub mod events;
pub mod fox_error;
pub mod startup;
mod system_open;

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
        .on_window_event(drag_drop::on_window_event)
        .invoke_handler(tauri::generate_handler![
            commands::document::open_bytes,
            commands::document::open_path,
            commands::document::save,
            commands::document::play,
            commands::document::step,
            commands::document::go_to,
            commands::document::undo,
            commands::document::redo,
            commands::edit::set_comment,
            commands::edit::delete_node,
            commands::edit::delete_branch,
            commands::edit::promote_to_main,
            commands::edit::new_game,
            commands::edit::set_game_info,
            commands::edit::recent_files,
            commands::edit::clear_recent_files,
            commands::setup_markup::set_setup_stone,
            commands::setup_markup::clear_setup,
            commands::setup_markup::set_first_player,
            commands::setup_markup::convert_to_start_position,
            commands::setup_markup::set_markup,
            commands::setup_markup::clear_markup,
            commands::setup_markup::toggle_to_play,
            commands::move_editing::play_colored,
            commands::move_editing::insert_move,
            commands::move_editing::switch_stone,
            commands::move_editing::delete_stone,
            commands::move_editing::move_stone,
            commands::move_editing::add_variation,
            commands::move_editing::stone_origin,
            commands::move_editing::start_trial,
            commands::move_editing::end_trial,
            commands::variation::variation_tree,
            commands::variation::preview_variation,
            commands::analysis::set_pondering,
            commands::analysis::analysis_state,
            commands::analysis::toggle_pondering,
            commands::settings::analysis_limits,
            commands::settings::set_analysis_limits,
            commands::analysis::subscribe,
            commands::analysis::unsubscribe,
            commands::benchmark::start_benchmark,
            commands::benchmark::cancel_benchmark,
            commands::benchmark::benchmark_status,
            commands::benchmark::reset_benchmark,
            commands::board_sync::start_board_sync,
            commands::board_sync::stop_board_sync,
            commands::board_sync::board_sync_status,
            commands::review::start_whole_game_analysis,
            commands::review::pause_whole_game_analysis,
            commands::review::resume_whole_game_analysis,
            commands::review::cancel_whole_game_analysis,
            commands::review::whole_game_progress,
            commands::review::whole_game_result,
            commands::problem_moves::problem_moves,
            commands::problem_moves::set_move_rank_settings,
            commands::strength::strength_estimate,
            commands::tsumego::generate_tsumego,
            commands::tsumego::exit_tsumego,
            commands::tsumego::tsumego_status,
            commands::engine::configure_engine,
            commands::engines::add_engine,
            commands::engines::update_engine,
            commands::engines::remove_engine,
            commands::engines::reorder_engines,
            commands::engines::select_main_engine,
            commands::engines::restart_engine,
            commands::engines::close_engine,
            commands::engines::set_engine_settings,
            commands::engines::preview_java_import,
            commands::engines::confirm_java_import,
            commands::engine::engine_display_name,
            commands::engine::detect_engines,
            commands::engine::auto_configure_engine,
            commands::engine::try_configure_engine,
            commands::fox::fox_search,
            commands::fox::fox_next_page,
            commands::fox::fox_open_game,
            commands::fox::fox_clear_history,
            commands::settings::settings,
            commands::settings::set_display_settings,
            commands::cache::cache_enabled,
            commands::cache::set_cache_enabled,
            commands::cache::clear_cache,
            commands::cache::clear_analysis,
            commands::settings::auto_quick_analyze_on_load,
            commands::settings::set_auto_quick_analyze_on_load,
        ])
}

fn context<R: Runtime>() -> tauri::Context<R> {
    tauri::generate_context!()
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let options = startup::StartupOptions::parse(std::env::args_os().skip(1))?;
    let (opener, incoming) = system_open::SystemOpener::channel();
    let builder = system_open::single_instance(tauri::Builder::default().manage(opener));
    let app = commands(builder)
        .setup(move |app| {
            app.manage(engine_setup::SetupEnvironment::for_app(app.handle())?);
            app.manage(board_sync::BoardSyncEnvironment::for_app(app.handle())?);
            let default_dir = match &options.config_dir {
                Some(directory) => directory.clone(),
                None => app.path().app_config_dir()?,
            };
            let data_dir = match &options.config_dir {
                Some(directory) => directory.clone(),
                None => app.path().app_data_dir()?,
            };
            let (core, subscription) = tauri::async_runtime::block_on(
                startup::start_with_data_directory(options, default_dir, data_dir),
            )?;
            app.manage(DesktopState::new(core.clone(), subscription));
            tauri::async_runtime::spawn(system_open::serve(core, incoming));
            Ok(())
        })
        .build(context())?;
    app.run(|app, event| {
        system_open::on_run_event(app, &event);
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
