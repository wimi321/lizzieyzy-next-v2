#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;
#[path = "common/game.rs"]
mod support;

use app_core::{
    AppError, AppEvent, ColorView, GameError, GamePhase, GameSetup, GameStart, NewGameOptions,
    Player, PvPick, WholeGameOptions,
};
use common::within;
use go_core::{Color, Move, Point};
use serde_json::json;
use support::{ai, document, game, harness, moves, query, requests, requests_before, setup};

fn play(color: Color, x: u8, y: u8) -> Move {
    Move::Play(color, Point { x, y })
}

async fn scripted(script: serde_json::Value) -> support::Harness {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("play.json");
    std::fs::write(&path, serde_json::to_vec(&script).unwrap()).unwrap();
    let config = support::config(directory.path());
    support::harness_in(
        directory,
        config,
        vec!["--play-script".into(), path.to_str().unwrap().into()],
    )
    .await
}

async fn gated() -> (support::Harness, std::path::PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let gate = directory.path().join("release");
    let path = directory.path().join("play.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&json!({
            "turns": {"1": {"gateFile": gate, "candidates": [{"move": "D4"}]}}
        }))
        .unwrap(),
    )
    .unwrap();
    let config = support::config(directory.path());
    let harness = support::harness_in(
        directory,
        config,
        vec!["--play-script".into(), path.to_str().unwrap().into()],
    )
    .await;
    (harness, gate)
}

