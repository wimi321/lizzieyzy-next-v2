use super::*;
use std::collections::BTreeSet;
use tauri::ipc::Origin;

const FLOATING_COMMANDS: &[&str] = &[
    "settings",
    "variation_tree",
    "subscribe",
    "unsubscribe",
    "play",
    "step",
    "go_to",
    "preview_variation",
    "set_analysis_restriction",
    "add_restriction_points",
    "remove_restriction_points",
    "clear_analysis_restriction",
    "analysis_state",
    "set_pondering",
    "toggle_pondering",
    "close_floating_board",
    "set_floating_board_options",
    "plugin:window|start_dragging",
    "plugin:window|start_resize_dragging",
    "plugin:window|set_size",
];

#[test]
fn floating_capability_allows_only_board_operations_on_local_float_labels() {
    let mut context = crate::context::<MockRuntime>();
    let authority = context.runtime_authority_mut();
    for label in ["float-main", "float-sub"] {
        for command in FLOATING_COMMANDS {
            assert!(
                authority
                    .resolve_access(command, label, label, &Origin::Local)
                    .is_some(),
                "missing {command} for {label}"
            );
            assert!(
                authority
                    .resolve_access(
                        command,
                        label,
                        label,
                        &Origin::Remote {
                            url: "https://example.com".parse().unwrap()
                        },
                    )
                    .is_none(),
                "remote {command} unexpectedly allowed"
            );
        }
        for command in [
            "open_bytes",
            "open_path",
            "save",
            "copy_sgf",
            "paste_sgf",
            "save_png",
            "open_url",
            "fox_search",
            "tencent_search",
            "zhizi_login_password",
            "check_update",
            "automatic_update_check",
            "download_update",
            "install_update",
            "configure_engine",
            "detect_engines",
            "auto_configure_engine",
            "add_engine",
            "try_configure_engine",
            "start_asset_download",
            "gtp_console_send",
            "start_commentary",
            "set_commentary_key",
            "read_remote_link_qr",
            "set_display_settings",
            "set_analysis_limits",
            "set_search_params",
            "set_restriction_depth",
            "set_pure_network",
            "set_analysis_extras",
            "open_floating_board",
            "set_floating_layout",
            "apply_window_layout",
            "reset_window_positions",
            "fit_main_window",
            "set_always_on_top",
            "set_language",
            "start_board_sync",
            "set_comment",
            "undo",
            "redo",
            "new_game",
            "load_slot",
            "delete_all_slots",
            "export_diagnostics",
            "reveal_in_folder",
            "plugin:window|close",
            "plugin:window|set_title",
            "plugin:window|set_position",
            "plugin:window|set_always_on_top",
            "plugin:dialog|open",
            "plugin:dialog|save",
            "plugin:fs|read_file",
            "plugin:shell|execute",
            "plugin:opener|open_url",
        ] {
            assert!(
                authority
                    .resolve_access(command, label, label, &Origin::Local)
                    .is_none(),
                "unneeded {command} allowed for {label}"
            );
        }
    }
    for label in ["other", "float-other", "float-main-2"] {
        for command in FLOATING_COMMANDS {
            assert!(
                authority
                    .resolve_access(command, label, label, &Origin::Local)
                    .is_none()
            );
        }
    }
    for command in [
        "fit_main_window",
        "reset_window_positions",
        "open_floating_board",
        "close_floating_board",
        "set_floating_board_options",
        "set_floating_layout",
        "apply_window_layout",
    ] {
        assert!(
            authority
                .resolve_access(command, "main", "main", &Origin::Local)
                .is_some()
        );
    }
}

async fn invoke(window: &WebviewWindow<MockRuntime>, command: &str, args: Value) -> Value {
    request(window, command, args, local_url())
        .await
        .unwrap_or_else(|error| panic!("{command}: {error}"))
}

async fn subscribe(window: &WebviewWindow<MockRuntime>, callback: u32) -> u32 {
    serde_json::from_value(
        invoke(
            window,
            "subscribe",
            json!({"channel": format!("__CHANNEL__:{callback}")}),
        )
        .await,
    )
    .unwrap()
}

async fn documents(harness: &mut Harness, callbacks: &[u32], revision: u64) {
    within(async {
        let mut pending: BTreeSet<_> = callbacks.iter().copied().collect();
        while !pending.is_empty() {
            let message = harness.next().await;
            if matches!(message.event, AppEvent::DocumentChanged { revision: actual, .. } if actual == revision) {
                assert!(pending.remove(&message.callback), "unexpected callback {}", message.callback);
            }
        }
    }).await;
}

