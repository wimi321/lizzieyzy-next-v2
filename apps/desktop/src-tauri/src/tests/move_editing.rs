use super::*;

fn move_at(color: &str, x: u8, y: u8) -> Value {
    json!({"type": "Play", "data": {"color": color, "point": {"x": x, "y": y}}})
}

fn tree_content(tree: &Value) -> Value {
    json!({"root": tree["root"], "nodes": tree["nodes"]})
}

fn document_content(view: &DocumentView) -> Value {
    json!({
        "board": view.board, "current": view.tree.current, "comment": view.comment,
        "game_info": view.game_info, "markup": view.markup,
        "modified": view.modified, "file_name": view.file_name, "file_path": view.file_path,
    })
}

#[tokio::test]
async fn move_editing_mutations_execute_and_undo_redo_through_ipc() {
    let mut harness = Harness::new().await;
    harness.subscribe(231).await;
    harness.document(231, 0).await;
    let source = b"(;SZ[9];B[aa];W[bb](;B[cc];W[dd])(;B[ee];W[ff]))";
    let cases = [
        ("play_colored", json!({"mv": move_at("White", 7, 7)})),
        ("insert_move", json!({"mv": move_at("Black", 7, 7)})),
        ("switch_stone", json!({"point": {"x": 0, "y": 0}})),
        ("delete_stone", json!({"point": {"x": 0, "y": 0}})),
        (
            "move_stone",
            json!({"from": {"x": 0, "y": 0}, "to": {"x": 7, "y": 7}}),
        ),
        (
            "add_variation",
            json!({"moves": [move_at("Black", 6, 6), move_at("White", 6, 7)]}),
        ),
    ];
    let mut revision = 0;
    for (command, args) in cases {
        revision += 1;
        assert_eq!(
            harness
                .invoke("open_bytes", json!({"bytes": source.as_slice()}))
                .await,
            revision
        );
        harness.document(231, revision).await;
        revision += 1;
        assert_eq!(harness.invoke("step", json!({"count": 2})).await, revision);
        let before = harness.document(231, revision).await;
        let tree_before = harness.invoke("variation_tree", json!({})).await;
        revision += 1;
        assert_eq!(harness.invoke(command, args).await, revision, "{command}");
        let after = harness.document(231, revision).await;
        let tree_after = harness.invoke("variation_tree", json!({})).await;
        assert_ne!(tree_content(&tree_before), tree_content(&tree_after));
        assert!(after.can_undo, "{command}");
        assert!(after.modified, "{command}");
        revision += 1;
        assert_eq!(harness.invoke("undo", json!({})).await, revision);
        let undone = harness.document(231, revision).await;
        assert_eq!(
            document_content(&undone),
            document_content(&before),
            "{command}"
        );
        assert_eq!(
            tree_content(&harness.invoke("variation_tree", json!({})).await),
            tree_content(&tree_before),
            "{command}"
        );
        assert!(undone.can_redo);
        revision += 1;
        assert_eq!(harness.invoke("redo", json!({})).await, revision);
        let redone = harness.document(231, revision).await;
        assert_eq!(
            document_content(&redone),
            document_content(&after),
            "{command}"
        );
        assert_eq!(
            tree_content(&harness.invoke("variation_tree", json!({})).await),
            tree_content(&tree_after),
            "{command}"
        );
    }
    harness.finish().await;
}

