use super::*;

#[tokio::test]
async fn fox_commands_preserve_typed_validation_errors_and_clear_history() {
    let harness = Harness::new().await;
    harness.core.set_pondering(false).await.unwrap();
    for (command, arguments, kind) in [
        (
            "fox_search",
            json!({"query": {"type": "Nickname", "value": "  "}}),
            "InvalidNickname",
        ),
        (
            "fox_search",
            json!({"query": {"type": "Uid", "value": "abc"}}),
            "InvalidUid",
        ),
        (
            "fox_next_page",
            json!({"uid": "abc", "cursor": "1"}),
            "InvalidUid",
        ),
        (
            "fox_open_game",
            json!({"gameId": "../../invalid"}),
            "InvalidGameId",
        ),
    ] {
        let error = request(&harness.main, command, arguments, local_url())
            .await
            .unwrap_err();
        assert_eq!(error, json!({"type": "Fox", "data": {"type": kind}}));
    }
    harness.invoke("fox_clear_history", json!({})).await;
    assert_eq!(
        harness.invoke("settings", json!({})).await["fox_history"],
        json!([])
    );
    let stored: Settings = serde_json::from_slice(
        &std::fs::read(harness.directory.path().join("settings.json")).unwrap(),
    )
    .unwrap();
    assert!(stored.fox_history.is_empty());
    harness.finish().await;
}

#[tokio::test]
async fn fox_commands_are_denied_to_other_windows_and_remote_origins() {
    let harness = Harness::new().await;
    let other = tauri::WebviewWindowBuilder::new(&harness.app, "other", Default::default())
        .build()
        .unwrap();
    for command in [
        "fox_search",
        "fox_next_page",
        "fox_open_game",
        "fox_clear_history",
    ] {
        for (window, url) in [
            (&other, local_url()),
            (&harness.main, "https://example.com/"),
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
