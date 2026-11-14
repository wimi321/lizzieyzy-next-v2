//! Thin account adapters; credentials stay in the core service and secret store.
use crate::{DesktopState, error::CommandError};
use app_core::zhizi::*;
use tauri::State;

#[tauri::command]
pub async fn zhizi_status(state: State<'_, DesktopState>) -> Result<ZhiziStatus, CommandError> {
    Ok(state.core.zhizi_status().await?)
}

#[tauri::command]
pub async fn zhizi_login_password(
    state: State<'_, DesktopState>,
    account: String,
    password: String,
    remember: bool,
) -> Result<ZhiziStatus, CommandError> {
    Ok(state
        .core
        .zhizi_login_password(account, password, remember)
        .await?)
}

#[tauri::command]
pub async fn zhizi_login_code(
    state: State<'_, DesktopState>,
    account: String,
    code: String,
    remember: bool,
) -> Result<ZhiziStatus, CommandError> {
    Ok(state.core.zhizi_login_code(account, code, remember).await?)
}

#[tauri::command]
pub async fn zhizi_send_code(
    state: State<'_, DesktopState>,
    account: String,
    purpose: ZhiziVerificationPurpose,
) -> Result<ZhiziCodeDelivery, CommandError> {
    Ok(state.core.zhizi_send_code(account, purpose).await?)
}

#[tauri::command]
pub async fn zhizi_reset_password(
    state: State<'_, DesktopState>,
    account: String,
    code: String,
    password: String,
    remember: bool,
) -> Result<ZhiziStatus, CommandError> {
    Ok(state
        .core
        .zhizi_reset_password(account, code, password, remember)
        .await?)
}

#[tauri::command]
pub async fn zhizi_logout(state: State<'_, DesktopState>) -> Result<ZhiziStatus, CommandError> {
    Ok(state.core.zhizi_logout().await?)
}

#[tauri::command]
pub async fn zhizi_overview(
    state: State<'_, DesktopState>,
    force: bool,
) -> Result<ZhiziOverview, CommandError> {
    Ok(state.core.zhizi_overview(force).await?)
}

#[tauri::command]
pub async fn zhizi_usages(
    state: State<'_, DesktopState>,
    page: u32,
) -> Result<ZhiziUsagePage, CommandError> {
    Ok(state.core.zhizi_usages(page).await?)
}

#[tauri::command]
pub async fn zhizi_credits(
    state: State<'_, DesktopState>,
    page: u32,
) -> Result<ZhiziCreditPage, CommandError> {
    Ok(state.core.zhizi_credits(page).await?)
}

#[tauri::command]
pub fn zhizi_website() -> String {
    website_url().to_owned()
}
