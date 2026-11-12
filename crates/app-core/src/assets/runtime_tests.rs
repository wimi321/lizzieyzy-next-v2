use super::*;
use crate::assets::{
    DownloadPhase,
    runtime_test_support::{test_runtime_catalog, zip_files},
};
use std::time::Duration;

const PROFILE: &str = "cuda12.8-cudnn9";
fn fixture() -> (tempfile::TempDir, AssetCatalog) {
    let directory = tempfile::tempdir().unwrap();
    let (catalog, archives) = test_runtime_catalog(b"tiny fake executable");
    for (path, bytes) in archives {
        let path = directory.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    (directory, catalog)
}
fn installed(root: &Path, catalog: &AssetCatalog) -> PathBuf {
    let (_sender, cancel) = watch::channel(false);
    let (temp, target) = stage(root, catalog, PROFILE, &cancel).unwrap();
    publish(temp, target).unwrap()
}
fn change_archive(root: &Path, catalog: &mut AssetCatalog, id: &str, bytes: Vec<u8>) {
    let spec = catalog.specs.iter_mut().find(|spec| spec.id == id).unwrap();
    spec.size = bytes.len() as u64;
    spec.sha256 = format!("{:x}", Sha256::digest(&bytes));
    fs::write(root.join(spec.relative_path()), bytes).unwrap();
}
async fn terminal(
    events: &mut watch::Receiver<super::super::AssetDownloadStatus>,
) -> super::super::AssetDownloadStatus {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let status = events.borrow_and_update().clone();
            if matches!(
                status.phase,
                DownloadPhase::Completed | DownloadPhase::Failed | DownloadPhase::Cancelled
            ) {
                return status;
            }
            events.changed().await.unwrap();
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn actor_installs_verified_archives_and_repairs_missing_dll_without_network() {
    let (directory, catalog) = fixture();
    let (handle, mut events) =
        super::super::spawn_with_catalog(directory.path().to_path_buf(), catalog);
    assert!(
        !handle
            .runtime_installation(PROFILE.into())
            .await
            .unwrap()
            .ready
    );
    let started = handle.install_runtime(PROFILE.into()).await.unwrap();
    assert_eq!(started.asset_id.as_deref(), Some(PROFILE));
    assert_eq!(terminal(&mut events).await.phase, DownloadPhase::Completed);
    let first = handle.runtime_installation(PROFILE.into()).await.unwrap();
    assert!(first.ready);
    let first_bin = first.directory.unwrap();
    assert_eq!(
        fs::read(first_bin.join("runtime.dll")).unwrap(),
        b"tiny runtime DLL"
    );
    handle.install_runtime(PROFILE.into()).await.unwrap();
    assert_eq!(terminal(&mut events).await.phase, DownloadPhase::Completed);
    assert_eq!(
        handle
            .runtime_installation(PROFILE.into())
            .await
            .unwrap()
            .directory
            .as_ref(),
        Some(&first_bin)
    );
    fs::remove_file(first_bin.join("runtime.dll")).unwrap();
    assert!(
        !handle
            .runtime_installation(PROFILE.into())
            .await
            .unwrap()
            .ready
    );
    handle.install_runtime(PROFILE.into()).await.unwrap();
    assert_eq!(terminal(&mut events).await.phase, DownloadPhase::Completed);
    let fixed = handle.runtime_installation(PROFILE.into()).await.unwrap();
    assert!(fixed.ready);
    assert_ne!(fixed.directory.as_ref(), Some(&first_bin));
    assert!(first_bin.join("katago.exe").exists());
    handle.shutdown().await.unwrap();
}

#[test]
fn every_installed_file_is_verified_and_a_fake_manifest_cannot_claim_ready() {
    let (directory, catalog) = fixture();
    let bin = installed(directory.path(), &catalog);
    fs::write(bin.join("runtime.dll"), b"tampered runtime").unwrap();
    let state = inspect(directory.path(), &catalog, PROFILE).unwrap();
    assert!(!state.ready);
    assert!(
        state
            .missing
            .iter()
            .any(|path| path.contains("runtime.dll"))
    );
    fs::write(
        bin.parent().unwrap().join("manifest.json"),
        br#"{"ready":true,"files":[]}"#,
    )
    .unwrap();
    assert!(!inspect(directory.path(), &catalog, PROFILE).unwrap().ready);
}

#[test]
fn pinned_archives_are_reverified_before_reconstructing_integrity() {
    let (directory, catalog) = fixture();
    installed(directory.path(), &catalog);
    let archive = directory
        .path()
        .join(catalog.get("runtime-cuda_cudart").unwrap().relative_path());
    let mut bytes = fs::read(&archive).unwrap();
    bytes[0] ^= 1;
    fs::write(archive, bytes).unwrap();
    let state = inspect(directory.path(), &catalog, PROFILE).unwrap();
    assert!(!state.ready);
    assert!(state.error.unwrap().contains("SHA-256"));
}

#[test]
fn incomplete_runtime_and_executable_pin_mismatch_never_publish() {
    for mismatch in [false, true] {
        let (directory, mut catalog) = fixture();
        if mismatch {
            catalog.specs[0].executable_sha256 = Some("0".repeat(64));
        } else {
            change_archive(
                directory.path(),
                &mut catalog,
                "runtime-cuda_cudart",
                zip_files(&[("LICENSE", b"no DLL")]),
            );
        }
        let (_sender, cancel) = watch::channel(false);
        assert!(stage(directory.path(), &catalog, PROFILE, &cancel).is_err());
        assert_eq!(
            fs::read_dir(directory.path().join("runtimes").join(PROFILE))
                .unwrap()
                .count(),
            0
        );
    }
}

#[test]
fn traversal_absolute_windows_paths_and_symlinks_are_rejected() {
    for unsafe_name in [
        "../escape.dll",
        "/escape.dll",
        "C:/escape.dll",
        "safe/../../escape.dll",
        "safe\\evil.dll",
        "safe/CON.dll",
    ] {
        let (directory, mut catalog) = fixture();
        let bytes = zip_files(&[(unsafe_name, b"unsafe")]);
        change_archive(directory.path(), &mut catalog, "runtime-cuda_cudart", bytes);
        let (_sender, cancel) = watch::channel(false);
        assert!(
            stage(directory.path(), &catalog, PROFILE, &cancel).is_err(),
            "{unsafe_name}"
        );
        assert!(!directory.path().join("escape.dll").exists());
    }
    let (directory, mut catalog) = fixture();
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    writer
        .add_symlink(
            "link.dll",
            "../../outside",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
    change_archive(
        directory.path(),
        &mut catalog,
        "runtime-cuda_cudart",
        writer.finish().unwrap().into_inner(),
    );
    let (_sender, cancel) = watch::channel(false);
    assert!(stage(directory.path(), &catalog, PROFILE, &cancel).is_err());
}

#[test]
fn cancellation_cleans_staging_and_leaves_old_installation() {
    let (directory, catalog) = fixture();
    let first = installed(directory.path(), &catalog);
    let (_sender, cancel) = watch::channel(true);
    assert!(matches!(
        stage(directory.path(), &catalog, PROFILE, &cancel),
        Err(AssetError::Cancelled)
    ));
    assert!(first.join("runtime.dll").exists());
    assert_eq!(
        fs::read_dir(directory.path().join("runtimes").join(PROFILE))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn external_and_managed_programs_are_identified_by_the_pinned_executable_digest() {
    let (directory, catalog) = fixture();
    let bin = installed(directory.path(), &catalog);
    let external = directory.path().join("external.exe");
    fs::write(&external, b"tiny fake executable").unwrap();
    let external_status = for_program(directory.path(), &catalog, &external)
        .unwrap()
        .unwrap();
    assert!(!external_status.ready);
    assert_eq!(external_status.profile_id, PROFILE);
    assert!(external_status.error.unwrap().contains("switching"));
    assert!(
        for_program(directory.path(), &catalog, &bin.join("katago.exe"))
            .unwrap()
            .unwrap()
            .ready
    );
    fs::write(&external, b"unknown exe").unwrap();
    assert!(
        for_program(directory.path(), &catalog, &external)
            .unwrap()
            .is_none()
    );
    fs::remove_file(bin.join("katago.exe")).unwrap();
    let missing = for_program(directory.path(), &catalog, &bin.join("katago.exe"))
        .unwrap()
        .unwrap();
    assert!(!missing.ready);
    assert_eq!(missing.profile_id, PROFILE);
}

#[test]
fn cancellation_after_first_extracted_file_cleans_partial_installation() {
    let (directory, catalog) = fixture();
    let (signal, cancel) = watch::channel(false);
    let mut files = 0;
    let result = stage_with_progress(directory.path(), &catalog, PROFILE, &cancel, &mut || {
        files += 1;
        signal.send_replace(true);
    });
    assert_eq!(files, 1);
    assert!(matches!(result, Err(AssetError::Cancelled)));
    assert_eq!(
        fs::read_dir(directory.path().join("runtimes").join(PROFILE))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn damaged_selected_generation_cannot_borrow_a_complete_generations_runtime() {
    let (directory, catalog) = fixture();
    let selected = installed(directory.path(), &catalog);
    fs::write(
        selected.join("runtime.dll"),
        b"wrong DLL beside selected executable",
    )
    .unwrap();
    let replacement = installed(directory.path(), &catalog);
    assert_ne!(selected, replacement);
    assert!(inspect(directory.path(), &catalog, PROFILE).unwrap().ready);
    let old = for_program(directory.path(), &catalog, &selected.join("katago.exe"))
        .unwrap()
        .unwrap();
    assert!(!old.ready);
    assert_eq!(old.directory.as_deref(), Some(selected.as_path()));
    assert!(
        old.missing
            .iter()
            .any(|missing| missing.contains("runtime.dll"))
    );
    assert!(
        for_program(directory.path(), &catalog, &replacement.join("katago.exe"))
            .unwrap()
            .unwrap()
            .ready
    );
}

#[test]
fn external_directory_missing_path_and_unreadable_program_do_not_abort_diagnostics() {
    let (directory, catalog) = fixture();
    // On Unix open(directory) succeeds but reading fails; on Windows opening
    // a directory fails. Both I/O paths must leave ordinary diagnostics intact.
    assert!(
        for_program(directory.path(), &catalog, directory.path())
            .unwrap()
            .is_none()
    );
    assert!(
        for_program(
            directory.path(),
            &catalog,
            &directory.path().join("missing.exe")
        )
        .unwrap()
        .is_none()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let loop_path = directory.path().join("unreadable.exe");
        symlink("unreadable.exe", &loop_path).unwrap();
        assert!(
            for_program(directory.path(), &catalog, &loop_path)
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn managed_program_io_errors_are_reported_as_an_incomplete_runtime() {
    let (directory, catalog) = fixture();
    let bin = installed(directory.path(), &catalog);
    let program = bin.join("katago.exe");
    fs::remove_file(&program).unwrap();
    fs::create_dir(&program).unwrap();
    let status = for_program(directory.path(), &catalog, &program)
        .unwrap()
        .unwrap();
    assert!(!status.ready);
    assert_eq!(status.profile_id, PROFILE);
    assert_eq!(status.program.as_ref(), Some(&program));
    assert!(status.error.unwrap().contains("not a regular file"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        fs::remove_dir(&program).unwrap();
        symlink("katago.exe", &program).unwrap();
        let status = for_program(directory.path(), &catalog, &program)
            .unwrap()
            .unwrap();
        assert!(!status.ready);
        assert!(status.error.is_some());
    }
}
