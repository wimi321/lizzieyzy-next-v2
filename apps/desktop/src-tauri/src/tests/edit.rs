use super::*;

fn new_game_options() -> Value {
    json!({
        "width": 13, "height": 13, "komi": 0.5, "handicap": 4, "rules": "japanese",
        "black_name": "黑方", "black_rank": "3d", "white_name": null, "white_rank": "1d"
    })
}

fn game_info() -> Value {
    json!({
        "black_name": "甲", "black_rank": null, "white_name": "乙", "white_rank": "2d",
        "komi": 6.5, "rules": "chinese", "result": "B+R", "date": "2026-11-11", "event": "友谊赛"
    })
}

#[tokio::test]
async fn new_game_info_comment_and_recent_files_round_trip_through_ipc() {
    let mut harness = Harness::new().await;
    harness.subscribe(211).await;
    harness.document(211, 0).await;
    assert_eq!(harness.invoke("recent_files", json!({})).await, json!([]));
    assert_eq!(
        harness
            .invoke("new_game", json!({"options": new_game_options()}))
            .await,
        1
    );
    let created = harness.document(211, 1).await;
    assert_eq!(created.file_path, None);
    assert!(!created.modified);
    assert_eq!(created.game_info.black_name.as_deref(), Some("黑方"));
    assert_eq!(created.game_info.black_rank.as_deref(), Some("3d"));
    assert_eq!(created.game_info.white_name, None);
    assert_eq!(created.game_info.white_rank.as_deref(), Some("1d"));
    assert_eq!(created.game_info.handicap, Some(4));
    assert_eq!(created.game_info.komi, 0.5);
    let board = created.board.unwrap();
    assert_eq!((board.width, board.height), (13, 13));
    assert_eq!(board.stones.iter().flatten().count(), 4);
    assert_eq!(
        harness
            .invoke("set_game_info", json!({"info": game_info()}))
            .await,
        2
    );
    let edited = harness.document(211, 2).await;
    assert!(edited.modified);
    assert_eq!(edited.game_info.black_name.as_deref(), Some("甲"));
    assert_eq!(edited.game_info.black_rank, None);
    assert_eq!(edited.game_info.white_name.as_deref(), Some("乙"));
    assert_eq!(edited.game_info.white_rank.as_deref(), Some("2d"));
    assert_eq!(edited.game_info.komi, 6.5);
    assert_eq!(edited.game_info.rules.as_deref(), Some("chinese"));
    assert_eq!(edited.game_info.result.as_deref(), Some("B+R"));
    assert_eq!(edited.game_info.date.as_deref(), Some("2026-11-11"));
    assert_eq!(edited.game_info.event.as_deref(), Some("友谊赛"));
    assert_eq!(
        harness.invoke("set_comment", json!({"text": "评论"})).await,
        3
    );
    let commented = harness.document(211, 3).await;
    assert_eq!(commented.comment.as_deref(), Some("评论"));
    assert_eq!(
        commented.tree.structure_revision,
        edited.tree.structure_revision
    );
    assert!(commented.tree.comment_revision > edited.tree.comment_revision);
    let tree = harness.invoke("variation_tree", json!({})).await;
    assert_eq!(tree["comment_revision"], commented.tree.comment_revision);
    assert_eq!(tree["nodes"][0]["has_comment"], true);
    assert_eq!(harness.invoke("set_comment", json!({"text": ""})).await, 4);
    assert_eq!(harness.document(211, 4).await.comment, None);
    let path = harness.directory.path().join("编辑棋谱.sgf");
    assert_eq!(
        harness.invoke("save", json!({"path": path})).await,
        Value::Null
    );
    let saved = harness.document(211, 4).await;
    assert!(!saved.modified);
    assert_eq!(saved.file_path.as_deref(), path.to_str());
    assert_eq!(saved.file_name.as_deref(), Some("编辑棋谱.sgf"));
    assert_eq!(
        harness.invoke("recent_files", json!({})).await,
        json!([path])
    );
    assert_eq!(
        harness.invoke("clear_recent_files", json!({})).await,
        Value::Null
    );
    assert_eq!(harness.invoke("recent_files", json!({})).await, json!([]));
    harness.finish().await;
}

