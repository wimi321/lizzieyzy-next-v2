use app_core::AppCore;
use tauri::Runtime;

pub(crate) async fn initialize<R: Runtime>(
    app: &tauri::AppHandle<R>,
    core: &AppCore,
) -> Result<(), Box<dyn std::error::Error>> {
    core.support_handle().await?.install_global()?;
    start_updates(core, crate::updater::backend(app)).await
}

pub(crate) async fn start_updates(
    core: &AppCore,
    backend: Result<Box<dyn app_core::updates::UpdateBackend>, app_core::updates::UpdateError>,
) -> Result<(), Box<dyn std::error::Error>> {
    let backend = match backend {
        Ok(backend) => backend,
        Err(app_core::updates::UpdateError::SigningNotConfigured) => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    core.attach_updates(app_core::updates::spawn(backend))
        .await?;
    let core = core.clone();
    tauri::async_runtime::spawn(async move {
        // A check only performs network I/O on the updater actor. It never
        // pauses analysis, installs anything, or raises a modal notification.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|time| u64::try_from(time.as_millis()).ok());
        if let Some(now) = now {
            match core.claim_daily_update_check(now).await {
                Ok(true) => {
                    let _ = core.check_update().await;
                }
                Ok(false) => {}
                Err(error) => eprintln!("automatic update check: {error}"),
            }
        }
    });
    Ok(())
}
