#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::assets::{
    AssetCatalog, DownloadPhase, runtime, runtime_test_support::test_runtime_catalog,
    spawn_with_catalog,
};
use std::{future::Future, time::Duration};

const PROFILE: &str = "cuda12.8-cudnn9";

struct Fixture {
    root: tempfile::TempDir,
    catalog: AssetCatalog,
    generation: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let (catalog, archives) = test_runtime_catalog(b"tiny fake executable");
        for (relative, bytes) in archives {
            let path = root.path().join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        let (_sender, cancel) = watch::channel(false);
        let (temp, target) = runtime::stage(root.path(), &catalog, PROFILE, &cancel).unwrap();
        let bin = runtime::publish(temp, target).unwrap();
        Self {
            root,
            catalog,
            generation: bin.parent().unwrap().to_path_buf(),
        }
    }

    fn quick(&self) -> super::super::RuntimeInstallation {
        runtime::inspect_quick(self.root.path(), &self.catalog, PROFILE).unwrap()
    }

    fn remove_archives(&self) {
        for spec in &self.catalog.specs {
            fs::remove_file(self.root.path().join(spec.relative_path())).unwrap();
        }
    }
}

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("runtime receipt test timed out")
}

#[test]
fn publication_records_metadata_outside_generation_and_fast_check_needs_no_archives() {
    let fixture = Fixture::new();
    let receipt = path(&fixture.generation).unwrap();
    assert!(receipt.is_file());
    assert_eq!(receipt.parent(), fixture.generation.parent());
    assert!(!receipt.starts_with(&fixture.generation));
    let record: Record = serde_json::from_slice(&fs::read(receipt).unwrap()).unwrap();
    assert_eq!(record.version, VERSION);
    assert!(record.files.contains_key(Path::new("bin/katago.exe")));
    assert!(fixture.quick().ready);
    fixture.remove_archives();
    assert!(fixture.quick().ready);
    let program = fixture.generation.join("bin/katago.exe");
    assert!(
        runtime::for_program_quick(fixture.root.path(), &fixture.catalog, &program)
            .unwrap()
            .unwrap()
            .ready
    );
    assert!(
        !runtime::for_program(fixture.root.path(), &fixture.catalog, &program)
            .unwrap()
            .unwrap()
            .ready,
        "explicit diagnosis still requires pinned archives"
    );
}

#[test]
fn fast_check_rejects_size_mtime_missing_extra_and_same_metadata_executable_replacement() {
    for mutation in 0..5 {
        let fixture = Fixture::new();
        let dll = fixture.generation.join("bin/runtime.dll");
        match mutation {
            0 => fs::write(&dll, b"different length plus data").unwrap(),
            1 => File::options()
                .write(true)
                .open(&dll)
                .unwrap()
                .set_times(
                    fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(1_000_000)),
                )
                .unwrap(),
            2 => fs::remove_file(&dll).unwrap(),
            3 => fs::write(fixture.generation.join("bin/unexpected.dll"), b"extra").unwrap(),
            4 => {
                let program = fixture.generation.join("bin/katago.exe");
                let modified = fs::metadata(&program).unwrap().modified().unwrap();
                let mut bytes = fs::read(&program).unwrap();
                bytes[0] ^= 1;
                fs::write(&program, bytes).unwrap();
                File::options()
                    .write(true)
                    .open(program)
                    .unwrap()
                    .set_times(fs::FileTimes::new().set_modified(modified))
                    .unwrap();
            }
            _ => unreachable!(),
        }
        let state = fixture.quick();
        assert!(!state.ready, "mutation {mutation}");
        assert!(!state.missing.is_empty(), "mutation {mutation}");
        // A mismatch never refreshes the receipt to bless altered files.
        assert!(!fixture.quick().ready);
    }
}

#[test]
fn missing_corrupt_and_unknown_receipts_require_full_validation_then_are_rewritten() {
    for replacement in [None, Some("not json"), Some("unknown version")] {
        let fixture = Fixture::new();
        let receipt = path(&fixture.generation).unwrap();
        let mut unsupported: serde_json::Value =
            serde_json::from_slice(&fs::read(&receipt).unwrap()).unwrap();
        unsupported["version"] = 99.into();
        fs::remove_file(&receipt).unwrap();
        if let Some(replacement) = replacement {
            let bytes = if replacement == "unknown version" {
                serde_json::to_vec(&unsupported).unwrap()
            } else {
                replacement.as_bytes().to_vec()
            };
            fs::write(&receipt, bytes).unwrap();
        }
        assert!(fixture.quick().ready);
        let record: Record = serde_json::from_slice(&fs::read(&receipt).unwrap()).unwrap();
        assert_eq!(record.version, VERSION);
        fixture.remove_archives();
        assert!(fixture.quick().ready);
        // Once the receipt is lost, the missing archives prevent fallback from
        // silently claiming that unverified files are installed.
        fs::remove_file(receipt).unwrap();
        assert!(!fixture.quick().ready);
    }
}

#[test]
fn corrupted_receipt_does_not_bless_changed_installation_during_fallback() {
    let fixture = Fixture::new();
    fs::write(path(&fixture.generation).unwrap(), "broken receipt").unwrap();
    fs::write(fixture.generation.join("bin/runtime.dll"), "changed DLL").unwrap();
    assert!(!fixture.quick().ready);
}

#[tokio::test]
async fn activating_installed_runtime_after_deleting_archives_does_not_download() {
    let fixture = Fixture::new();
    fixture.remove_archives();
    let (handle, mut events) =
        spawn_with_catalog(fixture.root.path().into(), fixture.catalog.clone());
    within(handle.ensure_runtime(PROFILE.into())).await.unwrap();
    let status = within(async {
        loop {
            let status = events.borrow_and_update().clone();
            if matches!(
                status.phase,
                DownloadPhase::Completed | DownloadPhase::Failed
            ) {
                break status;
            }
            events.changed().await.unwrap();
        }
    })
    .await;
    assert_eq!(status.phase, DownloadPhase::Completed, "{status:?}");
    assert!(
        within(handle.runtime_installation_quick(PROFILE.into()))
            .await
            .unwrap()
            .ready
    );
    let program = fixture.generation.join("bin/katago.exe");
    assert!(
        within(handle.runtime_for_program_quick(program))
            .await
            .unwrap()
            .unwrap()
            .ready
    );
    assert!(
        fixture.catalog.specs.iter().all(|spec| !fixture
            .root
            .path()
            .join(spec.relative_path())
            .exists())
    );
    within(handle.shutdown()).await.unwrap();
    within(tokio::task::spawn_blocking(move || fixture.root.close()))
        .await
        .unwrap()
        .unwrap();
}
