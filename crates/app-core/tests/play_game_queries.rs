#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;
#[path = "common/game.rs"]
mod support;

use app_core::{
    AiPlayer, AnalysisExtras, AppError, AppEvent, BatchOptions, BatchOutput, BatchPhase, ColorView,
    EngineRules, EngineSettings, GameError, GamePhase, GameSetup, GameStart, KataGoPlay, MoveView,
    Player, PointView, RandomOpening, WholeGameOptions, WholeGameStatus,
};
use common::within;
use go_core::{Color, Move, Point};
use serde_json::{Value, json};
use support::{ai, document, harness, moves, query, requests, requests_before, setup};

fn black_ai(play: KataGoPlay) -> GameSetup {
    GameSetup {
        black: Player::Ai(AiPlayer::KataGo(play)),
        white: Player::Human,
        ..setup()
    }
}

async fn scripted(script: Value, seed: u64) -> support::Harness {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("play.json");
    std::fs::write(&path, serde_json::to_vec(&script).unwrap()).unwrap();
    let mut config = support::config(directory.path());
    config.game_random_seed = Some(seed);
    support::harness_in(
        directory,
        config,
        vec!["--play-script".into(), path.to_str().unwrap().into()],
    )
    .await
}

fn last_move(view: &app_core::DocumentView) -> MoveView {
    view.board.as_ref().unwrap().last_move.unwrap()
}

#[tokio::test]
async fn seconds_visits_and_pure_network_finish_one_ai_move_with_the_requested_bound() {
    for (seconds, visits, pure, expected_visits, wire_visits) in [
        (Some(0.002), None, false, 200, u64::from(u32::MAX)),
        (None, Some(173), false, 173, 173),
        (None, None, true, 1, 1),
    ] {
        let mut h = harness(vec![]).await;
        // Ready has already observed the startup request; no game has run yet.
        let before = requests(h.directory.path()).len();
        let started = within(h.app.start_game(black_ai(KataGoPlay {
            seconds,
            visits,
            pure_network: pure,
            resign: None,
            random_opening: None,
        })))
        .await
        .unwrap();
        let state = moves(&h.app, 1).await;
        assert_eq!(state.phase, GamePhase::Playing);
        assert_eq!(state.to_play, ColorView::White);
        assert!(!state.ai_thinking);
        let view = document(&mut h.events, state.latest_node).await;
        assert_eq!(view.board.unwrap().move_number, 1);
        let result = within(h.app.whole_game_result(started.start_node))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result.visits, expected_visits);
        // The next AI request fences both this AI move and the following human
        // turn, including any unwanted live query queued after the move result.
        within(h.app.play(Move::Play(Color::White, Point { x: 0, y: 0 })))
            .await
            .unwrap();
        let recorded = requests_before(h.directory.path(), |q| {
            q["moves"].as_array().is_some_and(|m| m.len() == 2)
        })
        .await;
        let queries: Vec<_> = recorded[before..]
            .iter()
            .filter(|request| request.get("moves").is_some())
            .collect();
        assert_eq!(queries.len(), 1, "one AI move uses exactly one query");
        assert_eq!(queries[0]["maxVisits"], wire_visits);
        assert_eq!(queries[0]["overrideSettings"]["maxTime"].as_f64(), seconds);
        within(h.app.stop_game()).await.unwrap();
        within(h.app.shutdown()).await.unwrap();
    }
}

