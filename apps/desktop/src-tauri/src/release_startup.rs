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
    // The UI invokes automatic_update_check after startup and receives any
    // available release. Attaching must not consume its daily claim first.
    Ok(())
}
