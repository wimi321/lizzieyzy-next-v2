use super::{Harness, json, local_url, request};

#[tokio::test]
async fn commentary_ipc_round_trips_settings_memory_key_status_and_typed_errors() {
    // Harness uses AppConfig::default(), which never accesses the OS keychain.
    let harness = Harness::new().await;
    let settings = harness.invoke("settings", json!({})).await;
    assert_eq!(settings["commentary"]["rank"], "5k");
    assert_eq!(settings["commentary"]["write_to_comment"], true);
    let empty = harness.invoke("commentary_key_status", json!({})).await;
    assert_eq!(empty["has_key"], false);
    assert_eq!(empty["available"], false);
    let unconfigured = request(&harness.main, "commentary_models", json!({}), local_url())
        .await
        .unwrap_err();
    assert_eq!(unconfigured["type"], "Commentary");
    let configured = harness
        .invoke(
            "set_commentary_settings",
            json!({"settings": {"base_url": "http://127.0.0.1:9/", "model": "test-model"}}),
        )
        .await;
    assert_eq!(configured["base_url"], "http://127.0.0.1:9/v1");
    let status = harness
        .invoke(
            "set_commentary_key",
            json!({"key": "synthetic-ipc-test-key", "remember": true}),
        )
        .await;
    assert_eq!(status["has_key"], true);
    assert_eq!(status["stored"], false);
    assert_eq!(status["available"], false);
    let no_analysis = request(
        &harness.main,
        "start_commentary",
        json!({"request": {"mode": {"type": "NextMove"}}}),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(no_analysis["type"], "Commentary");
    assert_eq!(no_analysis["data"]["type"], "NoAnalysis");
    let invalid = request(
        &harness.main,
        "set_commentary_settings",
        json!({"settings": {"base_url": "http://example.com", "model": "test-model"}}),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(invalid["type"], "Commentary");
    let status = harness.invoke("clear_commentary_key", json!({})).await;
    assert_eq!(status["has_key"], false);
    let stopped = harness.invoke("stop_commentary", json!({})).await;
    assert_eq!(stopped["phase"], "Stopped");
    assert_eq!(
        harness.invoke("commentary_status", json!({})).await,
        stopped
    );
    let persisted =
        std::fs::read_to_string(harness.directory.path().join("settings.json")).unwrap();
    assert!(!persisted.contains("synthetic-ipc-test-key"));
    harness.finish().await;
}

#[tokio::test]
async fn commentary_commands_are_restricted_to_local_main() {
    let harness = Harness::new().await;
    let other =
        tauri::WebviewWindowBuilder::new(&harness.app, "commentary-other", Default::default())
            .build()
            .unwrap();
    for command in [
        "start_commentary",
        "stop_commentary",
        "commentary_status",
        "commentary_models",
        "set_commentary_key",
        "clear_commentary_key",
        "commentary_key_status",
        "set_commentary_settings",
    ] {
        for (window, url) in [
            (&other, local_url()),
            (&harness.main, "https://example.com/"),
        ] {
            let error = request(window, command, json!({}), url).await.unwrap_err();
            assert!(
                error
                    .as_str()
                    .is_some_and(|text| text.contains("not allowed")),
                "{command}: {error}"
            );
        }
    }
    harness.finish().await;
}