#[tokio::test]
async fn ai_visit_budget_can_outlast_the_response_timeout_while_partials_keep_arriving() {
    let directory = tempfile::tempdir().unwrap();
    let mut config = support::config(directory.path());
    config.game_query_timeout = std::time::Duration::from_secs(1);
    let mut h = support::harness_in(directory, config, vec![]).await;
    // The fake's 100,000 visits per simulated second require two seconds for
    // this budget. Its normal partials must refresh the one-second watchdog.
    let started = within(h.app.start_game(black_ai(KataGoPlay {
        seconds: None,
        visits: Some(200_000),
        resign: None,
        ..KataGoPlay::default()
    })))
    .await
    .unwrap();
    let mut saw_partial = false;
    let completed = within(async {
        loop {
            match h.events.recv().await.unwrap() {
                AppEvent::AnalysisUpdated(view)
                    if view.node_id == started.start_node && view.is_during_search =>
                {
                    saw_partial = true;
                }
                AppEvent::GameUpdated(view) => {
                    assert_eq!(
                        view.phase,
                        GamePhase::Playing,
                        "responsive AI paused: {view:?}"
                    );
                    if view.move_number == 1 {
                        break view;
                    }
                }
                _ => {}
            }
        }
    })
    .await;
    assert!(saw_partial);
    assert!(!completed.ai_thinking);
    assert_eq!(completed.to_play, ColorView::White);
    let result = within(h.app.whole_game_result(started.start_node))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.visits, 200_000);
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn random_opening_uses_movers_winrate_visit_share_and_seed_only_inside_move_range() {
    for color in [Color::Black, Color::White] {
        let (best_rate, eligible_rate, excluded_rate) = if color == Color::Black {
            (0.7, 0.698, 0.2)
        } else {
            (0.3, 0.302, 0.8)
        };
        let mut h = scripted(
            json!({"default":{
                "winrate":best_rate,
                "candidates":[
                    {"move":"D4","winrate":best_rate,"visitShare":1.0},
                    {"move":"Q16","winrate":eligible_rate,"visitShare":0.5},
                    {"move":"D16","winrate":excluded_rate,"visitShare":1.0},
                    {"move":"Q4","winrate":best_rate,"visitShare":0.001}
                ]
            }}),
            0,
        )
        .await;
        let player = Player::Ai(AiPlayer::KataGo(KataGoPlay {
            seconds: None,
            visits: Some(1000),
            resign: None,
            random_opening: Some(RandomOpening {
                within_moves: 1,
                max_winrate_drop: 0.005,
                min_visit_share: 0.05,
            }),
            ..KataGoPlay::default()
        }));
        let root = if color == Color::Black { "B" } else { "W" };
        let mut chosen = Vec::new();
        for _ in 0..2 {
            within(
                h.app
                    .open_bytes(format!("(;SZ[19]PL[{root}])").into_bytes()),
            )
            .await
            .unwrap();
            let mut settings = GameSetup {
                start: GameStart::CurrentPosition,
                black: Player::Human,
                white: Player::Human,
                ..setup()
            };
            *settings.player_mut(color) = player.clone();
            within(h.app.start_game(settings)).await.unwrap();
            let state = moves(&h.app, 1).await;
            let board = document(&mut h.events, state.latest_node).await;
            chosen.push(last_move(&board));
            within(h.app.stop_game()).await.unwrap();
        }
        assert_eq!(chosen[0], chosen[1], "the same injected seed is repeatable");
        assert_eq!(
            chosen[0],
            MoveView::Play {
                color: color.into(),
                point: PointView { x: 15, y: 3 }
            },
            "seed 0 selects the eligible alternative, never the worse-rate or low-visit candidate"
        );

        let source = if color == Color::Black {
            b"(;SZ[19];B[];W[])".to_vec()
        } else {
            b"(;SZ[19]PL[W];W[];B[])".to_vec()
        };
        within(h.app.open_bytes(source)).await.unwrap();
        within(h.app.step(2)).await.unwrap();
        let mut settings = GameSetup {
            start: GameStart::CurrentPosition,
            black: Player::Human,
            white: Player::Human,
            ..setup()
        };
        *settings.player_mut(color) = player;
        within(h.app.start_game(settings)).await.unwrap();
        let state = moves(&h.app, 3).await;
        let board = document(&mut h.events, state.latest_node).await;
        assert_eq!(
            last_move(&board),
            MoveView::Play {
                color: color.into(),
                point: PointView { x: 3, y: 15 }
            },
            "outside the opening range, the first candidate always wins"
        );
        within(h.app.shutdown()).await.unwrap();
    }
}

