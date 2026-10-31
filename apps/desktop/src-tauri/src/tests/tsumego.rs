use super::*;
use app_core::{TsumegoOptions, TsumegoStatus};

#[tokio::test]
async fn tsumego_commands_generate_report_and_restore_through_ipc() {
    let mut harness = Harness::new().await;
    harness.subscribe(81).await;
    harness.document(81, 0).await;
    harness
        .invoke(
            "open_bytes",
            json!({ "bytes": b"(;SZ[19]KM[6.5]C[source];B[bb];W[ab])".as_slice() }),
        )
        .await;
    harness.document(81, 1).await;
    harness.invoke("step", json!({ "count": 2 })).await;
    let original = harness.document(81, 2).await;
    let options = TsumegoOptions::default();
    assert_eq!(
        harness
            .invoke("generate_tsumego", json!({ "options": options }))
            .await,
        3
    );
    let generated = harness.document(81, 3).await;
    assert_eq!(generated.tree.root, generated.tree.current);
    assert_eq!(generated.tree.main_line.len(), 1);
    assert_eq!(generated.board.as_ref().unwrap().move_number, 0);
    let status: TsumegoStatus =
        serde_json::from_value(harness.invoke("tsumego_status", json!({})).await).unwrap();
    assert!(status.active);
    assert_eq!(status.revision, 3);
    assert_eq!(status.source_node, Some(original.tree.current));
    assert_eq!(status.options, options);
    assert_eq!(
        harness.invoke("settings", json!({})).await["tsumego"],
        json!(options)
    );
    assert_eq!(harness.invoke("exit_tsumego", json!({})).await, 4);
    let restored = harness.document(81, 4).await;
    assert_eq!(restored.board, original.board);
    assert_eq!(restored.tree.current, original.tree.current);
    assert_eq!(restored.tree.main_line, original.tree.main_line);
    assert_eq!(restored.game_info, original.game_info);
    assert_eq!(
        harness.invoke("tsumego_status", json!({})).await["active"],
        false
    );
    assert_eq!(harness.invoke("exit_tsumego", json!({})).await, 4);
    let error = request(
        &harness.main,
        "generate_tsumego",
        json!({ "options": {
        "to_play": "Inherit", "attacker": "Auto", "ko_threat": "Both", "wall_distance": 0
    } }),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(error["type"], "App");
    harness.finish().await;
}

#[tokio::test]
async fn tsumego_commands_are_denied_to_other_windows_and_remote_origins() {
    let harness = Harness::new().await;
    let other = tauri::WebviewWindowBuilder::new(&harness.app, "other", Default::default())
        .build()
        .unwrap();
    for command in ["generate_tsumego", "exit_tsumego", "tsumego_status"] {
        for (window, url) in [
            (&other, local_url()),
            (&harness.main, "https://example.com/"),
        ] {
            let error = request(
                window,
                command,
                json!({ "options": TsumegoOptions::default() }),
                url,
            )
            .await
            .unwrap_err();
            assert!(
                error
                    .as_str()
                    .is_some_and(|message| message.contains("not allowed")),
                "{command}: {error}"
            );
        }
    }
    harness.finish().await;
}
