use super::*;
use crate::assets::{AssetHandle, runtime_test_support::test_runtime_catalog, spawn_with_catalog};
use std::future::Future;

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("operation timed out")
}

fn option(plan: &AccelerationPlan, backend: AccelerationBackend) -> &AccelerationOption {
    plan.options
        .iter()
        .find(|option| option.backend == backend)
        .unwrap()
}

#[test]
fn recommendations_cover_nvidia_capabilities_driver_boundary_and_other_adapters() {
    let catalog = AssetCatalog::bundled().unwrap();
    for (cap, supported, recommended) in [
        ("6.1", false, false),
        ("7.5", true, true),
        ("8.6", true, true),
        ("8.8", true, true),
        ("8.9", true, false),
        ("12.0", true, false),
        ("N/A", true, false),
    ] {
        for (driver, note) in [
            ("528.33", Some(AccelerationNote::DriverMayBeTooOld)),
            ("570.64", Some(AccelerationNote::DriverMayBeTooOld)),
            ("570.65", None),
            ("570.100", None),
            ("N/A", None),
        ] {
            let gpus = parse_nvidia(&format!("NVIDIA unknown, {cap}, {driver}"), true);
            let plan = select_gpus("windows-x64", gpus, &catalog).unwrap();
            assert_eq!(plan.options.len(), 3);
            let cuda = option(&plan, AccelerationBackend::Cuda);
            assert!(cuda.supported && cuda.recommended);
            assert_eq!(cuda.note, note);
            let trt = option(&plan, AccelerationBackend::TensorRt);
            assert_eq!(trt.supported, supported);
            assert_eq!(trt.recommended, recommended);
            assert_eq!(trt.note, note);
            let opencl = option(&plan, AccelerationBackend::OpenCl);
            assert!(opencl.supported && !opencl.recommended);
            assert_eq!(opencl.note, None);
        }
    }
    for names in ["AMD Radeon RX 6800", "Intel UHD Graphics", ""] {
        let plan = select_gpus("windows-x64", parse_display_adapters(names), &catalog).unwrap();
        assert!(!option(&plan, AccelerationBackend::Cuda).supported);
        assert!(!option(&plan, AccelerationBackend::TensorRt).supported);
        let opencl = option(&plan, AccelerationBackend::OpenCl);
        assert!(opencl.supported);
        assert_eq!(opencl.recommended, !names.is_empty());
        assert_eq!(opencl.download_bytes, 3_127_940);
    }
    for platform in ["windows-arm64", "linux-x64", "macos-arm64", "macos-amd64"] {
        let plan = select_gpus(
            platform,
            parse_display_adapters("NVIDIA RTX 3080"),
            &catalog,
        )
        .unwrap();
        assert_eq!(plan.reason, AccelerationReason::UnsupportedPlatform);
        assert!(plan.options.is_empty());
        assert!(plan.gpus.is_empty());
    }
}

#[test]
fn adapter_parsing_filters_virtual_devices_and_keeps_driver_information() {
    let mut gpus = parse_nvidia(
        "NVIDIA RTX 3080, 8.6, 570.65\nNVIDIA unknown, N/A, N/A\n",
        true,
    );
    assert_eq!(gpus[0].driver_version.as_deref(), Some("570.65"));
    assert_eq!(gpus[0].compute_capability, Some(86));
    assert_eq!(gpus[1].driver_version, None);
    assert_eq!(gpus[1].compute_capability, None);
    let fallback = parse_nvidia("NVIDIA RTX 4090, 600.12\n", false);
    assert_eq!(fallback[0].compute_capability, Some(89));
    assert_eq!(fallback[0].driver_version.as_deref(), Some("600.12"));
    let others = parse_display_adapters(
        "\u{feff}NVIDIA RTX 3080\r\nAMD Radeon 显卡\r\nIntel UHD Graphics\n\
         GameViewer Virtual Display Adapter\nMicrosoft Remote Display Adapter\n\
         Microsoft Basic Display Adapter\nRDP Device\nMirror Driver\n\nAMD Radeon 显卡\n",
    );
    assert_eq!(others.len(), 3);
    merge_adapters(&mut gpus, others);
    assert_eq!(gpus.len(), 4);
    assert_eq!(gpus[0].driver_version.as_deref(), Some("570.65"));
    assert!(parse_nvidia("garbage\n, 8.6, 570.65", true).is_empty());
    assert!(!driver_may_be_old("N/A"));
    assert!(!driver_may_be_old("570"));
}