#[tokio::test]
async fn best_pass_is_played_even_when_random_opening_has_an_eligible_alternative() {
    let mut h = scripted(
        json!({"default":{"winrate":0.6,"candidates":[
            {"move":"pass","visitShare":1.0},
            {"move":"Q16","visitShare":0.5}
        ]}}),
        0,
    )
    .await;
    within(h.app.start_game(black_ai(KataGoPlay {
        seconds: None,
        visits: Some(100),
        resign: None,
        random_opening: Some(RandomOpening::default()),
        ..KataGoPlay::default()
    })))
    .await
    .unwrap();
    let state = moves(&h.app, 1).await;
    let view = document(&mut h.events, state.latest_node).await;
    assert_eq!(
        last_move(&view),
        MoveView::Pass {
            color: ColorView::Black
        }
    );
    assert!(view.board.unwrap().stones.iter().all(Option::is_none));
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn ponder_off_suppresses_human_queries_when_the_saved_analysis_intent_is_on() {
    let h = harness(vec![]).await;
    within(h.app.set_pondering(true)).await.unwrap();
    let ordinary = query(h.directory.path(), |q| q.get("moves").is_some()).await;
    within(h.app.start_game(setup())).await.unwrap();
    query(h.directory.path(), |q| q["terminateId"] == ordinary["id"]).await;
    within(h.app.play(Move::Play(Color::Black, Point { x: 0, y: 0 })))
        .await
        .unwrap();
    // The AI request closes the initial human turn's request-log interval.
    let game_start = requests_before(h.directory.path(), |q| {
        q["moves"].as_array().is_some_and(|m| m.len() == 1)
    })
    .await;
    let cancelled = game_start
        .iter()
        .position(|q| q["terminateId"] == ordinary["id"])
        .unwrap();
    assert!(
        game_start[cancelled + 1..]
            .iter()
            .all(|q| q.get("moves").is_none()),
        "starting a ponder-off human turn must not submit a replacement live search"
    );
    moves(&h.app, 2).await;
    within(h.app.play(Move::Play(Color::Black, Point { x: 1, y: 0 })))
        .await
        .unwrap();
    // Fence the second human turn too; exclude this next AI request itself.
    let recorded = requests_before(h.directory.path(), |q| {
        q["moves"].as_array().is_some_and(|m| m.len() == 3)
    })
    .await;
    let queries: Vec<_> = recorded[cancelled + 1..]
        .iter()
        .filter(|q| q.get("moves").is_some())
        .collect();
    assert_eq!(queries.len(), 1, "only the AI turn searches");
    assert_eq!(queries[0]["maxVisits"], 1);
    assert_eq!(queries[0]["moves"].as_array().unwrap().len(), 1);
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn ponder_off_suppresses_supplemental_queries_before_and_during_human_turns() {
    let h = harness(vec![]).await;
    within(h.app.set_analysis_extras(AnalysisExtras {
        policy: true,
        moves_ownership: true,
    }))
    .await
    .unwrap();
    let supplement = query(h.directory.path(), |q| q.get("moves").is_some()).await;
    // The first supplemental query has completed or will be cancelled by start.
    let before = requests(h.directory.path()).len();
    within(h.app.start_game(setup())).await.unwrap();
    within(h.app.set_analysis_extras(AnalysisExtras {
        policy: false,
        moves_ownership: false,
    }))
    .await
    .unwrap();
    within(h.app.set_analysis_extras(AnalysisExtras {
        policy: true,
        moves_ownership: true,
    }))
    .await
    .unwrap();
    within(h.app.play(Move::Play(Color::Black, Point { x: 0, y: 0 })))
        .await
        .unwrap();
    moves(&h.app, 2).await;
    within(h.app.play(Move::Play(Color::Black, Point { x: 1, y: 0 })))
        .await
        .unwrap();
    // Wait past both human turns before asserting that neither sent extras.
    let recorded = requests_before(h.directory.path(), |q| {
        q["moves"].as_array().is_some_and(|m| m.len() == 3)
    })
    .await;
    let queries: Vec<_> = recorded[before..]
        .iter()
        .filter(|q| q.get("moves").is_some() && q["id"] != supplement["id"])
        .collect();
    assert_eq!(
        queries.len(),
        1,
        "ponder-off turns must not request supplemental analysis"
    );
    assert_eq!(queries[0]["moves"].as_array().unwrap().len(), 1);
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn rules_and_komi_overrides_reject_start_and_resume_without_changing_the_game() {
    let h = harness(vec![]).await;
    for overridden in [
        EngineSettings {
            rules: EngineRules::Japanese,
            ..EngineSettings::default()
        },
        EngineSettings {
            read_komi: false,
            default_komi: 6.5,
            ..EngineSettings::default()
        },
    ] {
        within(h.app.set_engine_settings(overridden.clone()))
            .await
            .unwrap();
        let original = within(h.app.variation_tree()).await.unwrap();
        assert!(matches!(
            within(h.app.start_game(setup())).await,
            Err(AppError::Game(GameError::IncompatibleMode))
        ));
        assert_eq!(within(h.app.variation_tree()).await.unwrap(), original);
        within(h.app.set_engine_settings(EngineSettings::default()))
            .await
            .unwrap();
        within(h.app.start_game(setup())).await.unwrap();
        let paused = within(h.app.pause_game()).await.unwrap();
        within(h.app.set_engine_settings(overridden)).await.unwrap();
        assert!(matches!(
            within(h.app.resume_game()).await,
            Err(AppError::Game(GameError::IncompatibleMode))
        ));
        assert_eq!(support::game(&h.app).await, paused);
        within(h.app.set_engine_settings(EngineSettings::default()))
            .await
            .unwrap();
        within(h.app.resume_game()).await.unwrap();
        within(h.app.stop_game()).await.unwrap();
    }
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn pure_network_is_mutually_exclusive_and_start_or_resume_cancels_whole_game_search() {
    let h = scripted(json!({"default":{"hang":true}}), 0).await;
    within(h.app.set_pure_network(true)).await.unwrap();
    assert!(matches!(
        within(h.app.start_game(setup())).await,
        Err(AppError::PureNetworkActive)
    ));
    within(h.app.set_pure_network(false)).await.unwrap();
    let mut settings = setup();
    settings.start = GameStart::CurrentPosition;
    let options = WholeGameOptions {
        visits: 200,
        ..WholeGameOptions::default()
    };
    within(h.app.start_whole_game_analysis(options))
        .await
        .unwrap();
    let review = query(h.directory.path(), |q| q["priority"] == -100).await;
    within(h.app.start_game(settings)).await.unwrap();
    assert_eq!(
        within(h.app.whole_game_progress())
            .await
            .unwrap()
            .unwrap()
            .status,
        WholeGameStatus::Cancelled
    );
    query(h.directory.path(), |q| q["terminateId"] == review["id"]).await;
    assert!(matches!(
        within(h.app.set_pure_network(true)).await,
        Err(AppError::GameRunning)
    ));
    assert!(matches!(
        within(h.app.start_whole_game_analysis(options)).await,
        Err(AppError::GameRunning)
    ));
    within(h.app.pause_game()).await.unwrap();
    within(h.app.set_pure_network(true)).await.unwrap();
    assert!(matches!(
        within(h.app.resume_game()).await,
        Err(AppError::PureNetworkActive)
    ));
    within(h.app.set_pure_network(false)).await.unwrap();
    within(h.app.start_whole_game_analysis(options))
        .await
        .unwrap();
    let resumed_review = query(h.directory.path(), |q| {
        q["priority"] == -100 && q["id"] != review["id"]
    })
    .await;
    within(h.app.resume_game()).await.unwrap();
    assert_eq!(
        within(h.app.whole_game_progress())
            .await
            .unwrap()
            .unwrap()
            .status,
        WholeGameStatus::Cancelled
    );
    query(h.directory.path(), |q| {
        q["terminateId"] == resumed_review["id"]
    })
    .await;
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn running_and_paused_batches_reject_games_and_games_reject_batch_start() {
    let h = scripted(json!({"default":{"hang":true}}), 0).await;
    let path = h.directory.path().join("input.sgf");
    std::fs::write(&path, b"(;SZ[19];B[aa];W[bb])").unwrap();
    let options = BatchOptions {
        files: vec![path],
        visits: 200,
        scope: Default::default(),
        force: false,
        output: BatchOutput {
            suffix: "review".into(),
            directory: None,
        },
    };
    let batch = within(h.app.start_batch_analysis(options.clone()))
        .await
        .unwrap();
    assert_eq!(batch.phase, BatchPhase::Running);
    assert!(matches!(
        within(h.app.start_game(setup())).await,
        Err(AppError::BatchAnalysisRunning)
    ));
    assert!(matches!(
        within(h.app.continue_game(None)).await,
        Err(AppError::BatchAnalysisRunning)
    ));
    within(h.app.pause_batch_analysis()).await.unwrap();
    assert!(matches!(
        within(h.app.start_game(setup())).await,
        Err(AppError::BatchAnalysisRunning)
    ));
    within(h.app.stop_batch_analysis()).await.unwrap();
    within(h.app.start_game(setup())).await.unwrap();
    assert!(matches!(
        within(h.app.start_batch_analysis(options)).await,
        Err(AppError::GameRunning)
    ));
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn ai_queries_replace_live_queries_and_late_cancelled_results_cannot_play_a_new_position() {
    let directory = tempfile::tempdir().unwrap();
    let gate0 = directory.path().join("turn0");
    let gate1 = directory.path().join("turn1");
    let gate2 = directory.path().join("turn2");
    let path = directory.path().join("play.json");
    std::fs::write(
        &path,
        json!({"turns":{
            "0":{"gateFile":gate0,"candidates":[{"move":"D4"}]},
            "1":{"gateFile":gate1,"candidates":[{"move":"Q4"}]},
            "2":{"gateFile":gate2,"candidates":[{"move":"D16"}]}
        }})
        .to_string(),
    )
    .unwrap();
    let config = support::config(directory.path());
    let mut h = support::harness_in(
        directory,
        config,
        vec!["--play-script".into(), path.to_str().unwrap().into()],
    )
    .await;
    within(h.app.open_bytes(b"(;SZ[19];B[aa];W[bb])".to_vec()))
        .await
        .unwrap();
    within(h.app.set_pondering(true)).await.unwrap();
    let ordinary = query(h.directory.path(), |q| q.get("moves").is_some()).await;
    within(h.app.start_game(GameSetup {
        black: Player::Ai(ai(123)),
        white: Player::Human,
        start: GameStart::CurrentPosition,
        ponder: true,
        ..setup()
    }))
    .await
    .unwrap();
    let first = query(h.directory.path(), |q| q["maxVisits"] == 123).await;
    query(h.directory.path(), |q| q["terminateId"] == ordinary["id"]).await;
    within(h.app.go_to(2)).await.unwrap();
    document(&mut h.events, 2).await;
    // Changing payload preferences during the AI turn cannot reopen the
    // coordinator's live slot while the app-owned bounded query is running.
    within(h.app.set_analysis_extras(AnalysisExtras {
        policy: true,
        moves_ownership: true,
    }))
    .await
    .unwrap();
    std::fs::write(&gate0, "released").unwrap();
    let first_move = moves(&h.app, 1).await;
    assert_ne!(first_move.latest_node, 2);
    within(async {
        loop {
            match h.events.recv().await.unwrap() {
                AppEvent::AnalysisUpdated(view) => assert_eq!(view.node_id, 2),
                AppEvent::DocumentChanged { view, .. } => assert_eq!(view.tree.current, 2),
                AppEvent::GameUpdated(view)
                    if view.latest_node == first_move.latest_node && !view.ai_thinking =>
                {
                    break;
                }
                _ => {}
            }
        }
    })
    .await;
    let human_query = query(h.directory.path(), |q| q["moves"] == json!([["B", "D4"]])).await;
    // The following human request also fences the extras change during AI play.
    let log = requests_before(h.directory.path(), |q| q["id"] == human_query["id"]).await;
    let first_index = log.iter().position(|q| q["id"] == first["id"]).unwrap();
    let cancelled = log
        .iter()
        .position(|q| q["terminateId"] == ordinary["id"])
        .unwrap();
    assert!(cancelled < first_index);
    assert_eq!(log.iter().filter(|q| q.get("moves").is_some()).count(), 2);
    within(h.app.go_to(first_move.latest_node)).await.unwrap();
    within(h.app.play(Move::Play(Color::White, Point { x: 15, y: 3 })))
        .await
        .unwrap();
    let second = query(h.directory.path(), |q| {
        q["moves"] == json!([["B", "D4"], ["W", "Q16"]]) && q["maxVisits"] == 123
    })
    .await;
    query(h.directory.path(), |q| {
        q["terminateId"] == human_query["id"]
    })
    .await;
    std::fs::write(&gate1, "old position released").unwrap();
    std::fs::write(&gate2, "current position released").unwrap();
    let final_state = moves(&h.app, 3).await;
    assert_eq!(final_state.move_number, 3);
    let view = document(&mut h.events, final_state.latest_node).await;
    assert_eq!(
        last_move(&view),
        MoveView::Play {
            color: ColorView::Black,
            point: PointView { x: 3, y: 3 },
        }
    );
    assert_eq!(
        view.board.unwrap().stones[15 * 19 + 15],
        None,
        "cancelled Q4 candidate was not played"
    );
    // A later human-turn request closes the second AI turn's interval.
    let log = requests_before(h.directory.path(), |q| {
        q["moves"] == json!([["B", "D4"], ["W", "Q16"], ["B", "D16"]])
    })
    .await;
    let human_cancelled = log
        .iter()
        .position(|q| q["terminateId"] == human_query["id"])
        .unwrap();
    let second_index = log.iter().position(|q| q["id"] == second["id"]).unwrap();
    assert!(human_cancelled < second_index);
    assert_eq!(
        log.iter().filter(|q| q["moves"] == second["moves"]).count(),
        1
    );
    within(h.app.shutdown()).await.unwrap();
}
