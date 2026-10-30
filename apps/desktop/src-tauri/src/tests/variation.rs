use super::{Harness, json, local_url, request};

#[tokio::test]
async fn tree_and_preview_commands_preserve_document_and_return_captured_board() {
    let mut harness = Harness::new().await;
    harness.subscribe(71).await;
    harness.document(71, 0).await;
    let sgf = "(;SZ[5]PB[黑方]PW[白方]AB[ab][ba][cb]AW[bb]PL[B](;B[bc])(;B[ee]))";
    assert_eq!(
        harness
            .invoke("open_bytes", json!({ "bytes": sgf.as_bytes() }))
            .await,
        1
    );
    let document = harness.document(71, 1).await;
    assert_eq!(document.game_info.black_name.as_deref(), Some("黑方"));
    assert_eq!(document.game_info.white_name.as_deref(), Some("白方"));
    let root = document.tree.root;
    let tree = harness.invoke("variation_tree", json!({})).await;
    assert_eq!(tree["revision"], document.tree.structure_revision);
    assert_eq!(tree["root"], root);
    let nodes = tree["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 3);
    assert_eq!(nodes[0]["children"].as_array().unwrap().len(), 2);
    assert_eq!(nodes[1]["parent"], root);
    assert_eq!(nodes[2]["parent"], root);
    let preview = harness
        .invoke(
            "preview_variation",
            json!({ "nodeId": root, "moves": [
                { "type": "Play", "data": { "color": "Black", "point": { "x": 1, "y": 2 } } },
                { "type": "Pass", "data": { "color": "White" } },
                { "type": "Play", "data": { "color": "Black", "point": { "x": 1, "y": 2 } } }
            ] }),
        )
        .await;
    assert_eq!(preview["revision"], 1);
    assert_eq!(preview["node_id"], root);
    assert_eq!(preview["board"]["stones"][6], serde_json::Value::Null);
    assert_eq!(preview["board"]["stones"][11], "Black");
    assert_eq!(preview["board"]["captured_black"], 1);
    assert_eq!(preview["board"]["move_number"], 2);
    assert_eq!(preview["applied_moves"].as_array().unwrap().len(), 2);
    assert_eq!(
        preview["stones"],
        json!([
            { "color": "Black", "point": { "x": 1, "y": 2 }, "move_number": 1 }
        ])
    );
    assert!(preview["stop_reason"].is_string());
    assert_eq!(harness.invoke("step", json!({ "count": 0 })).await, 1);
    assert_eq!(harness.invoke("variation_tree", json!({})).await, tree);
    let error = request(&harness.main, "undo", json!({}), local_url())
        .await
        .unwrap_err();
    assert_eq!(error["type"], "App");
    assert!(
        error["data"]
            .as_str()
            .unwrap()
            .contains("no command to undo")
    );
    let error = request(
        &harness.main,
        "preview_variation",
        json!({ "nodeId": u32::MAX, "moves": [] }),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(error["type"], "App");
    // A fresh subscription replays the same document after both read commands.
    harness.subscribe(72).await;
    assert_eq!(harness.document(72, 1).await, document);
    harness.finish().await;
}
