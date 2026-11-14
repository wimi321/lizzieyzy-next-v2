use super::{Harness, json, local_url, request};

#[tokio::test]
async fn gtp_settings_and_match_typed_errors_round_trip() {
    let harness = Harness::new().await;
    assert_eq!(
        harness.invoke("settings", json!({})).await["gtp_engines"],
        json!([])
    );
    let entry = json!({"id":"third-party","name":"Example","program":"/unused/engine","args":[],"working_dir":null,"init_commands":["name"]});
    assert_eq!(
        harness
            .invoke("add_gtp_engine", json!({"entry":entry}))
            .await["gtp_engines"],
        json!([entry])
    );
    let mut edited = entry.clone();
    edited["name"] = json!("Renamed");
    assert_eq!(
        harness
            .invoke("update_gtp_engine", json!({"entry":edited}))
            .await["gtp_engines"][0]["name"],
        "Renamed"
    );
    assert_eq!(
        harness
            .invoke("gtp_console", json!({"engineId":"third-party"}))
            .await,
        json!([])
    );
    let error = request(
        &harness.main,
        "gtp_console_send",
        json!({"engineId":"third-party","text":"name\nquit"}),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(error["type"], "Gtp");
    assert_eq!(error["data"]["type"], "InvalidCommand");
    assert_eq!(harness.invoke("match_view", json!({})).await, json!(null));
    for command in [
        "pause_match",
        "resume_match",
        "stop_match",
        "match_play_now",
    ] {
        let error = request(&harness.main, command, json!({}), local_url())
            .await
            .unwrap_err();
        assert_eq!(error["type"], "Match");
        assert_eq!(error["data"]["type"], "NotActive");
    }
    assert_eq!(
        harness
            .invoke("remove_gtp_engine", json!({"id":"third-party"}))
            .await["gtp_engines"],
        json!([])
    );
    harness.finish().await;
}

#[tokio::test]
async fn gtp_and_match_commands_are_local_main_only() {
    let harness = Harness::new().await;
    let other = tauri::WebviewWindowBuilder::new(&harness.app, "gtp-other", Default::default())
        .build()
        .unwrap();
    for command in [
        "add_gtp_engine",
        "update_gtp_engine",
        "remove_gtp_engine",
        "test_gtp_engine",
        "gtp_console",
        "gtp_console_send",
        "start_match",
        "pause_match",
        "resume_match",
        "stop_match",
        "set_match_games",
        "match_play_now",
        "match_resign",
        "set_match_side_params",
        "match_view",
    ] {
        for (window, url) in [
            (&other, local_url()),
            (&harness.main, "https://example.com/"),
        ] {
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
