use super::*;

fn position_content(view: &DocumentView) -> Value {
    json!({ "board": view.board, "markup": view.markup })
}

#[tokio::test]
async fn setup_and_markup_commands_execute_and_round_trip_history_through_ipc() {
    let mut harness = Harness::new().await;
    harness.subscribe(221).await;
    harness.document(221, 0).await;
    let source = b"(;SZ[9]AB[aa]AW[bb]PL[B]TR[cc]C[original])";
    let commands = [
        (
            "set_setup_stone",
            json!({"point": {"x": 3, "y": 4}, "color": "Black"}),
        ),
        (
            "set_setup_stone",
            json!({"point": {"x": 3, "y": 4}, "color": "White"}),
        ),
        (
            "set_setup_stone",
            json!({"point": {"x": 0, "y": 0}, "color": null}),
        ),
        ("clear_setup", json!({})),
        ("set_first_player", json!({"color": "White"})),
        ("toggle_to_play", json!({})),
        (
            "set_markup",
            json!({"point": {"x": 3, "y": 4}, "kind": "Triangle"}),
        ),
        (
            "set_markup",
            json!({"point": {"x": 3, "y": 4}, "kind": "Square"}),
        ),
        (
            "set_markup",
            json!({"point": {"x": 3, "y": 4}, "kind": "Circle"}),
        ),
        (
            "set_markup",
            json!({"point": {"x": 3, "y": 4}, "kind": "Cross"}),
        ),
        (
            "set_markup",
            json!({"point": {"x": 3, "y": 4}, "kind": "Letter"}),
        ),
        (
            "set_markup",
            json!({"point": {"x": 3, "y": 4}, "kind": "Number"}),
        ),
        (
            "set_markup",
            json!({"point": {"x": 2, "y": 2}, "kind": "Erase"}),
        ),
        ("clear_markup", json!({})),
    ];
    let mut revision = 0;
    for (command, args) in commands {
        revision += 1;
        assert_eq!(
            harness
                .invoke("open_bytes", json!({"bytes": source.as_slice()}))
                .await,
            revision
        );
        let before = harness.document(221, revision).await;
        revision += 1;
        assert_eq!(
            harness.invoke(command, args.clone()).await,
            revision,
            "{command}"
        );
        let after = harness.document(221, revision).await;
        assert_ne!(
            position_content(&after),
            position_content(&before),
            "{command}"
        );
        assert!(after.can_undo);
        assert!(after.modified);
        match command {
            "set_setup_stone" => {
                let x = args["point"]["x"].as_u64().unwrap() as usize;
                let y = args["point"]["y"].as_u64().unwrap() as usize;
                assert_eq!(
                    serde_json::to_value(after.board.as_ref().unwrap().stones[y * 9 + x]).unwrap(),
                    args["color"]
                );
            }
            "clear_setup" => assert!(
                after
                    .board
                    .as_ref()
                    .unwrap()
                    .stones
                    .iter()
                    .all(Option::is_none)
            ),
            "set_first_player" | "toggle_to_play" => assert_eq!(
                serde_json::to_value(after.board.as_ref().unwrap().to_play).unwrap(),
                "White"
            ),
            "set_markup" if args["kind"] != "Erase" => {
                let mark = after
                    .markup
                    .iter()
                    .find(|mark| mark.point.x == 3 && mark.point.y == 4)
                    .unwrap();
                assert_eq!(serde_json::to_value(mark.kind).unwrap(), args["kind"]);
                if args["kind"] == "Letter" {
                    assert_eq!(mark.text.as_deref(), Some("A"));
                }
                if args["kind"] == "Number" {
                    assert_eq!(mark.text.as_deref(), Some("1"));
                }
            }
            "set_markup" | "clear_markup" => assert!(after.markup.is_empty()),
            _ => unreachable!(),
        }
        revision += 1;
        assert_eq!(harness.invoke("undo", json!({})).await, revision);
        let undone = harness.document(221, revision).await;
        assert_eq!(
            position_content(&undone),
            position_content(&before),
            "{command}"
        );
        assert!(!undone.modified);
        revision += 1;
        assert_eq!(harness.invoke("redo", json!({})).await, revision);
        let redone = harness.document(221, revision).await;
        assert_eq!(
            position_content(&redone),
            position_content(&after),
            "{command}"
        );
    }
    harness.finish().await;
}

#[tokio::test]
async fn conversion_replaces_document_with_current_board_through_ipc() {
    let mut harness = Harness::new().await;
    harness.subscribe(222).await;
    harness.document(222, 0).await;
    let source = b"(;SZ[9]PB[black]PW[white]KM[6.5];B[dd];W[ee])";
    assert_eq!(
        harness
            .invoke("open_bytes", json!({"bytes": source.as_slice()}))
            .await,
        1
    );
    harness.document(222, 1).await;
    assert_eq!(harness.invoke("step", json!({"count": 2})).await, 2);
    let before = harness.document(222, 2).await;
    assert_eq!(
        harness.invoke("convert_to_start_position", json!({})).await,
        3
    );
    let after = harness.document(222, 3).await;
    assert_eq!(
        after.board.as_ref().unwrap().stones,
        before.board.as_ref().unwrap().stones
    );
    assert_eq!(after.board.as_ref().unwrap().move_number, 0);
    assert_eq!(after.tree.main_line, vec![after.tree.root]);
    assert_eq!(after.game_info.black_name.as_deref(), Some("black"));
    assert_eq!(after.game_info.white_name.as_deref(), Some("white"));
    assert_eq!(after.game_info.komi, 6.5);
    assert!(!after.can_undo);
    assert!(!after.can_redo);
    assert!(
        request(&harness.main, "undo", json!({}), local_url())
            .await
            .is_err()
    );
    harness.finish().await;
}

#[tokio::test]
async fn setup_markup_rejects_invalid_input_and_requires_local_main_permissions() {
    let harness = Harness::new().await;
    for (command, args) in [
        (
            "set_setup_stone",
            json!({"point": {"x": 25, "y": 0}, "color": "Black"}),
        ),
        (
            "set_markup",
            json!({"point": {"x": 0, "y": 25}, "kind": "Triangle"}),
        ),
    ] {
        let error = request(&harness.main, command, args, local_url())
            .await
            .unwrap_err();
        assert_eq!(error["type"], "App");
    }
    for (command, args) in [
        (
            "set_setup_stone",
            json!({"point": {"x": -1, "y": 0}, "color": "Black"}),
        ),
        (
            "set_setup_stone",
            json!({"point": {"x": 1, "y": 0}, "color": "Blue"}),
        ),
        ("set_first_player", json!({"color": "Blue"})),
        (
            "set_markup",
            json!({"point": {"x": 0, "y": 0}, "kind": "Unknown"}),
        ),
    ] {
        assert!(
            request(&harness.main, command, args, local_url())
                .await
                .is_err()
        );
    }
    let other = tauri::WebviewWindowBuilder::new(&harness.app, "setup-other", Default::default())
        .build()
        .unwrap();
    for command in [
        "set_setup_stone",
        "clear_setup",
        "set_first_player",
        "convert_to_start_position",
        "set_markup",
        "clear_markup",
        "toggle_to_play",
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
