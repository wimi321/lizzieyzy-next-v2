use super::{Harness, json, local_url, request, within};

#[tokio::test]
async fn save_slot_commands_round_trip_through_ipc() {
    let mut harness = within(Harness::new()).await;
    within(harness.subscribe(124)).await;
    within(harness.document(124, 0)).await;
    assert_eq!(
        within(harness.invoke("save_slots", json!({}))).await,
        json!([])
    );
    assert_eq!(
        within(harness.invoke("set_autosave", json!({"enabled": false}))).await,
        false
    );
    assert_eq!(
        within(harness.invoke("set_restore_last_game", json!({"enabled": true}))).await,
        true
    );
    let settings = within(harness.invoke("settings", json!({}))).await;
    assert_eq!(settings["autosave"], false);
    assert_eq!(settings["restore_last_game"], true);
    let bytes: &[u8] = b"(;SZ[5];B[aa];W[bb])";
    let revision = within(harness.invoke("open_bytes", json!({"bytes": bytes}))).await;
    within(harness.document(124, revision.as_u64().unwrap())).await;
    let revision = within(harness.invoke("step", json!({"count": 1}))).await;
    let original = within(harness.document(124, revision.as_u64().unwrap())).await;
    let slot = within(harness.invoke("save_to_new_slot", json!({"name": "  study  "}))).await;
    let id = slot["id"].as_str().unwrap();
    assert_eq!(slot["name"], "study");
    assert_eq!(slot["move_number"], 1);
    assert_eq!(slot["is_autosave"], false);
    assert_eq!(slot["corrupt"], false);
    let renamed = within(harness.invoke("rename_slot", json!({"id": id, "name": "review"}))).await;
    assert_eq!(renamed["name"], "review");
    let overwritten = within(harness.invoke("overwrite_slot", json!({"id": id}))).await;
    assert_eq!(overwritten["id"], id);
    let revision = within(harness.invoke("step", json!({"count": 1}))).await;
    within(harness.document(124, revision.as_u64().unwrap())).await;
    let revision = within(harness.invoke("load_slot", json!({"id": id}))).await;
    let loaded = within(harness.document(124, revision.as_u64().unwrap())).await;
    assert_eq!(loaded.board, original.board);
    let error = within(request(
        &harness.main,
        "rename_slot",
        json!({"id": id, "name": ""}),
        local_url(),
    ))
    .await
    .unwrap_err();
    assert_eq!(
        error,
        json!({"type": "SaveSlots", "data": {"type": "InvalidName"}})
    );
    within(harness.invoke("delete_slot", json!({"id": id}))).await;
    assert_eq!(
        within(harness.invoke("save_slots", json!({}))).await,
        json!([])
    );
    within(harness.invoke("save_to_new_slot", json!({"name": "second"}))).await;
    within(harness.invoke("delete_all_slots", json!({}))).await;
    assert_eq!(
        within(harness.invoke("save_slots", json!({}))).await,
        json!([])
    );
    within(harness.finish()).await;
}

#[tokio::test]
async fn save_slot_commands_are_local_main_only() {
    let harness = within(Harness::new()).await;
    let other = tauri::WebviewWindowBuilder::new(&harness.app, "saves-other", Default::default())
        .build()
        .unwrap();
    for (window, url) in [
        (&other, local_url()),
        (&harness.main, "https://example.invalid/"),
    ] {
        for (command, arguments) in [
            ("save_slots", json!({})),
            ("save_to_new_slot", json!({"name": ""})),
            ("overwrite_slot", json!({"id": "slot"})),
            ("rename_slot", json!({"id": "slot", "name": "name"})),
            ("load_slot", json!({"id": "slot"})),
            ("delete_slot", json!({"id": "slot"})),
            ("delete_all_slots", json!({})),
            ("set_autosave", json!({"enabled": false})),
            ("set_restore_last_game", json!({"enabled": true})),
        ] {
            let error = within(request(window, command, arguments, url))
                .await
                .unwrap_err();
            assert!(
                error
                    .as_str()
                    .is_some_and(|message| message.contains("not allowed"))
            );
        }
    }
    within(harness.finish()).await;
}

#[tokio::test]
async fn research_display_settings_have_typed_ipc_range_errors() {
    let harness = within(Harness::new()).await;
    let display = json!({
        "autoplay_interval_ms": 100,
        "painting_color": "Blue",
        "control_panel_bounds": {"x": -1920, "y": 20, "width": 400, "height": 600}
    });
    let saved = within(harness.invoke("set_display_settings", json!({"display": display}))).await;
    assert_eq!(saved["autoplay_interval_ms"], 100);
    assert_eq!(saved["painting_color"], "Blue");
    assert_eq!(
        saved["control_panel_bounds"],
        display["control_panel_bounds"]
    );
    for (invalid, kind) in [
        (json!({"autoplay_interval_ms": 99}), "OutOfRange"),
        (
            json!({"control_panel_bounds": {"x": 0, "y": 0, "width": 0, "height": 400}}),
            "InvalidControlPanelBounds",
        ),
    ] {
        let error = within(request(
            &harness.main,
            "set_display_settings",
            json!({"display": invalid}),
            local_url(),
        ))
        .await
        .unwrap_err();
        assert_eq!(error["type"], "DisplaySettings");
        assert_eq!(error["data"]["type"], kind);
        assert_eq!(
            within(harness.invoke("settings", json!({}))).await["display"],
            saved
        );
    }
    within(harness.finish()).await;
}