#[tokio::test]
async fn origin_and_trial_commands_execute_without_losing_original_document() {
    let mut harness = Harness::new().await;
    harness.subscribe(232).await;
    harness.document(232, 0).await;
    assert_eq!(
        harness
            .invoke("open_bytes", json!({"bytes": b"(;SZ[9];B[aa];W[bb])"}))
            .await,
        1
    );
    harness.document(232, 1).await;
    assert_eq!(harness.invoke("step", json!({"count": 2})).await, 2);
    let before = harness.document(232, 2).await;
    let origin = harness
        .invoke("stone_origin", json!({"point": {"x": 0, "y": 0}}))
        .await;
    assert_eq!(origin["revision"], 2);
    assert_eq!(origin["node_id"], before.tree.main_line[1]);
    assert_eq!(origin["move_number"], 1);
    assert_eq!(origin["review_parent"], before.tree.root);
    assert_eq!(
        harness
            .invoke("stone_origin", json!({"point": {"x": 8, "y": 8}}))
            .await,
        Value::Null
    );
    assert_eq!(harness.invoke("step", json!({"count": 0})).await, 2);
    let tree_before = harness.invoke("variation_tree", json!({})).await;
    assert_eq!(harness.invoke("start_trial", json!({})).await, 3);
    let trial = harness.document(232, 3).await;
    assert!(trial.trial);
    assert_eq!(
        trial.board.as_ref().unwrap().stones,
        before.board.as_ref().unwrap().stones
    );
    assert_eq!(harness.invoke("start_trial", json!({})).await, 3);
    assert_eq!(
        harness
            .invoke("play_colored", json!({"mv": move_at("White", 7, 7)}))
            .await,
        4
    );
    let played = harness.document(232, 4).await;
    assert!(played.trial);
    assert!(played.can_undo);
    assert_eq!(harness.invoke("undo", json!({})).await, 5);
    let undone = harness.document(232, 5).await;
    assert_eq!(undone.board, trial.board);
    assert!(undone.trial);
    assert_eq!(harness.invoke("redo", json!({})).await, 6);
    assert_eq!(harness.document(232, 6).await.board, played.board);
    assert_eq!(harness.invoke("end_trial", json!({})).await, 7);
    let restored = harness.document(232, 7).await;
    assert!(!restored.trial);
    assert_eq!(document_content(&restored), document_content(&before));
    assert_eq!(restored.can_undo, before.can_undo);
    assert_eq!(restored.can_redo, before.can_redo);
    assert_eq!(
        tree_content(&harness.invoke("variation_tree", json!({})).await),
        tree_content(&tree_before)
    );
    assert_eq!(harness.invoke("end_trial", json!({})).await, 7);
    harness.finish().await;
}

#[tokio::test]
async fn move_editing_rejects_invalid_inputs_and_denies_non_local_callers() {
    let harness = Harness::new().await;
    harness
        .invoke("play_colored", json!({"mv": move_at("Black", 0, 0)}))
        .await;
    for (command, args) in [
        ("play_colored", json!({"mv": move_at("White", 0, 0)})),
        ("insert_move", json!({"mv": move_at("White", 0, 0)})),
        ("switch_stone", json!({"point": {"x": 1, "y": 1}})),
        ("delete_stone", json!({"point": {"x": 1, "y": 1}})),
        (
            "move_stone",
            json!({"from": {"x": 1, "y": 1}, "to": {"x": 2, "y": 2}}),
        ),
        ("add_variation", json!({"moves": [move_at("White", 0, 0)]})),
        ("stone_origin", json!({"point": {"x": 25, "y": 0}})),
    ] {
        let error = request(&harness.main, command, args, local_url())
            .await
            .unwrap_err();
        assert_eq!(error["type"], "App", "{command}: {error}");
    }
    for (command, args) in [
        (
            "play_colored",
            json!({"mv": {"type": "Pass", "data": {"color": "Blue"}}}),
        ),
        ("insert_move", json!({"mv": {"type": "Unknown"}})),
        ("switch_stone", json!({"point": {"x": -1, "y": 0}})),
        ("delete_stone", json!({"point": {"x": "a", "y": 0}})),
        ("move_stone", json!({"from": {"x": 0, "y": 0}})),
        ("add_variation", json!({"moves": "not a list"})),
        ("stone_origin", json!({"point": null})),
    ] {
        assert!(
            request(&harness.main, command, args, local_url())
                .await
                .is_err(),
            "{command}"
        );
    }
    let other = tauri::WebviewWindowBuilder::new(&harness.app, "move-other", Default::default())
        .build()
        .unwrap();
    for command in [
        "play_colored",
        "insert_move",
        "switch_stone",
        "delete_stone",
        "move_stone",
        "add_variation",
        "stone_origin",
        "start_trial",
        "end_trial",
    ] {
        for (window, url) in [
            (&other, local_url()),
            (&harness.main, "https://example.com"),
        ] {
            let error = request(window, command, json!({}), url).await.unwrap_err();
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
