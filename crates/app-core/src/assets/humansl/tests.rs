use super::*;

#[test]
fn backend_names_identify_explicit_tensorrt_and_leave_plain_names_unknown() {
    for name in [
        "/engines/cuda/katago",
        "C:\\KataGo\\katago-eigen.exe",
        "/metal/katago",
        "katago-opencl",
    ] {
        assert_eq!(backend_from_name(name), HumanModelBackend::Supported);
    }
    for name in [
        "katago-tensorrt.exe",
        "/cuda/katago-tensorrt",
        "C:\\KataGo\\katago-trt.exe",
        "katagoTRT.exe",
        "katago-trt10.exe",
    ] {
        assert_eq!(backend_from_name(name), HumanModelBackend::TensorRt);
    }
    for name in [
        "katago",
        "katago.exe",
        "/opt/homebrew/bin/katago",
        "/unknown/katago",
        "/nvidia/katago",
        "/trt/katago",
        "/TensorRT/runtime/katago.exe",
    ] {
        assert_eq!(backend_from_name(name), HumanModelBackend::Unknown);
    }
    assert_eq!(
        super::super::catalog::backend_for_digest(
            "6d1d1a0dce114ef3add7f859b7a19bc5bc23ec65b0357f5915a9d76cff2f3df0"
        )
        .as_deref(),
        Some("tensorrt")
    );
    assert_eq!(
        super::super::catalog::backend_for_digest(
            "3daaf0f32276102dbf1908cf322520de440d6416392b1104da504ebde52bc887"
        )
        .as_deref(),
        Some("metal")
    );
}

#[tokio::test]
async fn unknown_backend_never_becomes_supported_from_model_or_config_name() {
    let directory = tempfile::tempdir().unwrap();
    let program = directory.path().join("katago");
    tokio::fs::write(&program, b"unknown executable")
        .await
        .unwrap();
    let mut profile = crate::EngineProfile {
        program,
        model: "cuda.bin.gz".into(),
        config: "eigen.cfg".into(),
        ..Default::default()
    };
    assert_eq!(
        human_model_backend(&profile).await,
        HumanModelBackend::Unknown
    );
    profile.program = directory.path().join("katago-eigen");
    tokio::fs::write(&profile.program, b"unknown executable")
        .await
        .unwrap();
    assert_eq!(
        human_model_backend(&profile).await,
        HumanModelBackend::Supported
    );
}
