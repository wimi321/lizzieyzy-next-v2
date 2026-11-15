//! Window commands delegate to the serialized shell controller and AppCore.
use crate::{
    error::CommandError,
    floating::{Action, FloatingWindows},
};
use app_core::{DisplaySettings, FloatingBoardKind, FloatingBoardsView, WindowLayout};
use tauri::{Manager, Runtime, Webview};

fn own_window(label: &str, kind: FloatingBoardKind) -> Result<(), CommandError> {
    let own = match kind {
        FloatingBoardKind::Main => "float-main",
        FloatingBoardKind::Sub => "float-sub",
    };
    if label == "main" || label == own {
        Ok(())
    } else {
        Err(CommandError::WindowUnavailable(
            "a floating board can only change its own window".into(),
        ))
    }
}

fn service<R: Runtime>(webview: &Webview<R>) -> Result<FloatingWindows, CommandError> {
    webview
        .try_state::<FloatingWindows>()
        .map(|state| state.inner().clone())
        .ok_or_else(|| CommandError::WindowUnavailable("window controller is unavailable".into()))
}

#[tauri::command]
pub async fn open_floating_board<R: Runtime>(
    webview: Webview<R>,
    kind: FloatingBoardKind,
) -> Result<FloatingBoardsView, CommandError> {
    Ok(service(&webview)?
        .apply(Action::Open(kind, true))
        .await?
        .floating_boards)
}

#[tauri::command]
pub async fn close_floating_board<R: Runtime>(
    webview: Webview<R>,
    kind: FloatingBoardKind,
) -> Result<FloatingBoardsView, CommandError> {
    own_window(webview.label(), kind)?;
    Ok(service(&webview)?
        .apply(Action::Open(kind, false))
        .await?
        .floating_boards)
}

#[tauri::command]
pub async fn set_floating_board_options<R: Runtime>(
    webview: Webview<R>,
    kind: FloatingBoardKind,
    on_top: bool,
    locked: bool,
) -> Result<FloatingBoardsView, CommandError> {
    own_window(webview.label(), kind)?;
    Ok(service(&webview)?
        .apply(Action::Options(kind, on_top, locked))
        .await?
        .floating_boards)
}

#[tauri::command]
pub async fn set_floating_layout<R: Runtime>(
    webview: Webview<R>,
    main: bool,
    keep_main_board: bool,
    sub: bool,
) -> Result<FloatingBoardsView, CommandError> {
    Ok(service(&webview)?
        .apply(Action::Layout(main, keep_main_board, sub))
        .await?
        .floating_boards)
}

#[tauri::command]
pub async fn fit_main_window<R: Runtime>(
    webview: Webview<R>,
    width: u32,
    height: u32,
) -> Result<(), CommandError> {
    service(&webview)?.apply(Action::Fit(width, height)).await?;
    Ok(())
}

#[tauri::command]
pub async fn reset_window_positions<R: Runtime>(webview: Webview<R>) -> Result<(), CommandError> {
    service(&webview)?.apply(Action::Reset).await?;
    Ok(())
}

#[tauri::command]
pub async fn apply_window_layout<R: Runtime>(
    webview: Webview<R>,
    layout: WindowLayout,
) -> Result<DisplaySettings, CommandError> {
    service(&webview)?.apply(Action::ApplyLayout(layout)).await
}
