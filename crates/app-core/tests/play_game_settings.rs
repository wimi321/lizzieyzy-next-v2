#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;
#[path = "common/game.rs"]
mod support;

use app_core::{GamePhase, GameSetup, GameStart, NewGameOptions, Player, Settings};
use common::within;
use go_core::Color;
use support::{ai, config, harness, setup};

#[tokio::test]
async fn continue_game_preserves_saved_new_board_setup_in_memory_and_after_restart() {
    let h = harness(vec![]).await;
    let saved = GameSetup {
        white: Player::Ai(ai(17)),
        start: GameStart::NewBoard(NewGameOptions {
            width: 13,
            height: 13,
            handicap: 2,
            komi: 0.5,
            rules: "japanese".into(),
            black_name: Some("Human Black".into()),
            black_rank: Some("5k".into()),
            white_name: Some("KataGo White".into()),
            white_rank: Some("9d".into()),
        }),
        ..setup()
    };
    within(h.app.start_game(saved.clone())).await.unwrap();
    within(h.app.stop_game()).await.unwrap();
    assert_eq!(within(h.app.settings()).await.unwrap().game_setup, saved);
    let settings_path = h.directory.path().join("settings.json");
    let original_bytes = std::fs::read(&settings_path).unwrap();

    for color in [Some(Color::Black), None] {
        within(h.app.open_bytes(b"(;SZ[9]KM[7.5];B[aa])".to_vec()))
            .await
            .unwrap();
        within(h.app.step(1)).await.unwrap();
        let continued = within(h.app.continue_game(color)).await.unwrap();
        assert_eq!(continued.phase, GamePhase::Playing);
        assert_eq!(continued.start_node, 1);
        let (black, white) = if color == Some(Color::Black) {
            (Player::Ai(ai(17)), Player::Human)
        } else {
            (Player::Human, Player::Ai(ai(17)))
        };
        assert_eq!((continued.black, continued.white), (black, white));
        assert_eq!(within(h.app.settings()).await.unwrap().game_setup, saved);
        within(h.app.stop_game()).await.unwrap();
        assert_eq!(within(h.app.settings()).await.unwrap().game_setup, saved);
        assert_eq!(std::fs::read(&settings_path).unwrap(), original_bytes);
        assert_eq!(
            within(Settings::load(h.directory.path()))
                .await
                .unwrap()
                .game_setup,
            saved
        );
    }

    within(h.app.shutdown()).await.unwrap();
    let (restarted, _events) = within(app_core::spawn_with_config(
        h.directory.path(),
        config(h.directory.path()),
    ))
    .await
    .unwrap();
    assert_eq!(
        within(restarted.settings()).await.unwrap().game_setup,
        saved
    );
    within(restarted.shutdown()).await.unwrap();
}
