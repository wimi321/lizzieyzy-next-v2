use super::{Harness, json, local_url, request, within};
use app_core::{AppEvent, GamePhase, GameSetup, Player};

#[tokio::test]
async fn game_controls_settings_and_typed_guards_round_trip_through_ipc() {
    let harness = Harness::new().await;
    assert_eq!(harness.invoke("game_view", json!({})).await, json!(null));
    let setup = GameSetup {
        white: Player::Human,
        auto_save: false,
        ponder: false,
        ..Default::default()
    };
    let started = harness
        .invoke("start_game", json!({ "setup": setup }))
        .await;
    assert_eq!(started["phase"], "Playing");
    assert_eq!(
        harness.invoke("settings", json!({})).await["game_setup"],
        json!(setup)
    );
    assert_eq!(harness.invoke("game_view", json!({})).await, started);
    let error = request(
        &harness.main,
        "set_comment",
        json!({"text":"blocked"}),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(error["type"], "GameRunning");
    assert_eq!(
        harness.invoke("pause_game", json!({})).await["phase"],
        "Paused"
    );
    assert_eq!(
        harness.invoke("resume_game", json!({})).await["phase"],
        "Playing"
    );
    harness
        .invoke(
            "play",
            json!({ "mv": { "type":"Play", "data": {"color":"Black", "point":{"x":0,"y":0}} } }),
        )
        .await;
    assert_eq!(
        harness.invoke("take_back", json!({})).await["move_number"],
        0
    );
    assert_eq!(
        harness.invoke("resign_game", json!({})).await["result"],
        "W+R"
    );
    let ended = harness.invoke("game_view", json!({})).await;
    assert_eq!(ended["phase"], "Finished");
    let error = request(&harness.main, "play_best_move", json!({}), local_url())
        .await
        .unwrap_err();
    assert_eq!(error, json!({"type":"Game", "data":{"type":"NoAnalysis"}}));
    harness
        .invoke("start_game", json!({ "setup": setup }))
        .await;
    assert_eq!(
        harness.invoke("stop_game", json!({})).await["end_reason"],
        "Stopped"
    );
    harness.finish().await;
}

#[tokio::test]
async fn game_status_is_replayed_through_existing_channel_and_setting_validates() {
    let mut harness = Harness::new().await;
    harness.subscribe(81).await;
    harness.document(81, 0).await;
    harness
        .invoke(
            "start_game",
            json!({"setup": {"white":{"type":"Human"},"auto_save":false,"ponder":false}}),
        )
        .await;
    within(async {
        loop {
            if let AppEvent::GameUpdated(view) = harness.next().await.event
                && view.phase == GamePhase::Playing
            {
                break;
            }
        }
    })
    .await;
    harness.subscribe(82).await;
    within(async {
        loop {
            let message = harness.next().await;
            if message.callback == 82
                && let AppEvent::GameUpdated(view) = message.event
                && view.phase == GamePhase::Playing
            {
                break;
            }
        }
    })
    .await;
    harness.invoke("stop_game", json!({})).await;
    harness
        .invoke(
            "set_ai_player",
            json!({"color":"White","player":{"type":"KataGo","data":{"seconds":null,"visits":17}}}),
        )
        .await;
    assert_eq!(
        harness.invoke("settings", json!({})).await["game_setup"]["white"]["data"]["data"]["visits"],
        17
    );
    let error = request(
        &harness.main,
        "set_ai_player",
        json!({"color":"White","player":{"type":"KataGo","data":{"seconds":null,"visits":null}}}),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(error["type"], "Game");
    assert_eq!(error["data"]["type"], "InvalidSetup");
    harness.finish().await;
}

#[tokio::test]
async fn game_commands_are_only_allowed_for_local_main() {
    let harness = Harness::new().await;
    let other = tauri::WebviewWindowBuilder::new(&harness.app, "game-other", Default::default())
        .build()
        .unwrap();
    for command in [
        "start_game",
        "pause_game",
        "resume_game",
        "stop_game",
        "resign_game",
        "take_back",
        "continue_game",
        "set_ai_player",
        "play_best_move",
        "game_view",
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
