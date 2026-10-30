fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = tauri_build::AppManifest::new().commands(&[
        "open_bytes",
        "open_path",
        "save",
        "play",
        "step",
        "go_to",
        "undo",
        "redo",
        "set_pondering",
        "configure_engine",
        "settings",
        "subscribe",
        "unsubscribe",
    ]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))?;
    Ok(())
}
