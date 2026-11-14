use super::{Harness, json, local_url, request};

#[tokio::test]
async fn remote_ipc_imports_links_and_qr_and_preserves_typed_failures() {
    let harness = Harness::new().await;
    let normalized = harness
        .invoke(
            "normalize_remote_link",
            json!({"text": "链接：＂ｗｓ：／／127.0.0.1:12345/analysis＂"}),
        )
        .await;
    assert_eq!(normalized, "ws://127.0.0.1:12345/analysis");
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/remote/link.jpg");
    let decoded = harness
        .invoke("read_remote_link_qr", json!({"path": path}))
        .await;
    assert_eq!(decoded, "ws://127.0.0.1:12345/analysis?token=fixture-only");
    let invalid = request(
        &harness.main,
        "test_remote_engine",
        json!({
            "entry": {"type": "WebSocket", "data": {"url": "https://127.0.0.1/invalid"}}
        }),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(invalid["type"], "RemoteLink");
    assert_eq!(invalid["data"], "InvalidLink");
    let no_local = request(
        &harness.main,
        "switch_to_local_engine",
        json!({}),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(no_local["type"], "Remote");
    assert_eq!(no_local["data"]["type"], "NoLocalEngine");
    harness.finish().await;
}

#[tokio::test]
async fn remote_ipc_adds_and_updates_without_persisting_credentials() {
    // The harness installs no operating-system credential store and never connects.
    let harness = Harness::new().await;
    let added = harness.invoke("add_remote_engine", json!({
        "name": "LAN", "entry": {"type": "WebSocket", "data": {"url": "ws://127.0.0.1:12345/private-link-token"}}
    })).await;
    let id = added["engines"][0]["id"].as_str().unwrap();
    assert!(added["engine"].is_null());
    assert_eq!(
        added["engines"][0]["profile"]["remote"]["type"],
        "WebSocket"
    );
    let updated = harness.invoke("update_remote_engine", json!({
        "id": id, "name": "SSH", "entry": {"type": "Ssh", "data": {
            "host": "127.0.0.1", "port": 12345, "user": "fixture", "command": "katago analysis",
            "auth": {"type": "Password", "data": {"password": "private-password-token"}}
        }}
    })).await;
    assert_eq!(updated["engines"][0]["id"], id);
    assert_eq!(updated["engines"][0]["name"], "SSH");
    assert_eq!(updated["engines"][0]["profile"]["remote"]["type"], "Ssh");
    let persisted =
        std::fs::read_to_string(harness.directory.path().join("settings.json")).unwrap();
    for secret in ["private-link-token", "private-password-token"] {
        assert!(!persisted.contains(secret));
        assert!(!updated.to_string().contains(secret));
    }
    harness.finish().await;
}

#[tokio::test]
async fn remote_commands_only_allow_the_local_main_window() {
    let harness = Harness::new().await;
    let other = tauri::WebviewWindowBuilder::new(&harness.app, "remote-other", Default::default())
        .build()
        .unwrap();
    for command in [
        "add_remote_engine",
        "update_remote_engine",
        "test_remote_engine",
        "switch_to_local_engine",
        "normalize_remote_link",
        "read_remote_link_qr",
    ] {
        for (window, url) in [(&other, local_url()), (&harness.main, "https://127.0.0.1/")] {
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