#[tokio::test]
async fn node_branch_and_main_line_commands_return_revisions_and_undo_as_one_edit() {
    let mut harness = Harness::new().await;
    harness.subscribe(212).await;
    harness.document(212, 0).await;
    let sgf = b"(;SZ[9](;B[aa];W[ab])(;B[cc](;W[cd])(;W[ce])))";
    assert_eq!(
        harness
            .invoke("open_bytes", json!({"bytes": sgf.as_slice()}))
            .await,
        1
    );
    let opened = harness.document(212, 1).await;
    let tree = harness.invoke("variation_tree", json!({})).await;
    let nodes = tree["nodes"].as_array().unwrap();
    let branch = nodes[0]["children"][1].as_u64().unwrap();
    let leaf = nodes.iter().find(|node| node["id"] == branch).unwrap()["children"][1]
        .as_u64()
        .unwrap();
    assert_eq!(
        harness
            .invoke("promote_to_main", json!({"nodeId": leaf}))
            .await,
        2
    );
    let promoted = harness.document(212, 2).await;
    assert_eq!(
        promoted.tree.main_line,
        vec![opened.tree.root, branch as u32, leaf as u32]
    );
    assert!(promoted.modified);
    assert_eq!(harness.invoke("undo", json!({})).await, 3);
    let undone = harness.document(212, 3).await;
    assert_eq!(undone.tree.main_line, opened.tree.main_line);
    assert!(!undone.modified);
    assert_eq!(harness.invoke("redo", json!({})).await, 4);
    assert_eq!(
        harness.document(212, 4).await.tree.main_line,
        promoted.tree.main_line
    );
    // Delete the original main line, which is now a side branch.
    let old_leaf = *opened.tree.main_line.last().unwrap();
    assert_eq!(
        harness
            .invoke("delete_branch", json!({"nodeId": old_leaf}))
            .await,
        5
    );
    harness.document(212, 5).await;
    let remaining = harness.invoke("variation_tree", json!({})).await;
    assert_eq!(remaining["nodes"][0]["children"], json!([branch]));
    assert_eq!(
        harness
            .invoke("delete_node", json!({"nodeId": branch}))
            .await,
        6
    );
    let deleted = harness.document(212, 6).await;
    assert_eq!(deleted.tree.main_line, vec![opened.tree.root]);
    assert_eq!(harness.invoke("undo", json!({})).await, 7);
    assert_eq!(
        harness.document(212, 7).await.tree.main_line,
        promoted.tree.main_line
    );
    harness.finish().await;
}

#[tokio::test]
async fn edit_commands_reject_invalid_parameters_and_non_local_callers() {
    let harness = Harness::new().await;
    for command in ["delete_node", "delete_branch", "promote_to_main"] {
        let error = request(
            &harness.main,
            command,
            json!({"nodeId": u32::MAX}),
            local_url(),
        )
        .await
        .unwrap_err();
        assert_eq!(error["type"], "App");
        assert!(
            request(
                &harness.main,
                command,
                json!({"nodeId": "bad"}),
                local_url()
            )
            .await
            .is_err()
        );
    }
    for command in ["delete_node", "delete_branch"] {
        assert!(
            request(&harness.main, command, json!({"nodeId": 0}), local_url())
                .await
                .is_err()
        );
    }
    for (field, invalid) in [("width", 1), ("height", 26), ("handicap", 10)] {
        let mut options = new_game_options();
        options[field] = json!(invalid);
        let error = request(
            &harness.main,
            "new_game",
            json!({"options": options}),
            local_url(),
        )
        .await
        .unwrap_err();
        assert_eq!(error["type"], "App");
    }
    let mut info = game_info();
    info["rules"] = json!("unknown");
    assert!(
        request(
            &harness.main,
            "set_game_info",
            json!({"info": info}),
            local_url()
        )
        .await
        .is_err()
    );
    assert!(
        request(
            &harness.main,
            "set_comment",
            json!({"text": 4}),
            local_url()
        )
        .await
        .is_err()
    );
    let other = tauri::WebviewWindowBuilder::new(&harness.app, "edit-other", Default::default())
        .build()
        .unwrap();
    for command in [
        "set_comment",
        "delete_node",
        "delete_branch",
        "promote_to_main",
        "new_game",
        "set_game_info",
        "recent_files",
        "clear_recent_files",
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
