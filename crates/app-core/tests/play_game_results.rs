#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
use common::within;
#[path = "common/game.rs"]
mod support;
use app_core::{
    AiPlayer, AppError, GameEndReason, GameError, GamePhase, GameStart, HumanClock, KataGoPlay,
    NewGameOptions, Player, ResignRule,
};
use go_core::{Color, Move};
use serde_json::json;
use std::time::Duration;
use support::*;

async fn finished(app: &app_core::AppCore) -> app_core::GameView {
    within(async {
        loop {
            let view = app.game_view().await.unwrap().unwrap();
            if view.phase == GamePhase::Finished {
                return view;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
}
async fn scripted(script: serde_json::Value) -> Harness {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("script.json");
    std::fs::write(&file, script.to_string()).unwrap();
    let mut cfg = config(directory.path());
    cfg.game_query_timeout = Duration::from_secs(2);
    harness_in(
        directory,
        cfg,
        vec!["--play-script".into(), file.to_string_lossy().into()],
    )
    .await
}

#[tokio::test]
async fn double_pass_scores_black_perspective_on_komi_lattice_and_writes_re() {
    for (komi, score, expected) in [(7.5, 2.1, "B+2.5"), (7.5, -2.1, "W+2.5"), (6.0, 0.2, "0")] {
        let mut h =
            scripted(json!({"default":{"scoreLead":score,"candidates":[{"move":"pass"}]}})).await;
        let mut options = setup();
        options.start = GameStart::NewBoard(NewGameOptions {
            komi,
            ..Default::default()
        });
        within(h.app.start_game(options)).await.unwrap();
        within(h.app.play(Move::Pass(Color::Black))).await.unwrap();
        let view = finished(&h.app).await;
        assert_eq!(view.end_reason, Some(GameEndReason::Scored));
        assert_eq!(view.result.as_deref(), Some(expected));
        let doc = document(&mut h.events, view.latest_node).await;
        assert_eq!(doc.game_info.result.as_deref(), Some(expected));
        let request = query(h.directory.path(), |q| q["maxVisits"] == 200).await;
        assert_eq!(request["moves"].as_array().unwrap().len(), 2);
        within(h.app.shutdown()).await.unwrap();
    }
}

#[tokio::test]
async fn failed_scoring_finishes_without_result_and_auto_saves() {
    let mut h = scripted(
        json!({"default":{"candidates":[{"move":"pass"}]},"turns":{"2":{"error":"cannot score"}}}),
    )
    .await;
    let mut options = setup();
    options.auto_save = true;
    within(h.app.start_game(options)).await.unwrap();
    within(h.app.play(Move::Pass(Color::Black))).await.unwrap();
    let view = finished(&h.app).await;
    assert_eq!(view.end_reason, Some(GameEndReason::Unscored));
    assert_eq!(view.result, None);
    let path = within(async {
        loop {
            let doc = document(&mut h.events, view.latest_node).await;
            if let Some(path) = doc.file_path {
                break path;
            }
        }
    })
    .await;
    let text = std::fs::read_to_string(path).unwrap();
    assert!(!text.contains("RE["));
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn scoring_timeout_is_unscored_and_late_query_cannot_move() {
    tokio::time::resume();
    let h =
        scripted(json!({"default":{"candidates":[{"move":"pass"}]},"turns":{"2":{"hang":true}}}))
            .await;
    within(h.app.start_game(setup())).await.unwrap();
    within(h.app.play(Move::Pass(Color::Black))).await.unwrap();
    moves(&h.app, 2).await;
    query(h.directory.path(), |q| q["maxVisits"] == 200).await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(3)).await;
    tokio::time::resume();
    let view = finished(&h.app).await;
    assert_eq!(view.end_reason, Some(GameEndReason::Unscored));
    assert_eq!(view.result, None);
    assert_eq!(view.move_number, 2);
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn ai_resignation_counts_its_consecutive_turns_and_resets_on_recovery() {
    let h=scripted(json!({"default":{"winrate":0.99,"candidates":[{"move":"pass"}]},"turns":{"3":{"winrate":0.1,"candidates":[{"move":"pass"}]}}})).await;
    let mut options = setup();
    options.white = Player::Ai(AiPlayer::KataGo(KataGoPlay {
        seconds: None,
        visits: Some(1),
        resign: Some(ResignRule {
            after_moves: 0,
            consecutive: 2,
            below_winrate: 0.05,
        }),
        ..Default::default()
    }));
    within(h.app.start_game(options)).await.unwrap();
    for (i, expected) in [(0, 2), (1, 4), (2, 6)] {
        within(
            h.app
                .play(Move::Play(Color::Black, go_core::Point { x: i, y: 0 })),
        )
        .await
        .unwrap();
        let view = moves(&h.app, expected).await;
        assert_eq!(view.phase, GamePhase::Playing);
    }
    within(
        h.app
            .play(Move::Play(Color::Black, go_core::Point { x: 3, y: 0 })),
    )
    .await
    .unwrap();
    let view = finished(&h.app).await;
    assert_eq!(view.end_reason, Some(GameEndReason::Resignation));
    assert_eq!(view.result.as_deref(), Some("B+R"));
    assert_eq!(view.move_number, 7);
    within(h.app.shutdown()).await.unwrap();
}

#[derive(Debug)]
struct FixedClock;
impl app_core::BatchClock for FixedClock {
    fn local_timestamp(&self) -> String {
        "20261113123456".into()
    }
}

#[tokio::test]
async fn human_resign_and_stop_save_safe_names_and_never_overwrite() {
    let directory = tempfile::tempdir().unwrap();
    let mut cfg = config(directory.path());
    cfg.batch_clock = std::sync::Arc::new(FixedClock);
    let h = harness_in(directory, cfg, vec![]).await;
    let mut options = setup();
    options.auto_save = true;
    options.start = GameStart::NewBoard(NewGameOptions {
        black_name: Some("A/B:*?".into()),
        white_name: Some("C\\D".into()),
        ..Default::default()
    });
    within(h.app.start_game(options.clone())).await.unwrap();
    let view = within(h.app.resign_game()).await.unwrap();
    assert_eq!(view.result.as_deref(), Some("W+R"));
    let path = h
        .directory
        .path()
        .join("MyGames/2026-11-13-123456_A_B____vs_C_D.sgf");
    assert!(std::fs::read_to_string(&path).unwrap().contains("RE[W+R]"));
    within(h.app.start_game(options)).await.unwrap();
    let view = within(h.app.stop_game()).await.unwrap();
    assert_eq!(view.result, None);
    assert_eq!(view.end_reason, Some(GameEndReason::Stopped));
    let second = h
        .directory
        .path()
        .join("MyGames/2026-11-13-123456_A_B____vs_C_D-2.sgf");
    assert!(!std::fs::read_to_string(second).unwrap().contains("RE["));
    assert!(std::fs::read_to_string(path).unwrap().contains("RE[W+R]"));
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn save_failure_is_typed_but_does_not_reopen_finished_game() {
    let directory = tempfile::tempdir().unwrap();
    let mut cfg = config(directory.path());
    let blocked = directory.path().join("not-directory");
    std::fs::write(&blocked, "keep").unwrap();
    cfg.games_directory = Some(blocked);
    let h = harness_in(directory, cfg, vec![]).await;
    let mut options = setup();
    options.auto_save = true;
    within(h.app.start_game(options)).await.unwrap();
    assert!(matches!(
        within(h.app.resign_game()).await,
        Err(AppError::Game(GameError::AutoSave { .. }))
    ));
    let view = game(&h.app).await;
    assert_eq!(view.phase, GamePhase::Finished);
    assert_eq!(view.result.as_deref(), Some("W+R"));
    assert!(matches!(view.error, Some(GameError::AutoSave { .. })));
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn human_clock_main_periods_reset_pause_and_timeout() {
    tokio::time::resume();
    let h = harness(vec![]).await;
    let mut options = setup();
    options.white = Player::Human;
    options.human_clock = Some(HumanClock {
        main_seconds: 2,
        byoyomi_seconds: 3,
        periods: 2,
    });
    within(h.app.start_game(options)).await.unwrap();
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(3)).await;
    let view = game(&h.app).await;
    assert_eq!(view.black_clock.as_ref().unwrap().main_seconds, 0.0);
    assert_eq!(view.black_clock.as_ref().unwrap().periods, 2);
    assert!((view.black_clock.as_ref().unwrap().byoyomi_seconds - 2.0).abs() < 0.01);
    let paused = within(h.app.pause_game()).await.unwrap();
    tokio::time::advance(Duration::from_secs(100)).await;
    assert_eq!(game(&h.app).await.black_clock, paused.black_clock);
    within(h.app.resume_game()).await.unwrap();
    within(h.app.play(Move::Pass(Color::Black))).await.unwrap();
    let view = game(&h.app).await;
    assert_eq!(view.black_clock.unwrap().byoyomi_seconds, 3.0);
    // White consumes main time, one period, then the final period.
    tokio::time::advance(Duration::from_secs(5)).await;
    let view = game(&h.app).await;
    assert_eq!(view.white_clock.as_ref().unwrap().periods, 1);
    tokio::time::advance(Duration::from_secs(3)).await;
    let view = finished(&h.app).await;
    assert_eq!(view.end_reason, Some(GameEndReason::Timeout));
    assert_eq!(view.result.as_deref(), Some("B+T"));
    tokio::time::resume();
    within(h.app.shutdown()).await.unwrap();
}
