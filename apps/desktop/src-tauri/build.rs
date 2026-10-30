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
        "detect_engines",
        "try_configure_engine",
        "settings",
        "subscribe",
        "unsubscribe",
    ]);
    let mut attributes = tauri_build::Attributes::new().app_manifest(manifest);
    let windows_msvc = std::env::var("CARGO_CFG_TARGET_OS")? == "windows"
        && std::env::var("CARGO_CFG_TARGET_ENV")? == "msvc";
    if windows_msvc {
        // The linker embeds one manifest in every executable, including test
        // harnesses. Disable Tauri's resource manifest to avoid embedding twice.
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
    }
    tauri_build::try_build(attributes)?;

    println!("cargo:rerun-if-changed=windows-app.manifest");
    if windows_msvc {
        // Cargo supplies an absolute package path and forwards each link-arg
        // as one argument, including when the path contains spaces.
        let manifest_path = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?)
            .join("windows-app.manifest");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg=/MANIFESTINPUT:{}",
            manifest_path.display()
        );
    }
    Ok(())
}
