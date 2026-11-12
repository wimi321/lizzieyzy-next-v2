use super::{Harness, json, local_url, profile, request};
use app_core::Settings;

#[tokio::test]
async fn engine_settings_ipc_saves_defaults_and_rejects_invalid_updates() {
    let harness = Harness::new().await;
    let settings = json!({
        "rules": { "type": "Japanese" }, "read_komi": false,
        "default_komi": 6.5, "auto_check_alive": false, "report_interval_ms": 250,
    });
    assert_eq!(
        harness
            .invoke("set_engine_settings", json!({ "settings": settings }))
            .await,
        settings
    );
    assert_eq!(
        harness.invoke("settings", json!({})).await["engine_settings"],
        settings
    );
    assert_eq!(
        json!(
            Settings::load(harness.directory.path())
                .await
                .unwrap()
                .engine_settings
        ),
        settings
    );
    let error = request(
        &harness.main,
        "set_engine_settings",
        json!({
            "settings": { "report_interval_ms": 0 }
        }),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(error["type"], "App");
    assert_eq!(
        harness.invoke("settings", json!({})).await["engine_settings"],
        settings
    );
    assert_eq!(
        harness
            .invoke("set_engine_settings", json!({ "settings": {} }))
            .await,
        json!(app_core::EngineSettings::default())
    );
    harness.finish().await;
}

#[tokio::test]
async fn engine_list_ipc_round_trips_and_preserves_legacy_projection() {
    let harness = Harness::new().await;
    let profile = profile(harness.directory.path());
    let first = harness
        .invoke("add_engine", json!({ "name": "CPU", "profile": profile }))
        .await;
    let first_id = first["engines"][0]["id"].as_str().unwrap();
    assert!(first["engine"].is_null());
    let second = harness
        .invoke("add_engine", json!({ "name": "GPU", "profile": profile }))
        .await;
    let second_id = second["engines"][1]["id"].as_str().unwrap();
    let reordered = harness
        .invoke("reorder_engines", json!({ "ids": [second_id, first_id] }))
        .await;
    assert_eq!(reordered["engines"][0]["id"], second_id);
    let updated = harness
        .invoke(
            "update_engine",
            json!({ "id": first_id, "name": "Renamed CPU", "profile": profile }),
        )
        .await;
    assert_eq!(updated["engines"][1]["name"], "Renamed CPU");
    let selected = harness
        .invoke("select_main_engine", json!({ "id": first_id }))
        .await;
    assert_eq!(selected["main_engine_id"], first_id);
    assert_eq!(selected["engine"], json!(profile));
    let restarted = harness.invoke("restart_engine", json!({})).await;
    assert_eq!(restarted, selected);
    let closed = harness.invoke("close_engine", json!({})).await;
    assert!(closed["engine"].is_null());
    assert_eq!(closed["main_engine_id"], first_id);
    let removed = harness
        .invoke("remove_engine", json!({ "id": second_id }))
        .await;
    assert_eq!(removed["engines"].as_array().unwrap().len(), 1);
    assert_eq!(removed["engines"][0]["id"], first_id);
    let saved = Settings::load(harness.directory.path()).await.unwrap();
    assert_eq!(json!(saved), removed);
    // The existing settings UI still configures the selected entry.
    harness
        .invoke("configure_engine", json!({ "profile": profile }))
        .await;
    let legacy = harness.invoke("settings", json!({})).await;
    assert_eq!(legacy["engine"], json!(profile));
    assert_eq!(legacy["engines"].as_array().unwrap().len(), 1);
    harness.finish().await;
}

#[tokio::test]
async fn java_import_ipc_previews_then_confirms_without_writing_source() {
    let harness = Harness::new().await;
    let source = tempfile::TempDir::new().unwrap();
    let config = source.path().join("config.txt");
    let bytes = br#"{"leelaz":{"engine-command":"missing-katago analysis -model missing.bin -config analysis.cfg"},"ui":{"show-coordinates":false}}"#;
    std::fs::write(&config, bytes).unwrap();
    let before = harness.invoke("settings", json!({})).await;
    let preview = harness
        .invoke("preview_java_import", json!({ "directory": source.path() }))
        .await;
    assert_eq!(harness.invoke("settings", json!({})).await, before);
    assert!(!harness.directory.path().join("settings.json").exists());
    let confirmed = harness
        .invoke(
            "confirm_java_import",
            json!({ "previewId": preview["preview_id"] }),
        )
        .await;
    assert_eq!(confirmed["display"]["show_coordinates"], false);
    assert!(confirmed["engine"].is_null());
    assert_eq!(std::fs::read(&config).unwrap(), bytes);
    assert_eq!(std::fs::read_dir(source.path()).unwrap().count(), 1);
    assert_eq!(
        json!(Settings::load(harness.directory.path()).await.unwrap()),
        confirmed
    );
    harness.finish().await;
}

#[tokio::test]
async fn engine_commands_are_only_allowed_from_local_main() {
    let harness = Harness::new().await;
    let other = tauri::WebviewWindowBuilder::new(&harness.app, "engines-other", Default::default())
        .build()
        .unwrap();
    for (window, url) in [
        (&other, local_url()),
        (&harness.main, "https://example.com/"),
    ] {
        for command in [
            "add_engine",
            "update_engine",
            "remove_engine",
            "reorder_engines",
            "select_main_engine",
            "restart_engine",
            "close_engine",
            "set_engine_settings",
            "preview_java_import",
            "confirm_java_import",
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
