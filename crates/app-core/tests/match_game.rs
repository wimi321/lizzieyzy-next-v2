#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
#[path = "common/game.rs"]
mod support;
use app_core::{
    AppCore, AppError, GameStart, KataGoPlay, MatchError, MatchPhase, MatchPlayer, MatchSetup,
    MatchSide, MatchView, NewGameOptions,
};
use common::within;
use serde_json::{Value, json};
use support::*;

async fn scripted(script: Value) -> Harness {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("play.json");
    std::fs::write(&path, script.to_string()).unwrap();
    let mut cfg = config(directory.path());
    cfg.game_random_seed = Some(42);
    let launch = directory.path().join("launches.jsonl");
    harness_in(
        directory,
        cfg,
        vec![
            "--play-script".into(),
            path.to_string_lossy().into(),
            "--launch-log".into(),
            launch.to_string_lossy().into(),
        ],
    )
    .await
}
async fn setup_match(app: &AppCore) -> MatchSetup {
    let id = within(app.settings())
        .await
        .unwrap()
        .main_engine_id
        .unwrap();
    let side = |wrn, pda| MatchPlayer::KataGo {
        engine_id: id.clone(),
        play: KataGoPlay {
            seconds: None,
            visits: Some(1),
            resign: None,
            ..Default::default()
        },
        wrn,
        pda,
    };
    MatchSetup {
        a: side(0.2, 0.1),
        b: side(0.7, -0.1),
        start: GameStart::NewBoard(NewGameOptions {
            width: 9,
            height: 9,
            ..Default::default()
        }),
        auto_save: false,
        ..Default::default()
    }
}
async fn completed(app: &AppCore, n: u32) -> MatchView {
    // A new safety window for every game, never one deadline for the batch.
    let mut last = None;
    for expected in 1..=n {
        last = Some(
            within(async {
                loop {
                    let view = app.match_view().await.unwrap().unwrap();
                    if view.completed >= expected {
                        return view;
                    }
                    assert_ne!(view.phase, MatchPhase::Stopped);
                    tokio::task::yield_now().await;
                }
            })
            .await,
        );
    }
    within(async {
        loop {
            let view = app.match_view().await.unwrap().unwrap();
            if view.phase == MatchPhase::Completed {
                return view;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    last.unwrap()
}
#[tokio::test]
async fn shared_main_engine_exchanges_colors_scores_and_applies_each_sides_overrides() {
    let mut h = scripted(json!({"default":{"scoreLead":2.1,"candidates":[{"move":"pass"}]}})).await;
    let mut setup = setup_match(&h.app).await;
    setup.games = 2;
    within(h.app.start_match(setup)).await.unwrap();
    let view = completed(&h.app, 2).await;
    assert_eq!(view.a.total, 1);
    assert_eq!(view.b.total, 1);
    assert_eq!(view.a.black, 1);
    assert_eq!(view.b.black, 1);
    assert!(view.unscored.is_empty());
    assert_eq!(view.elo.unwrap().elo, 0.0);
    let requests = requests(h.directory.path());
    let plays: Vec<_> = requests.iter().filter(|q| q["maxVisits"] == 1).collect();
    assert_eq!(plays.len(), 4);
    assert_eq!(plays[0]["overrideSettings"]["wideRootNoise"], 0.2);
    assert_eq!(plays[1]["overrideSettings"]["wideRootNoise"], 0.7);
    assert_eq!(
        plays[2]["overrideSettings"]["playoutDoublingAdvantage"],
        -0.1
    );
    assert_eq!(
        std::fs::read_to_string(h.directory.path().join("launches.jsonl"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert!(h.app.settings().await.unwrap().engine.is_some());
    within(async {
        loop {
            if let app_core::AppEvent::MatchUpdated(event) = h.events.recv().await.unwrap()
                && event.phase == MatchPhase::Completed
            {
                assert_eq!(event.completed, 2);
                break;
            }
        }
    })
    .await;
    within(h.app.shutdown()).await.unwrap();
}
#[tokio::test]
async fn sequential_and_seeded_random_openings_start_at_mainline_end_and_stop_at_limit() {
    for random in [false, true] {
        let h = scripted(json!({"default":{"candidates":[{"move":"pass"}]}})).await;
        let a = h.directory.path().join("a.sgf");
        let b = h.directory.path().join("b.sgf");
        std::fs::write(&a, "(;SZ[9]KM[6.5];B[aa](;W[bb])(;W[cc]))").unwrap();
        std::fs::write(&b, "(;SZ[9]KM[6.5];B[dd];W[ee])").unwrap();
        let mut setup = setup_match(&h.app).await;
        setup.games = 3;
        setup.openings = vec![a.clone(), b.clone()];
        setup.max_moves = Some(2);
        setup.random_openings = random;
        within(h.app.start_match(setup)).await.unwrap();
        let view = completed(&h.app, 3).await;
        assert_eq!(view.unscored["超手数"], 3);
        assert_eq!(view.a.total + view.b.total, 0);
        let counts: std::collections::BTreeMap<_, _> = view
            .openings
            .iter()
            .map(|o| (o.opening.clone(), o.completed))
            .collect();
        if random {
            // SplitMix64 seeded with 42 selects indexes 1,1,0.
            assert_eq!(counts[&a.to_string_lossy().into_owned()], 1);
            assert_eq!(counts[&b.to_string_lossy().into_owned()], 2);
        } else {
            assert_eq!(counts[&a.to_string_lossy().into_owned()], 2);
            assert_eq!(counts[&b.to_string_lossy().into_owned()], 1);
        }
        let game = h.app.game_view().await.unwrap().unwrap();
        assert_eq!(game.move_number, 2);
        assert!(
            requests(h.directory.path())
                .iter()
                .all(|q| q.get("moves").is_none())
        );
        within(h.app.shutdown()).await.unwrap();
    }
}
#[tokio::test]
async fn query_errors_and_broken_openings_are_unscored_and_batch_progresses() {
    let h = scripted(json!({"default":{"error":"scripted failure"}})).await;
    let mut setup = setup_match(&h.app).await;
    setup.games = 2;
    within(h.app.start_match(setup)).await.unwrap();
    let view = completed(&h.app, 2).await;
    assert_eq!(view.unscored.values().sum::<u32>(), 2);
    assert_eq!(view.a.total + view.b.total, 0);
    let mut setup = setup_match(&h.app).await;
    setup.openings = vec![h.directory.path().join("missing.sgf")];
    within(h.app.start_match(setup)).await.unwrap();
    let view = completed(&h.app, 1).await;
    assert!(matches!(view.error, Some(MatchError::Opening { .. })));
    assert_eq!(view.unscored.values().sum::<u32>(), 1);
    within(h.app.shutdown()).await.unwrap();
}
#[tokio::test]
async fn pause_resume_params_play_now_manual_resign_and_reduce_game_count() {
    let h = scripted(json!({"default":{"candidates":[{"move":"A1"}]}})).await;
    let mut setup = setup_match(&h.app).await;
    setup.games = 3;
    setup.max_moves = Some(2);
    for player in [&mut setup.a, &mut setup.b] {
        if let MatchPlayer::KataGo { play, .. } = player {
            play.visits = Some(1_000_000);
        }
    }
    within(h.app.start_match(setup)).await.unwrap();
    query(h.directory.path(), |q| q["maxVisits"] == 1_000_000).await;
    assert_eq!(
        within(h.app.pause_match()).await.unwrap().phase,
        MatchPhase::Paused
    );
    assert!(matches!(
        within(h.app.start_game(support::setup())).await,
        Err(AppError::Match(MatchError::AlreadyActive))
    ));
    within(h.app.set_match_side_params(MatchSide::A, 1.2, 0.8))
        .await
        .unwrap();
    within(h.app.resume_match()).await.unwrap();
    query(h.directory.path(), |q| {
        q["overrideSettings"]["wideRootNoise"] == 1.2
    })
    .await;
    within(async {
        loop {
            match h.app.match_play_now().await {
                Ok(_) => break,
                Err(AppError::Game(app_core::GameError::NoAnalysis)) => {
                    tokio::task::yield_now().await
                }
                Err(error) => panic!("{error}"),
            }
        }
    })
    .await;
    assert_eq!(h.app.game_view().await.unwrap().unwrap().move_number, 1);
    assert_eq!(within(h.app.set_match_games(0)).await.unwrap().games, 1);
    within(h.app.match_resign(go_core::Color::White))
        .await
        .unwrap();
    let view = completed(&h.app, 1).await;
    assert_eq!(view.a.total, 1);
    within(h.app.shutdown()).await.unwrap();
}
#[tokio::test]
async fn stopped_current_game_is_not_saved_and_finished_game_uses_match_filename() {
    let h = scripted(json!({"default":{"hang":true}})).await;
    let mut setup = setup_match(&h.app).await;
    setup.auto_save = true;
    let started = within(h.app.start_match(setup.clone())).await.unwrap();
    let directory = std::path::PathBuf::from(started.save_directory.unwrap());
    assert_eq!(within(h.app.stop_match()).await.unwrap().completed, 0);
    assert!(!directory.exists());
    within(h.app.start_match(setup)).await.unwrap();
    within(h.app.match_resign(go_core::Color::Black))
        .await
        .unwrap();
    let view = completed(&h.app, 1).await;
    assert_eq!(view.b.total, 1);
    let directory = std::path::PathBuf::from(view.save_directory.unwrap());
    assert!(directory.parent().unwrap().ends_with("EngineGames"));
    let files: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|v| v.unwrap().path())
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0].file_name().unwrap().to_string_lossy(),
        "1_KataGo_vs_KataGo_W+R.sgf"
    );
    assert!(
        std::fs::read_to_string(&files[0])
            .unwrap()
            .contains("RE[W+R]")
    );
    within(h.app.shutdown()).await.unwrap();
}
#[tokio::test]
async fn extra_katago_entry_is_spawned_once_for_both_players() {
    let h = scripted(json!({"default":{"candidates":[{"move":"pass"}]}})).await;
    let mut profile = h.app.settings().await.unwrap().engine.unwrap();
    let log = h.directory.path().join("extra-launch.jsonl");
    let i = profile
        .extra_args
        .iter()
        .position(|s| s == "--launch-log")
        .unwrap();
    profile.extra_args[i + 1] = log.to_string_lossy().into();
    let exit_log = h.directory.path().join("extra-exit.log");
    profile
        .extra_args
        .extend(["--exit-log".into(), exit_log.to_string_lossy().into()]);
    let settings = within(h.app.add_engine("Second".into(), profile))
        .await
        .unwrap();
    let id = settings
        .engines
        .iter()
        .find(|e| e.name == "Second")
        .unwrap()
        .id
        .clone();
    let mut setup = setup_match(&h.app).await;
    for player in [&mut setup.a, &mut setup.b] {
        if let MatchPlayer::KataGo { engine_id, .. } = player {
            *engine_id = id.clone()
        }
    }
    within(h.app.start_match(setup)).await.unwrap();
    completed(&h.app, 1).await;
    assert_eq!(std::fs::read_to_string(log).unwrap().lines().count(), 1);
    assert_eq!(
        std::fs::read_to_string(exit_log).unwrap().lines().count(),
        1
    );
    assert!(
        within(h.app.whole_game_result(0)).await.unwrap().is_none(),
        "side-specific match analysis cannot seed the normal engine cache"
    );
    // The borrowed main session is still usable after the match closed its own one.
    within(h.app.start_game(support::setup())).await.unwrap();
    within(h.app.stop_game()).await.unwrap();
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn stopping_second_game_keeps_completed_standings_and_only_first_saved_sgf() {
    let h = scripted(json!({"default":{"hang":true}})).await;
    let mut setup = setup_match(&h.app).await;
    setup.games = 3;
    setup.auto_save = true;
    let started = within(h.app.start_match(setup)).await.unwrap();
    let directory = std::path::PathBuf::from(started.save_directory.unwrap());
    within(h.app.match_resign(go_core::Color::Black))
        .await
        .unwrap();
    // Wait only for the first completed game and the next game boundary.
    // The script prevents the second game from finishing independently.
    let first = within(async {
        loop {
            let view = h.app.match_view().await.unwrap().unwrap();
            if view.completed == 1 && view.current_game == 2 {
                return view;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert_eq!(first.b.total, 1);
    assert_eq!(first.b.white, 1);
    assert_eq!(first.a.total, 0);
    let stopped = within(h.app.stop_match()).await.unwrap();
    assert_eq!(stopped.phase, MatchPhase::Stopped);
    assert_eq!(stopped.completed, 1);
    assert_eq!(stopped.current_game, 2);
    assert_eq!(stopped.a, first.a);
    assert_eq!(stopped.b, first.b);
    assert_eq!(stopped.draws, first.draws);
    assert_eq!(stopped.unscored, first.unscored);
    assert_eq!(within(h.app.match_view()).await.unwrap(), Some(stopped));
    let files: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(files.len(), 1, "the stopped second game must not be saved");
    assert_eq!(
        files[0].file_name().unwrap().to_string_lossy(),
        "1_KataGo_vs_KataGo_W+R.sgf"
    );
    assert!(
        std::fs::read_to_string(&files[0])
            .unwrap()
            .contains("RE[W+R]")
    );
    within(h.app.shutdown()).await.unwrap();
}
