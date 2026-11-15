#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;
#[path = "common/game.rs"]
mod game;

use app_core::{
    AppCore, AppError, AppEvent, AppSubscription, DocumentView, NewGameOptions, ScoringError,
    ScoringRuleView, Settings,
};
use common::within;
use go_core::Point;

async fn offline() -> (tempfile::TempDir, AppCore, AppSubscription) {
    let directory = tempfile::tempdir().unwrap();
    within(
        Settings {
            pondering: false,
            auto_quick_analyze_on_load: false,
            ..Settings::default()
        }
        .save(directory.path()),
    )
    .await
    .unwrap();
    let (app, events) = within(app_core::spawn(directory.path())).await.unwrap();
    (directory, app, events)
}

async fn document(events: &mut AppSubscription, scoring: bool) -> DocumentView {
    within(async {
        loop {
            if let AppEvent::DocumentChanged { view, .. } = events.recv().await.unwrap()
                && view.scoring.is_some() == scoring
            {
                return view;
            }
        }
    })
    .await
}

#[tokio::test]
async fn toggles_entire_connected_group_and_single_neutral_point_and_cancels_without_edit() {
    let (_dir, app, mut events) = offline().await;
    within(app.open_bytes(b"(;SZ[5]KM[0]AB[aa][ba][cb]AW[ae])".to_vec()))
        .await
        .unwrap();
    let before = document(&mut events, false).await;
    let initial = within(app.start_scoring()).await.unwrap();
    assert!(initial.dead_stones.is_empty());
    assert_eq!(initial.rule, ScoringRuleView::Area);
    assert!(!within(app.analysis_state()).await.unwrap().pondering);
    let dead = within(app.toggle_scoring_point(Point { x: 1, y: 0 }))
        .await
        .unwrap();
    assert_eq!(dead.dead_stones.len(), 2);
    assert_eq!(dead.black.dead_stones, 2);
    assert_eq!(dead.black.living_stones, 1);
    let alive = within(app.toggle_scoring_point(Point { x: 0, y: 0 }))
        .await
        .unwrap();
    assert!(alive.dead_stones.is_empty());
    let neutral = within(app.toggle_scoring_point(Point { x: 3, y: 3 }))
        .await
        .unwrap();
    assert_eq!(
        neutral.neutral_points,
        vec![app_core::PointView { x: 3, y: 3 }]
    );
    assert_eq!(neutral.ownership[18], None);
    assert!(
        within(app.toggle_scoring_point(Point { x: 3, y: 3 }))
            .await
            .unwrap()
            .neutral_points
            .is_empty()
    );
    assert!(matches!(
        within(app.toggle_scoring_point(Point { x: 5, y: 0 })).await,
        Err(AppError::Scoring(ScoringError::OutOfBounds))
    ));
    let japanese = within(app.set_scoring_rule(ScoringRuleView::Territory))
        .await
        .unwrap();
    assert_eq!(japanese.rule, ScoringRuleView::Territory);
    within(app.cancel_scoring()).await.unwrap();
    let after = document(&mut events, false).await;
    assert_eq!(after.board, before.board);
    assert_eq!(after.game_info, before.game_info);
    assert!(!after.modified);
    assert!(!after.can_undo);
    assert!(!within(app.analysis_state()).await.unwrap().pondering);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn commit_writes_standard_re_and_is_one_undo_redo_edit() {
    let (_dir, app, mut events) = offline().await;
    within(app.open_bytes(b"(;SZ[3]KM[0.5]AB[aa]AW[bb])".to_vec()))
        .await
        .unwrap();
    let before = document(&mut events, false).await;
    within(app.start_scoring()).await.unwrap();
    let score = within(app.toggle_scoring_point(Point { x: 1, y: 1 }))
        .await
        .unwrap();
    assert_eq!(score.result, "B+8.5");
    within(app.commit_scoring()).await.unwrap();
    let committed = document(&mut events, false).await;
    assert_eq!(committed.game_info.result.as_deref(), Some("B+8.5"));
    assert_eq!(committed.board, before.board);
    assert!(committed.can_undo);
    within(app.undo()).await.unwrap();
    let undone = document(&mut events, false).await;
    assert_eq!(undone.game_info.result, None);
    assert!(!undone.can_undo);
    assert!(undone.can_redo);
    assert!(!undone.modified);
    within(app.redo()).await.unwrap();
    assert_eq!(
        document(&mut events, false)
            .await
            .game_info
            .result
            .as_deref(),
        Some("B+8.5")
    );
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn follows_sgf_rule_and_handicap_and_commits_draw() {
    let (_dir, app, mut events) = offline().await;
    for (preset, compensation) in [("chinese", 3), ("aga", 2), ("japanese", 0)] {
        within(app.open_bytes(format!("(;SZ[5]RU[{preset}]HA[3]KM[0])").into_bytes()))
            .await
            .unwrap();
        let score = within(app.start_scoring()).await.unwrap();
        assert_eq!(score.white.handicap_compensation, compensation);
        let territory = within(app.set_scoring_rule(ScoringRuleView::Territory))
            .await
            .unwrap();
        assert_eq!(territory.white.handicap_compensation, 0);
        assert_eq!(territory.result, "0");
        within(app.commit_scoring()).await.unwrap();
        assert_eq!(
            document(&mut events, false)
                .await
                .game_info
                .result
                .as_deref(),
            Some("0")
        );
    }
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn navigation_content_edits_undo_and_replacement_exit_scoring() {
    let (_dir, app, mut events) = offline().await;
    within(app.open_bytes(b"(;SZ[5];B[aa])".to_vec()))
        .await
        .unwrap();
    within(app.start_scoring()).await.unwrap();
    within(app.step(1)).await.unwrap();
    assert_eq!(document(&mut events, false).await.tree.current, 1);
    assert!(matches!(
        within(app.commit_scoring()).await,
        Err(AppError::Scoring(ScoringError::NotStarted))
    ));
    within(app.start_scoring()).await.unwrap();
    within(app.set_comment("edited".into())).await.unwrap();
    assert_eq!(
        document(&mut events, false).await.comment.as_deref(),
        Some("edited")
    );
    within(app.start_scoring()).await.unwrap();
    within(app.undo()).await.unwrap();
    assert_eq!(document(&mut events, false).await.comment, None);
    within(app.start_scoring()).await.unwrap();
    within(app.new_game(NewGameOptions::default()))
        .await
        .unwrap();
    assert_eq!(document(&mut events, false).await.tree.current, 0);
    within(app.start_scoring()).await.unwrap();
    within(app.open_bytes(b"(;SZ[9])".to_vec())).await.unwrap();
    assert_eq!(document(&mut events, false).await.board.unwrap().width, 9);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn active_and_paused_game_reject_scoring_with_typed_error() {
    let h = game::harness(Vec::new()).await;
    within(h.app.start_game(game::setup())).await.unwrap();
    assert!(matches!(
        within(h.app.start_scoring()).await,
        Err(AppError::Scoring(ScoringError::GameActive))
    ));
    within(h.app.pause_game()).await.unwrap();
    assert!(matches!(
        within(h.app.start_scoring()).await,
        Err(AppError::Scoring(ScoringError::GameActive))
    ));
    within(h.app.stop_game()).await.unwrap();
    within(h.app.start_scoring()).await.unwrap();
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn active_and_paused_match_reject_scoring_with_typed_error() {
    let h = game::harness(Vec::new()).await;
    let id = within(h.app.settings())
        .await
        .unwrap()
        .main_engine_id
        .unwrap();
    let side = app_core::MatchPlayer::KataGo {
        engine_id: id,
        play: app_core::KataGoPlay {
            seconds: Some(60.0),
            visits: None,
            resign: None,
            ..Default::default()
        },
        wrn: Some(0.0),
        pda: Some(0.0),
    };
    within(h.app.start_match(app_core::MatchSetup {
        a: side.clone(),
        b: side,
        auto_save: false,
        ..Default::default()
    }))
    .await
    .unwrap();
    assert!(matches!(
        within(h.app.start_scoring()).await,
        Err(AppError::Scoring(ScoringError::GameActive))
    ));
    within(h.app.pause_match()).await.unwrap();
    assert!(matches!(
        within(h.app.start_scoring()).await,
        Err(AppError::Scoring(ScoringError::GameActive))
    ));
    within(h.app.stop_match()).await.unwrap();
    within(h.app.start_scoring()).await.unwrap();
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn pauses_live_queries_and_restores_previous_user_intent() {
    let mut h = game::harness(Vec::new()).await;
    within(h.app.set_pondering(true)).await.unwrap();
    let first = game::query(h.directory.path(), |q| q.get("moves").is_some()).await;
    within(h.app.start_scoring()).await.unwrap();
    game::query(h.directory.path(), |q| {
        q["action"] == "terminate" && q["terminateId"] == first["id"]
    })
    .await;
    assert!(!within(h.app.analysis_state()).await.unwrap().pondering);
    assert!(within(h.app.settings()).await.unwrap().pondering);
    within(h.app.cancel_scoring()).await.unwrap();
    game::query(h.directory.path(), |q| {
        q.get("moves").is_some() && q["id"] != first["id"]
    })
    .await;
    assert!(within(h.app.analysis_state()).await.unwrap().pondering);
    let view = document(&mut h.events, false).await;
    assert_eq!(view.game_info.result, None);
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn cancelling_scoring_preserves_an_automatic_playout_pause() {
    let mut h = game::harness(Vec::new()).await;
    within(h.app.set_analysis_limits(app_core::AnalysisLimitSettings {
        limit_time: false,
        limit_playout: true,
        limit_playouts: 1,
        ..Default::default()
    }))
    .await
    .unwrap();
    within(h.app.set_pondering(true)).await.unwrap();
    within(async {
        loop {
            if let AppEvent::AnalysisStateChanged(state) = h.events.recv().await.unwrap()
                && state.pause_reason == Some(app_core::AnalysisPauseReason::PlayoutLimit)
            {
                break;
            }
        }
    })
    .await;
    within(h.app.start_scoring()).await.unwrap();
    within(h.app.cancel_scoring()).await.unwrap();
    let state = within(h.app.analysis_state()).await.unwrap();
    assert!(!state.pondering);
    assert_eq!(
        state.pause_reason,
        Some(app_core::AnalysisPauseReason::PlayoutLimit)
    );
    assert!(within(h.app.settings()).await.unwrap().pondering);
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn committing_restores_user_pondering_enabled_during_scoring() {
    let h = game::harness(Vec::new()).await;
    within(h.app.start_scoring()).await.unwrap();
    within(h.app.set_pondering(true)).await.unwrap();
    assert!(!within(h.app.analysis_state()).await.unwrap().pondering);
    within(h.app.commit_scoring()).await.unwrap();
    game::query(h.directory.path(), |q| q.get("moves").is_some()).await;
    assert!(within(h.app.analysis_state()).await.unwrap().pondering);
    within(h.app.shutdown()).await.unwrap();
}