#[test]
fn catalog_has_pinned_opencl_and_recognizes_bundled_cpu_digest() {
    let catalog = AssetCatalog::bundled().unwrap();
    let opencl = catalog.get("windows-opencl").unwrap();
    assert!(opencl.url.contains("/next-2026-10-24.2/"));
    assert_eq!(opencl.runtime_profile.as_deref(), Some("opencl"));
    let profile = catalog.profile("opencl").unwrap();
    assert_eq!(profile.assets, ["windows-opencl"]);
    assert_eq!(profile.required_files, ["katago.exe", "OpenCL.dll"]);
    assert_eq!(
        backend_for_digest(
            &catalog,
            "38f6addb15f4b16d0417f3aaf55bdbcad6c624f0c665e80a2d317a11a675a6c0"
        ),
        EngineBackend::Eigen
    );
    for (id, backend) in [
        ("windows-nvidia", EngineBackend::Cuda),
        ("windows-tensorrt", EngineBackend::TensorRt),
        ("windows-opencl", EngineBackend::OpenCl),
    ] {
        let pin = catalog
            .get(id)
            .unwrap()
            .executable_sha256
            .as_deref()
            .unwrap();
        assert_eq!(backend_for_digest(&catalog, &pin.to_uppercase()), backend);
    }
}

async fn plan(handle: &AssetHandle, catalog: &AssetCatalog) -> AccelerationPlan {
    let base = select_gpus(
        "windows-x64",
        parse_nvidia("NVIDIA RTX 3080, 8.6, 570.65", true),
        catalog,
    )
    .unwrap();
    within(handle.complete_acceleration_plan(base, None))
        .await
        .unwrap()
}

#[tokio::test]
async fn remaining_download_bytes_count_only_unverified_archives_and_installed_requires_files() {
    let directory = tempfile::tempdir().unwrap();
    let (catalog, archives) = test_runtime_catalog(b"fake executable");
    let (handle, _) = spawn_with_catalog(directory.path().into(), catalog.clone());
    let total: u64 = archives.iter().map(|(_, bytes)| bytes.len() as u64).sum();
    assert_eq!(
        plan(&handle, &catalog).await.options[0].download_bytes,
        total
    );
    for (index, (relative, bytes)) in archives.iter().enumerate() {
        let path = directory.path().join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
        let expected: u64 = archives
            .iter()
            .skip(index + 1)
            .map(|(_, bytes)| bytes.len() as u64)
            .sum();
        let current = plan(&handle, &catalog).await;
        assert_eq!(current.options[0].download_bytes, expected);
        assert!(!current.options[0].installed);
    }
    let mut events = handle.subscribe_status();
    within(handle.install_runtime("cuda12.8-cudnn9".into()))
        .await
        .unwrap();
    within(async {
        loop {
            let phase = events.borrow().phase.clone();
            if phase == crate::assets::DownloadPhase::Completed {
                break;
            }
            assert_ne!(
                phase,
                crate::assets::DownloadPhase::Failed,
                "{:?}",
                *events.borrow()
            );
            events.changed().await.unwrap();
        }
    })
    .await;
    assert!(plan(&handle, &catalog).await.options[0].installed);
    std::fs::write(directory.path().join(&archives[0].0), b"corrupt").unwrap();
    let damaged = plan(&handle, &catalog).await;
    assert!(!damaged.options[0].installed);
    assert_eq!(
        damaged.options[0].download_bytes,
        archives[0].1.len() as u64
    );
    within(handle.shutdown()).await.unwrap();
}

#[tokio::test]
async fn current_backend_hashes_renamed_program_and_rejects_unknown_or_missing_files() {
    let directory = tempfile::tempdir().unwrap();
    let program = directory.path().join("renamed.exe");
    std::fs::write(&program, b"fake executable").unwrap();
    let (catalog, _) = test_runtime_catalog(b"fake executable");
    let (handle, _) = spawn_with_catalog(directory.path().into(), catalog);
    assert_eq!(
        within(handle.current_backend(program.clone()))
            .await
            .unwrap(),
        EngineBackend::Cuda
    );
    std::fs::write(&program, b"changed executable").unwrap();
    assert_eq!(
        within(handle.current_backend(program)).await.unwrap(),
        EngineBackend::External
    );
    assert_eq!(
        within(handle.current_backend(directory.path().join("missing")))
            .await
            .unwrap(),
        EngineBackend::External
    );
    within(handle.shutdown()).await.unwrap();
}