#[tokio::test]
async fn floating_channels_broadcast_replace_and_unsubscribe_per_webview() {
    let mut harness = Harness::new().await;
    let main = harness.main.clone();
    let floating_main =
        tauri::WebviewWindowBuilder::new(&harness.app, "float-main", Default::default())
            .build()
            .unwrap();
    let floating_sub =
        tauri::WebviewWindowBuilder::new(&harness.app, "float-sub", Default::default())
            .build()
            .unwrap();
    let main_id = subscribe(&main, 701).await;
    documents(&mut harness, &[701], 0).await;
    let old_float_id = subscribe(&floating_main, 702).await;
    documents(&mut harness, &[702], 0).await;
    let sub_id = subscribe(&floating_sub, 703).await;
    documents(&mut harness, &[703], 0).await;
    assert_eq!(
        invoke(
            &floating_main,
            "play",
            json!({"mv": {"type":"Pass", "data":{"color":"Black"}}})
        )
        .await,
        1
    );
    documents(&mut harness, &[701, 702, 703], 1).await;

    let new_float_id = subscribe(&floating_main, 704).await;
    documents(&mut harness, &[704], 1).await;
    assert_ne!(old_float_id, new_float_id);
    // A stale ID and another webview's current ID must not remove either owner.
    invoke(&floating_main, "unsubscribe", json!({"id": old_float_id})).await;
    invoke(&floating_main, "unsubscribe", json!({"id": main_id})).await;
    invoke(&main, "unsubscribe", json!({"id": new_float_id})).await;
    assert_eq!(
        invoke(
            &floating_sub,
            "play",
            json!({"mv": {"type":"Pass", "data":{"color":"White"}}})
        )
        .await,
        2
    );
    documents(&mut harness, &[701, 703, 704], 2).await;

    invoke(&floating_sub, "unsubscribe", json!({"id": sub_id})).await;
    assert_eq!(
        invoke(&floating_main, "step", json!({"count": -1})).await,
        3
    );
    documents(&mut harness, &[701, 704], 3).await;
    let reconnected = subscribe(&floating_sub, 705).await;
    documents(&mut harness, &[705], 3).await;
    assert_ne!(reconnected, sub_id);
    for (window, id) in [
        (&main, main_id),
        (&floating_main, new_float_id),
        (&floating_sub, reconnected),
    ] {
        invoke(window, "unsubscribe", json!({"id": id})).await;
    }
    // Acknowledged unsubscribe calls are barriers after the completed broadcast.
    while let Ok(message) = harness.output.try_recv() {
        assert!(
            !matches!(message.event, AppEvent::DocumentChanged { revision: 3, .. })
                || message.callback != 703
        );
    }
    harness.finish().await;
}

#[tokio::test]
async fn floating_board_commands_reach_core_but_file_and_engine_commands_are_denied() {
    let harness = Harness::new().await;
    let window = tauri::WebviewWindowBuilder::new(&harness.app, "float-main", Default::default())
        .build()
        .unwrap();
    assert!(invoke(&window, "settings", json!({})).await.is_object());
    assert!(
        invoke(&window, "variation_tree", json!({}))
            .await
            .is_object()
    );
    assert_eq!(
        invoke(
            &window,
            "play",
            json!({"mv":{"type":"Play","data":{"color":"Black","point":{"x":3,"y":3}}}})
        )
        .await,
        1
    );
    assert_eq!(invoke(&window, "step", json!({"count":-1})).await, 2);
    assert_eq!(invoke(&window, "go_to", json!({"nodeId":1})).await, 3);
    let preview = invoke(&window, "preview_variation", json!({"nodeId":1,"moves":[]})).await;
    assert_eq!(preview["applied_moves"], json!([]));
    invoke(
        &window,
        "set_analysis_restriction",
        json!({"kind":"Allow","points":[{"x":2,"y":2}]}),
    )
    .await;
    invoke(
        &window,
        "add_restriction_points",
        json!({"kind":"Allow","points":[{"x":4,"y":4}]}),
    )
    .await;
    invoke(
        &window,
        "remove_restriction_points",
        json!({"points":[{"x":2,"y":2}]}),
    )
    .await;
    invoke(&window, "clear_analysis_restriction", json!({})).await;
    invoke(&window, "set_pondering", json!({"enabled":false})).await;
    assert_eq!(
        invoke(&window, "analysis_state", json!({})).await["pondering"],
        false
    );
    assert_eq!(
        invoke(&window, "toggle_pondering", json!({})).await["pondering"],
        true
    );
    for command in [
        "open_path",
        "configure_engine",
        "check_update",
        "set_display_settings",
        "open_floating_board",
        "apply_window_layout",
    ] {
        let error = request(&window, command, json!({}), local_url())
            .await
            .unwrap_err();
        assert!(
            error
                .as_str()
                .is_some_and(|message| message.contains("not allowed")),
            "{error}"
        );
    }
    harness.finish().await;
}
