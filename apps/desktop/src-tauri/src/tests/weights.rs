use super::{Harness, json, local_url, profile, request};

#[tokio::test]
async fn weight_queries_and_invalid_downloads_do_not_touch_the_network() {
    let harness = Harness::new().await;
    let weights = harness.invoke("official_weights", json!({})).await;
    let weights = weights.as_array().unwrap();
    assert!(!weights.is_empty());
    assert!(weights.iter().all(|weight| weight["downloaded"] == false));
    assert!(
        weights
            .iter()
            .all(|weight| weight["size_bytes"].as_u64().unwrap() > 0)
    );
    assert_eq!(
        harness.invoke("asset_download_status", json!({})).await["phase"],
        "Idle"
    );
    assert_eq!(
        harness.invoke("cancel_asset_download", json!({})).await["phase"],
        "Idle"
    );
    for command in ["start_asset_download", "use_official_weight"] {
        let error = request(
            &harness.main,
            command,
            json!({ "assetId": "not-a-catalog-id" }),
            local_url(),
        )
        .await
        .unwrap_err();
        assert_eq!(error["type"], "App");
    }
    assert_eq!(
        harness.invoke("diagnose_engine", json!({})).await,
        json!({ "profile": null, "checks": [] })
    );
    // The runtime action is decoded and forwarded to AppCore. With no main
    // engine it fails before any hardware probe or remote download can run.
    let error = request(
        &harness.main,
        "repair_engine",
        json!({ "repair": "InstallRuntime" }),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(error["type"], "App");
    assert!(!harness.directory.path().join("settings.json").exists());
    harness.finish().await;
}

#[tokio::test]
async fn diagnostics_ipc_reports_paths_and_repairs_configuration_with_the_engine_transaction() {
    let harness = Harness::new().await;
    let profile = profile(harness.directory.path());
    std::fs::write(&profile.config, "numSearchThreadsPerAnalysisThread = 1\n").unwrap();
    std::fs::write(
        harness
            .directory
            .path()
            .join("resources/katago/analysis.cfg"),
        "numSearchThreadsPerAnalysisThread = 1\nreportAnalysisWinratesAs = BLACK\n",
    )
    .unwrap();
    harness
        .invoke("configure_engine", json!({ "profile": profile }))
        .await;
    let report = harness.invoke("diagnose_engine", json!({})).await;
    assert_eq!(report["profile"], json!(profile));
    assert_eq!(report["checks"].as_array().unwrap().len(), 8);
    assert!(
        report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|check| check["status"] == expected_fake_check_status(check)),
        "{report}"
    );
    assert_eq!(report["checks"][7]["kind"], "RuntimeComponents");
    assert_eq!(report["checks"][7]["status"], "Skipped");
    assert_eq!(report["checks"][7]["repairs"], json!([]));
    // fake-katago deliberately implements analysis/benchmark only. The real
    // read-only version command must expose that failure instead of inventing
    // a version or loading a model during path diagnosis.
    assert!(
        report["checks"][2]["detail"]
            .as_str()
            .unwrap()
            .contains("expected the analysis or benchmark subcommand")
    );
    std::fs::write(&profile.config, "not an assignment\n").unwrap();
    let broken = harness.invoke("diagnose_engine", json!({})).await;
    let config = broken["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["kind"] == "ConfigValid")
        .unwrap();
    assert_eq!(config["status"], "Error");
    assert!(
        config["repairs"]
            .as_array()
            .unwrap()
            .contains(&json!("RegenerateConfig"))
    );
    let repaired = harness
        .invoke("repair_engine", json!({ "repair": "RegenerateConfig" }))
        .await;
    assert!(
        repaired["checks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|check| check["status"] == expected_fake_check_status(check)),
        "{repaired}"
    );
    assert_eq!(harness.invoke("diagnose_engine", json!({})).await, repaired);
    assert_eq!(
        std::fs::read_to_string(&profile.config).unwrap(),
        "not an assignment\n"
    );
    assert_ne!(repaired["profile"]["config"], json!(profile.config));
    harness.finish().await;
}

fn expected_fake_check_status(check: &serde_json::Value) -> &'static str {
    match check["kind"].as_str() {
        Some("ProgramVersion") => "Error",
        Some("RuntimeComponents") => "Skipped",
        _ => "Ok",
    }
}

#[tokio::test]
async fn weight_and_diagnostic_commands_are_only_allowed_from_local_main() {
    let harness = Harness::new().await;
    let other = tauri::WebviewWindowBuilder::new(&harness.app, "weights-other", Default::default())
        .build()
        .unwrap();
    for (window, url) in [
        (&other, local_url()),
        (&harness.main, "https://example.com/"),
    ] {
        for command in [
            "official_weights",
            "start_asset_download",
            "cancel_asset_download",
            "asset_download_status",
            "acceleration_plan",
            "use_official_weight",
            "diagnose_engine",
            "repair_engine",
        ] {
            let error = request(window, command, json!({}), url).await.unwrap_err();
            assert!(
                error.as_str().unwrap().contains("not allowed"),
                "{command}: {error}"
            );
        }
    }
    harness.finish().await;
}
