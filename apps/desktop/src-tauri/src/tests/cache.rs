use super::{Harness, json, local_url, request};
use app_core::Settings;

#[tokio::test]
async fn cache_commands_forward_settings_and_undoable_clearing() {
    let mut harness = Harness::new().await;
    assert_eq!(harness.invoke("cache_enabled", json!({})).await, true);
    assert_eq!(
        harness
            .invoke("set_cache_enabled", json!({ "enabled": false }))
            .await,
        false
    );
    assert!(
        !Settings::load(harness.directory.path())
            .await
            .unwrap()
            .cache_enabled
    );
    assert_eq!(
        harness.invoke("settings", json!({})).await["cache_enabled"],
        false
    );
    harness.subscribe(92).await;
    harness.document(92, 0).await;
    let bytes: &[u8] = b"(;SZ[5]LZ[Main 45 100 2];B[aa]LZ[Main 40 200 3])";
    harness
        .invoke("open_bytes", json!({ "bytes": bytes }))
        .await;
    let loaded = harness.document(92, 1).await;
    assert!(loaded.graph.points[0].summary.is_some());
    assert!(loaded.graph.points[1].summary.is_some());
    for all in [false, true] {
        assert!(
            harness
                .invoke("clear_cache", json!({ "all": all }))
                .await
                .is_null()
        );
        assert!(harness.core.whole_game_result(0).await.unwrap().is_some());
    }
    let revision = harness
        .invoke("clear_analysis", json!({ "all": false }))
        .await;
    let cleared = harness.document(92, revision.as_u64().unwrap()).await;
    assert!(cleared.graph.points[0].summary.is_none());
    assert!(cleared.graph.points[1].summary.is_some());
    assert!(cleared.can_undo);
    let revision = harness.invoke("undo", json!({})).await;
    let restored = harness.document(92, revision.as_u64().unwrap()).await;
    assert!(restored.graph.points[0].summary.is_some());
    let revision = harness
        .invoke("clear_analysis", json!({ "all": true }))
        .await;
    let cleared = harness.document(92, revision.as_u64().unwrap()).await;
    assert!(
        cleared
            .graph
            .points
            .iter()
            .all(|point| point.summary.is_none())
    );
    harness.finish().await;
}

#[tokio::test]
async fn cache_setting_write_error_preserves_enabled_state() {
    let harness = Harness::new().await;
    std::fs::create_dir(harness.directory.path().join("settings.json")).unwrap();
    let error = request(
        &harness.main,
        "set_cache_enabled",
        json!({ "enabled": false }),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(error["type"], "App");
    assert_eq!(harness.invoke("cache_enabled", json!({})).await, true);
    harness.finish().await;
}

#[tokio::test]
async fn cache_commands_are_local_main_only() {
    let harness = Harness::new().await;
    let other = tauri::WebviewWindowBuilder::new(&harness.app, "cache-other", Default::default())
        .build()
        .unwrap();
    for (window, url) in [
        (&other, local_url()),
        (&harness.main, "https://example.com/"),
    ] {
        for (command, arguments) in [
            ("cache_enabled", json!({})),
            ("set_cache_enabled", json!({ "enabled": false })),
            ("clear_cache", json!({ "all": true })),
            ("clear_analysis", json!({ "all": true })),
        ] {
            let error = request(window, command, arguments, url).await.unwrap_err();
            assert!(
                error
                    .as_str()
                    .is_some_and(|message| message.contains("not allowed"))
            );
        }
    }
    harness.finish().await;
}