#[tokio::test]
async fn new_games_use_document_handicap_for_all_three_sizes_and_persist_setup() {
    let mut h = harness(vec![]).await;
    for size in [9, 13, 19] {
        let options = NewGameOptions {
            width: size,
            height: size,
            komi: 0.5,
            handicap: 4,
            black_name: Some("Human Black".into()),
            white_name: Some("Human White".into()),
            ..NewGameOptions::default()
        };
        let setup = GameSetup {
            white: Player::Human,
            start: GameStart::NewBoard(options),
            ..setup()
        };
        let started = within(h.app.start_game(setup.clone())).await.unwrap();
        assert_eq!(started.phase, GamePhase::Playing);
        assert_eq!(started.move_number, 0);
        assert_eq!(started.start_node, started.latest_node);
        assert_eq!(started.to_play, ColorView::White);
        assert!(!started.ai_thinking);
        let view = document(&mut h.events, started.latest_node).await;
        let board = view.board.unwrap();
        assert_eq!((board.width, board.height), (size, size));
        assert_eq!(board.stones.iter().flatten().count(), 4);
        assert!(
            board
                .stones
                .iter()
                .flatten()
                .all(|color| *color == ColorView::Black)
        );
        assert_eq!(view.game_info.komi, 0.5);
        assert_eq!(view.game_info.black_name.as_deref(), Some("Human Black"));
        assert_eq!(within(h.app.settings()).await.unwrap().game_setup, setup);
        within(h.app.stop_game()).await.unwrap();
    }
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn current_position_game_appends_a_distinct_branch_and_keeps_comments() {
    let mut h = harness(vec![]).await;
    let source = b"(;SZ[19]C[root];B[aa]C[start](;W[bb]C[original];B[cc])(;W[dd]C[side]))";
    within(h.app.open_bytes(source.to_vec())).await.unwrap();
    within(h.app.step(1)).await.unwrap();
    let tree = within(h.app.variation_tree()).await.unwrap();
    let start = tree.nodes[1].id;
    let original = tree.nodes.clone();
    let state = within(h.app.start_game(GameSetup {
        white: Player::Human,
        start: GameStart::CurrentPosition,
        ..setup()
    }))
    .await
    .unwrap();
    assert_eq!(state.start_node, start);
    assert_eq!(state.move_number, 1);
    within(h.app.play(play(Color::White, 1, 1))).await.unwrap();
    let state = game(&h.app).await;
    assert_eq!(state.move_number, 2);
    let new_tree = within(h.app.variation_tree()).await.unwrap();
    let fork = new_tree.nodes.iter().find(|node| node.id == start).unwrap();
    assert_eq!(fork.children.len(), 3);
    assert_eq!(fork.children.last(), Some(&state.latest_node));
    for old in original.iter().filter(|node| node.id != start) {
        assert_eq!(
            new_tree.nodes.iter().find(|node| node.id == old.id),
            Some(old)
        );
    }
    within(h.app.stop_game()).await.unwrap();
    for (id, comment) in [
        (start, "start"),
        (fork.children[0], "original"),
        (fork.children[1], "side"),
    ] {
        within(h.app.go_to(id)).await.unwrap();
        assert_eq!(
            document(&mut h.events, id).await.comment.as_deref(),
            Some(comment)
        );
    }
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn continue_game_chooses_side_to_play_or_explicit_color_from_saved_settings() {
    let h = harness(vec![]).await;
    let saved = GameSetup {
        white: Player::Ai(ai(17)),
        ponder: false,
        ..setup()
    };
    within(h.app.start_game(saved)).await.unwrap();
    within(h.app.stop_game()).await.unwrap();
    within(h.app.open_bytes(b"(;SZ[19];B[aa];W[bb])".to_vec()))
        .await
        .unwrap();
    within(h.app.step(1)).await.unwrap();
    let automatic = within(h.app.continue_game(None)).await.unwrap();
    assert_eq!(automatic.black, Player::Human);
    assert_eq!(automatic.white, Player::Ai(ai(17)));
    assert_eq!(automatic.start_node, 1);
    within(h.app.stop_game()).await.unwrap();
    within(h.app.go_to(1)).await.unwrap();
    let explicit = within(h.app.continue_game(Some(Color::Black)))
        .await
        .unwrap();
    assert!(matches!(explicit.black, Player::Ai(_)));
    assert_eq!(explicit.white, Player::Human);
    assert_eq!(explicit.to_play, ColorView::White);
    assert!(!explicit.ai_thinking);
    assert!(!within(h.app.settings()).await.unwrap().game_setup.ponder);
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn game_rejects_edits_and_ai_turn_play_but_navigation_does_not_redirect_ai() {
    let (mut h, gate) = gated().await;
    let started = within(h.app.start_game(setup())).await.unwrap();
    document(&mut h.events, started.latest_node).await;
    within(h.app.play(play(Color::Black, 0, 0))).await.unwrap();
    let thinking = game(&h.app).await;
    assert!(thinking.ai_thinking);
    assert!(matches!(
        within(h.app.play(Move::Pass(Color::White))).await,
        Err(AppError::Game(GameError::AiTurn))
    ));
    assert!(matches!(
        within(h.app.delete_node(thinking.latest_node)).await,
        Err(AppError::GameRunning)
    ));
    assert!(matches!(
        within(h.app.promote_to_main(thinking.latest_node)).await,
        Err(AppError::GameRunning)
    ));
    assert!(matches!(
        within(h.app.set_comment("blocked".into())).await,
        Err(AppError::GameRunning)
    ));
    assert!(matches!(
        within(h.app.undo()).await,
        Err(AppError::GameRunning)
    ));
    assert!(matches!(
        within(h.app.open_bytes(b"(;)".to_vec())).await,
        Err(AppError::GameRunning)
    ));
    assert!(matches!(
        within(h.app.new_game(NewGameOptions::default())).await,
        Err(AppError::GameRunning)
    ));
    document(&mut h.events, thinking.latest_node).await;
    within(h.app.go_to(started.start_node)).await.unwrap();
    let browsed = document(&mut h.events, started.start_node).await;
    assert_eq!(browsed.board.unwrap().move_number, 0);
    let request = query(h.directory.path(), |q| {
        q["moves"].as_array().is_some_and(|m| m.len() == 1)
    })
    .await;
    assert_eq!(request["moves"], json!([["B", "A19"]]));
    std::fs::write(gate, "release").unwrap();
    let answered = moves(&h.app, 2).await;
    assert_ne!(answered.latest_node, started.start_node);
    let still_browsing = document(&mut h.events, started.start_node).await;
    assert_eq!(still_browsing.board.unwrap().move_number, 0);
    assert!(matches!(
        within(h.app.play(play(Color::Black, 1, 0))).await,
        Err(AppError::Game(GameError::NotLatestNode))
    ));
    within(h.app.go_to(answered.latest_node)).await.unwrap();
    within(h.app.stop_game()).await.unwrap();
    within(h.app.set_comment("allowed after stopping".into()))
        .await
        .unwrap();
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn take_back_before_ai_response_deletes_only_human_move_and_cancels_query() {
    let (h, gate) = gated().await;
    let started = within(h.app.start_game(setup())).await.unwrap();
    assert!(matches!(
        within(h.app.take_back()).await,
        Err(AppError::Game(GameError::CannotTakeBack))
    ));
    within(h.app.play(play(Color::Black, 0, 0))).await.unwrap();
    let request = query(h.directory.path(), |q| {
        q["moves"].as_array().is_some_and(|m| m.len() == 1)
    })
    .await;
    let taken = within(h.app.take_back()).await.unwrap();
    assert_eq!(taken.latest_node, started.start_node);
    assert_eq!(taken.move_number, 0);
    assert!(!taken.ai_thinking);
    let cancel = query(h.directory.path(), |q| q["terminateId"] == request["id"]).await;
    assert_eq!(cancel["action"], "terminate");
    std::fs::write(gate, "release stale result").unwrap();
    within(h.app.pause_game()).await.unwrap();
    within(h.app.resume_game()).await.unwrap();
    assert_eq!(within(h.app.variation_tree()).await.unwrap().nodes.len(), 1);
    assert!(matches!(
        within(h.app.take_back()).await,
        Err(AppError::Game(GameError::CannotTakeBack))
    ));
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn take_back_after_ai_response_removes_two_moves_and_never_passes_start() {
    let h = scripted(json!({"turns": {"1": {"candidates": [{"move": "D4"}]}}})).await;
    within(h.app.start_game(setup())).await.unwrap();
    within(h.app.play(play(Color::Black, 0, 0))).await.unwrap();
    let answered = moves(&h.app, 2).await;
    let before = within(h.app.variation_tree()).await.unwrap();
    assert_eq!(before.nodes.len(), 3);
    let state = within(h.app.take_back()).await.unwrap();
    assert_eq!(state.move_number, 0);
    assert_eq!(state.latest_node, state.start_node);
    assert_ne!(state.latest_node, answered.latest_node);
    assert_eq!(within(h.app.variation_tree()).await.unwrap().nodes.len(), 1);
    assert!(matches!(
        within(h.app.take_back()).await,
        Err(AppError::Game(GameError::CannotTakeBack))
    ));
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn pausing_cancels_ai_and_resume_starts_one_new_query_for_same_latest_node() {
    let (h, gate) = gated().await;
    within(h.app.start_game(setup())).await.unwrap();
    within(h.app.play(play(Color::Black, 0, 0))).await.unwrap();
    let first = query(h.directory.path(), |q| {
        q["moves"].as_array().is_some_and(|m| m.len() == 1)
    })
    .await;
    let paused = within(h.app.pause_game()).await.unwrap();
    assert_eq!(paused.phase, GamePhase::Paused);
    assert!(!paused.ai_thinking);
    query(h.directory.path(), |q| q["terminateId"] == first["id"]).await;
    // A paused game permits ordinary edits; commentary does not alter position.
    within(h.app.set_comment("paused annotation".into()))
        .await
        .unwrap();
    std::fs::write(gate, "release").unwrap();
    let resumed = within(h.app.resume_game()).await.unwrap();
    assert_eq!(resumed.phase, GamePhase::Playing);
    assert_eq!(resumed.latest_node, paused.latest_node);
    moves(&h.app, 2).await;
    assert_eq!(within(h.app.variation_tree()).await.unwrap().nodes.len(), 3);
    // The next AI request follows every request from the resumed turn on the wire.
    within(h.app.play(play(Color::Black, 1, 0))).await.unwrap();
    let recorded = requests_before(h.directory.path(), |q| {
        q["moves"].as_array().is_some_and(|m| m.len() == 3)
    })
    .await;
    let query_count = recorded
        .into_iter()
        .filter(|q| q["moves"].as_array().is_some_and(|m| m.len() == 1))
        .count();
    assert_eq!(query_count, 2);
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn changing_ai_limit_preserves_active_request_and_applies_on_next_ai_turn() {
    let (h, gate) = gated().await;
    within(h.app.start_game(GameSetup {
        white: Player::Ai(ai(7)),
        ..setup()
    }))
    .await
    .unwrap();
    within(h.app.play(play(Color::Black, 0, 0))).await.unwrap();
    let first = query(h.directory.path(), |q| {
        q["moves"].as_array().is_some_and(|m| m.len() == 1)
    })
    .await;
    assert_eq!(first["maxVisits"], 7);
    within(h.app.set_ai_player(Color::White, ai(23)))
        .await
        .unwrap();
    let current = game(&h.app).await;
    assert!(current.ai_thinking);
    assert_eq!(current.move_number, 1);
    std::fs::write(gate, "release").unwrap();
    moves(&h.app, 2).await;
    within(h.app.play(play(Color::Black, 1, 0))).await.unwrap();
    let next = query(h.directory.path(), |q| {
        q["moves"].as_array().is_some_and(|m| m.len() == 3)
    })
    .await;
    assert_eq!(next["maxVisits"], 23);
    // A later request is a wire barrier for any cancellation of the first turn.
    let recorded = requests_before(h.directory.path(), |q| q["id"] == next["id"]).await;
    assert!(!recorded.iter().any(|q| q["terminateId"] == first["id"]));
    moves(&h.app, 4).await;
    assert_eq!(
        within(h.app.settings()).await.unwrap().game_setup.white,
        Player::Ai(ai(23))
    );
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn best_move_uses_first_candidate_or_pv_and_each_is_exactly_one_undo() {
    let mut h = scripted(json!({"default": {"candidates": [
        {"move": "D4", "pv": ["D4", "Q16", "C3"]},
        {"move": "Q16", "pv": ["Q16", "D4", "R17"]}
    ]}}))
    .await;
    assert!(matches!(
        within(h.app.play_best_move(None)).await,
        Err(AppError::Game(GameError::NoAnalysis))
    ));
    within(h.app.set_pondering(true)).await.unwrap();
    within(async {
        loop {
            if let AppEvent::AnalysisUpdated(view) = h.events.recv().await.unwrap() {
                assert_eq!(view.node_id, 0);
                assert_eq!(view.candidates[0].coordinate, "D4");
                break;
            }
        }
    })
    .await;
    within(h.app.set_pondering(false)).await.unwrap();
    within(h.app.play_best_move(None)).await.unwrap();
    let one = within(h.app.variation_tree()).await.unwrap();
    assert_eq!(one.nodes.len(), 2);
    assert_eq!(one.nodes[1].mv, Some(play(Color::Black, 3, 15).into()));
    within(h.app.undo()).await.unwrap();
    assert_eq!(within(h.app.variation_tree()).await.unwrap().nodes.len(), 1);
    assert!(within(h.app.undo()).await.is_err());
    let pv_revision = within(h.app.play_best_move(Some(PvPick {
        candidate: 1,
        moves: 3,
    })))
    .await
    .unwrap();
    assert_eq!(changed(&mut h.events, pv_revision).await.tree.current, 0);
    let pv = within(h.app.variation_tree()).await.unwrap();
    assert_eq!(pv.nodes.len(), 4);
    assert_eq!(pv.nodes[1].mv, Some(play(Color::Black, 15, 3).into()));
    assert_eq!(pv.nodes[2].mv, Some(play(Color::White, 3, 15).into()));
    assert_eq!(pv.nodes[3].mv, Some(play(Color::Black, 16, 2).into()));
    within(h.app.undo()).await.unwrap();
    assert_eq!(within(h.app.variation_tree()).await.unwrap().nodes.len(), 1);
    assert!(within(h.app.undo()).await.is_err());
    within(h.app.redo()).await.unwrap();
    assert_eq!(
        within(h.app.variation_tree()).await.unwrap().nodes,
        pv.nodes
    );
    assert!(within(h.app.game_view()).await.unwrap().is_none());
    within(h.app.shutdown()).await.unwrap();
}

async fn changed(events: &mut app_core::AppSubscription, expected: u64) -> app_core::DocumentView {
    within(async {
        loop {
            if let AppEvent::DocumentChanged { revision, view } = events.recv().await.unwrap()
                && revision == expected
            {
                return view;
            }
        }
    })
    .await
}

async fn candidate_analysis(h: &mut support::Harness, node: u32) -> app_core::AnalysisView {
    within(h.app.set_pondering(true)).await.unwrap();
    let view = within(async {
        loop {
            if let AppEvent::AnalysisUpdated(view) = h.events.recv().await.unwrap()
                && view.node_id == node
            {
                return view;
            }
        }
    })
    .await;
    within(h.app.set_pondering(false)).await.unwrap();
    view
}

#[tokio::test]
async fn best_move_without_pv_reuses_an_existing_matching_child_without_adding_history() {
    let mut h = scripted(json!({"default": {"candidates": [{"move": "D4"}]}})).await;
    within(h.app.open_bytes(b"(;SZ[19];B[dp]C[keep];W[pd])".to_vec()))
        .await
        .unwrap();
    candidate_analysis(&mut h, 0).await;
    let before = within(h.app.variation_tree()).await.unwrap();
    let existing = before.nodes[0].children[0];
    let revision = within(h.app.play_best_move(None)).await.unwrap();
    let after = changed(&mut h.events, revision).await;
    assert_eq!(after.tree.current, existing);
    assert_eq!(after.comment.as_deref(), Some("keep"));
    assert!(!after.can_undo);
    assert_eq!(
        within(h.app.variation_tree()).await.unwrap().nodes,
        before.nodes
    );
    assert!(within(h.app.undo()).await.is_err());
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn best_move_without_pv_appends_one_missing_child_and_advances() {
    let mut h = scripted(json!({"default": {"candidates": [{"move": "D4"}]}})).await;
    candidate_analysis(&mut h, 0).await;
    let before = within(h.app.variation_tree()).await.unwrap();
    let revision = within(h.app.play_best_move(None)).await.unwrap();
    let after = changed(&mut h.events, revision).await;
    let tree = within(h.app.variation_tree()).await.unwrap();
    assert_eq!(tree.nodes.len(), before.nodes.len() + 1);
    assert_eq!(after.tree.current, tree.nodes[1].id);
    assert_eq!(tree.nodes[1].mv, Some(play(Color::Black, 3, 15).into()));
    assert!(after.can_undo);
    within(h.app.undo()).await.unwrap();
    assert_eq!(
        within(h.app.variation_tree()).await.unwrap().nodes,
        before.nodes
    );
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn best_move_pv_matches_add_variation_tree_focus_and_atomic_undo_redo() {
    for (source, node, advances) in [
        ("(;SZ[19];B[aa])", 0, false),
        ("(;SZ[19];B[aa])", 1, false),
        ("(;SZ[19];B[aa](;W[bb])(;W[cc]))", 3, true),
    ] {
        let mut picked = scripted(json!({"default": {"candidates": [
            {"move": "D4", "pv": ["D4", "Q16", "C3"]}
        ]}}))
        .await;
        let mut ordinary = harness(vec![]).await;
        for h in [&picked, &ordinary] {
            within(h.app.open_bytes(source.as_bytes().to_vec()))
                .await
                .unwrap();
            within(h.app.go_to(node)).await.unwrap();
        }
        let analysis = candidate_analysis(&mut picked, node).await;
        let pv = analysis.candidates[0]
            .pv
            .iter()
            .take(2)
            .copied()
            .map(Into::into)
            .collect();
        let before = within(ordinary.app.variation_tree()).await.unwrap();
        let ordinary_revision = within(ordinary.app.add_variation(pv)).await.unwrap();
        let ordinary_after = changed(&mut ordinary.events, ordinary_revision).await;
        let picked_revision = within(picked.app.play_best_move(Some(PvPick {
            candidate: 0,
            moves: 2,
        })))
        .await
        .unwrap();
        let picked_after = changed(&mut picked.events, picked_revision).await;
        let ordinary_tree = within(ordinary.app.variation_tree()).await.unwrap();
        assert_eq!(
            within(picked.app.variation_tree()).await.unwrap().nodes,
            ordinary_tree.nodes
        );
        assert_eq!(picked_after.tree.current, ordinary_after.tree.current);
        assert_eq!(picked_after.board, ordinary_after.board);
        assert_eq!(picked_after.tree.current != node, advances);
        assert_eq!(picked_after.can_undo, ordinary_after.can_undo);
        for h in [&mut picked, &mut ordinary] {
            let revision = within(h.app.undo()).await.unwrap();
            let undone = changed(&mut h.events, revision).await;
            assert_eq!(undone.tree.current, node);
            assert!(!undone.can_undo);
            assert_eq!(
                within(h.app.variation_tree()).await.unwrap().nodes,
                before.nodes
            );
            let revision = within(h.app.redo()).await.unwrap();
            let redone = changed(&mut h.events, revision).await;
            assert_eq!(redone.tree.current, ordinary_after.tree.current);
            assert_eq!(redone.board, ordinary_after.board);
            assert_eq!(
                within(h.app.variation_tree()).await.unwrap().nodes,
                ordinary_tree.nodes
            );
        }
        within(picked.app.shutdown()).await.unwrap();
        within(ordinary.app.shutdown()).await.unwrap();
    }
}

#[tokio::test]
async fn invalid_ai_results_pause_the_session_instead_of_leaving_a_stuck_ai_turn() {
    for candidates in [
        json!([]),
        json!([{"move": "A19"}]),
        json!([{"move": "invalid"}]),
    ] {
        let h = scripted(json!({"turns": {"1": {"candidates": candidates}}})).await;
        within(h.app.start_game(setup())).await.unwrap();
        within(h.app.play(play(Color::Black, 0, 0))).await.unwrap();
        let failed = within(async {
            loop {
                let view = h.app.game_view().await.unwrap().unwrap();
                if view.error.is_some() {
                    return view;
                }
                tokio::task::yield_now().await;
            }
        })
        .await;
        assert_eq!(failed.phase, GamePhase::Paused);
        assert!(!failed.ai_thinking);
        assert_eq!(failed.move_number, 1);
        assert_eq!(within(h.app.variation_tree()).await.unwrap().nodes.len(), 2);
        within(h.app.shutdown()).await.unwrap();
    }
}

#[tokio::test]
async fn changing_ai_timing_does_not_convert_an_active_human_player() {
    let h = harness(vec![]).await;
    within(h.app.start_game(GameSetup {
        white: Player::Human,
        ..setup()
    }))
    .await
    .unwrap();
    assert!(matches!(
        within(h.app.set_ai_player(Color::Black, ai(7))).await,
        Err(AppError::Game(GameError::InvalidSetup(_)))
    ));
    let view = game(&h.app).await;
    assert_eq!(view.black, Player::Human);
    assert_eq!(view.white, Player::Human);
    assert_eq!(
        within(h.app.settings()).await.unwrap().game_setup.black,
        Player::Human
    );
    within(h.app.pause_game()).await.unwrap();
    assert!(matches!(
        within(h.app.set_ai_player(Color::Black, ai(7))).await,
        Err(AppError::Game(GameError::InvalidSetup(_)))
    ));
    within(h.app.resume_game()).await.unwrap();
    within(h.app.play(play(Color::Black, 0, 0))).await.unwrap();
    assert_eq!(game(&h.app).await.to_play, ColorView::White);
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn paused_position_edits_are_allowed_but_invalidated_game_cannot_resume() {
    let h = harness(vec![]).await;
    within(h.app.start_game(GameSetup {
        white: Player::Human,
        ..setup()
    }))
    .await
    .unwrap();
    within(h.app.play(play(Color::Black, 0, 0))).await.unwrap();
    let paused = within(h.app.pause_game()).await.unwrap();
    within(h.app.delete_node(paused.latest_node)).await.unwrap();
    // Review requests share the engine FIFO without cancelling the live slot.
    within(h.app.start_whole_game_analysis(WholeGameOptions {
        visits: 997,
        ..WholeGameOptions::default()
    }))
    .await
    .unwrap();
    query(h.directory.path(), |q| {
        q["priority"] == -100 && q["maxVisits"] == 997
    })
    .await;
    let queries = requests(h.directory.path())
        .into_iter()
        .filter(|q| q.get("moves").is_some())
        .count();
    assert!(matches!(
        within(h.app.resume_game()).await,
        Err(AppError::Game(GameError::PositionChanged))
    ));
    assert_eq!(game(&h.app).await.phase, GamePhase::Paused);
    assert_eq!(within(h.app.variation_tree()).await.unwrap().nodes.len(), 1);
    // A higher visit bound cannot be satisfied by the first review's result.
    within(h.app.start_whole_game_analysis(WholeGameOptions {
        visits: 998,
        ..WholeGameOptions::default()
    }))
    .await
    .unwrap();
    let recorded = requests_before(h.directory.path(), |q| {
        q["priority"] == -100 && q["maxVisits"] == 998
    })
    .await;
    assert_eq!(
        recorded
            .into_iter()
            .filter(|q| q.get("moves").is_some())
            .count(),
        queries
    );
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn starting_and_stopping_an_unplayed_game_does_not_create_phantom_undo_history() {
    let mut h = harness(vec![]).await;
    let started = within(h.app.start_game(GameSetup {
        white: Player::Human,
        auto_save: false,
        ..setup()
    }))
    .await
    .unwrap();
    let initial = document(&mut h.events, started.latest_node).await;
    assert!(!initial.can_undo);
    assert!(!initial.can_redo);
    assert!(!initial.modified);
    let stopped = within(h.app.stop_game()).await.unwrap();
    assert_eq!(stopped.move_number, 0);
    assert_eq!(stopped.result, None);
    let after = document(&mut h.events, stopped.latest_node).await;
    assert!(!after.can_undo);
    assert!(!after.can_redo);
    assert!(!after.modified);
    assert!(within(h.app.undo()).await.is_err());
    within(h.app.set_comment("one real edit".into()))
        .await
        .unwrap();
    within(h.app.undo()).await.unwrap();
    assert!(!document(&mut h.events, stopped.latest_node).await.can_undo);
    within(h.app.shutdown()).await.unwrap();
}
