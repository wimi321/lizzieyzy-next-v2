use super::{Harness, json, local_url, profile, request};

#[tokio::test]
async fn automatic_setup_ipc_preserves_missing_asset_and_per_pair_errors() {
    let harness = Harness::new().await;
    let no_programs = request(
        &harness.main,
        "auto_configure_engine",
        json!({}),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(
        no_programs,
        json!({ "type": "AutoSetup", "data": { "type": "NoPrograms" } })
    );

    let installation = harness.directory.path().join("katago");
    std::fs::create_dir_all(&installation).unwrap();
    let program = installation.join(if cfg!(windows) {
        "katago.exe"
    } else {
        "katago"
    });
    // An invalid executable makes version probing fail without any real engine.
    std::fs::write(&program, b"not an executable image").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let no_models = request(
        &harness.main,
        "auto_configure_engine",
        json!({}),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(
        no_models,
        json!({ "type": "AutoSetup", "data": { "type": "NoModels" } })
    );

    let model = installation.join("default.bin.gz");
    std::fs::write(&model, b"fake model").unwrap();
    let all_failed = request(
        &harness.main,
        "auto_configure_engine",
        json!({}),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(all_failed["type"], "AutoSetup");
    assert_eq!(all_failed["data"]["type"], "AllFailed");
    let attempts = all_failed["data"]["data"]["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(attempts[0]["program"], json!(program));
    assert_eq!(attempts[0]["model"], json!(model));
    assert!(!attempts[0]["reason"].as_str().unwrap().is_empty());
    assert!(!harness.directory.path().join("settings.json").exists());
    assert!(harness.core.settings().await.unwrap().engine.is_none());
    harness.finish().await;
}

#[tokio::test]
async fn automatic_setup_ipc_returns_saved_profile_and_canonical_name_unchanged() {
    let harness = Harness::new().await;
    let mut saved = profile(harness.directory.path());
    saved.canonical_model_name = Some("kata1-tf3-b11c768-s12002M-d6304M.bin.gz".into());
    harness
        .invoke("configure_engine", json!({ "profile": saved }))
        .await;
    let before = std::fs::read(harness.directory.path().join("settings.json")).unwrap();
    // The injected search roots contain no discoverable executable. Returning
    // the saved profile proves auto setup did not require fresh discovery.
    assert_eq!(
        harness.invoke("auto_configure_engine", json!({})).await,
        json!(saved)
    );
    assert_eq!(
        std::fs::read(harness.directory.path().join("settings.json")).unwrap(),
        before
    );
    harness.finish().await;
}
