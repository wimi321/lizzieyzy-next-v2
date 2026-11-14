#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{
    AppConfig, AppCore, AppError, AppEvent, AppSubscription, CoachColor, CoachError, CoachMode,
    CoachPace, CoachRank, CoachSetup, ColorView, EngineProfile, EngineStatus, GameEndReason,
    GamePhase, HumanSlModelStatus, Settings, assets::AssetCatalog,
};
use go_core::{Color, Move, Point};
use serde_json::{Value, json};
use std::{future::Future, path::Path, time::Duration};

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), Box::pin(future))
        .await
        .expect("public coaching API safety deadline")
}

struct Harness {
    directory: tempfile::TempDir,
    app: AppCore,
    events: AppSubscription,
}

async fn harness(model_present: bool) -> Harness {
    harness_with_args(model_present, &[]).await
}

async fn harness_with_args(model_present: bool, extra_args: &[&str]) -> Harness {
    // Fake compilation is preparation, outside all behavior deadlines.
    let binary = fake_katago::binary_path().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let program = directory
        .path()
        .join(format!("katago-eigen{}", std::env::consts::EXE_SUFFIX));
    std::fs::hard_link(&binary, &program)
        .or_else(|_| std::fs::copy(&binary, &program).map(|_| ()))
        .unwrap();
    let model = directory.path().join("main.bin");
    let config = directory.path().join("analysis.cfg");
    let human = directory.path().join("weights/human-sl.bin.gz");
    std::fs::write(&model, b"main model").unwrap();
    std::fs::write(&config, "reportAnalysisWinratesAs = BLACK\n").unwrap();
    if model_present {
        std::fs::create_dir_all(human.parent().unwrap()).unwrap();
        std::fs::write(&human, b"human-model").unwrap();
    }
    let mut policy = vec![0.0; 82];
    policy[9] = 1.0;
    let script = directory.path().join("script.json");
    std::fs::write(
        &script,
        serde_json::to_vec(&json!({"profiles":{"rank_20k":{"turns":{
            "1":{"humanPolicy":policy,"candidates":[{"move":"A8","visitShare":1.0}]}
        }}}}))
        .unwrap(),
    )
    .unwrap();
    within(
        Settings {
            pondering: false,
            auto_quick_analyze_on_load: false,
            cache_enabled: false,
            ..Settings::default()
        }
        .save(directory.path()),
    )
    .await
    .unwrap();
    let mut tuning = AppConfig {
        asset_catalog: Some(AssetCatalog::test_weight(
            "human-sl",
            "http://127.0.0.1/unused",
            b"human-model",
        )),
        game_random_seed: Some(1234),
        games_directory: Some(directory.path().join("MyGames")),
        ..AppConfig::default()
    };
    tuning.analysis.live.max_visits = 20;
    tuning.session.max_restarts = 0;
    let (app, mut events) = within(crate::spawn_with_config(directory.path(), tuning))
        .await
        .unwrap();
    within(
        app.configure_engine(Some(EngineProfile {
            program,
            model,
            config,
            // A missing remembered optional file must not prevent main-engine startup.
            human_model: (!model_present).then_some(human),
            canonical_model_name: None,
            extra_args: [
                vec![
                    "--play-script".into(),
                    script.to_string_lossy().into_owned(),
                    "--visits-per-sec".into(),
                    "1000000".into(),
                    "--request-log".into(),
                    directory
                        .path()
                        .join("requests.jsonl")
                        .to_string_lossy()
                        .into_owned(),
                ],
                extra_args.iter().map(|arg| (*arg).to_owned()).collect(),
            ]
            .concat(),
        })),
    )
    .await
    .unwrap();
    within(async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::EngineStatusChanged(EngineStatus::Ready { .. }) => break,
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("main engine failed: {reason}");
                }
                _ => {}
            }
        }
    })
    .await;
    Harness {
        directory,
        app,
        events,
    }
}

fn setup() -> CoachSetup {
    CoachSetup {
        width: 9,
        height: 9,
        rank: CoachRank::Kyu(20),
        color: CoachColor::Black,
        pace: CoachPace::Fast,
        mode: CoachMode::ReviewAfter,
        auto_save: false,
        ..CoachSetup::default()
    }
}

async fn query(directory: &Path, predicate: impl Fn(&Value) -> bool) -> Value {
    within(async {
        loop {
            let recorded =
                std::fs::read_to_string(directory.join("requests.jsonl")).unwrap_or_default();
            if let Some(value) = recorded
                .lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .find(&predicate)
            {
                return value;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
}

async fn plain_analysis(h: &mut Harness) {
    within(h.app.set_pondering(true)).await.unwrap();
    within(async {
        loop {
            if let AppEvent::AnalysisUpdated(view) = h.events.recv().await.unwrap() {
                assert!(view.visits > 0);
                break;
            }
        }
    })
    .await;
    let request = query(h.directory.path(), |r| r["maxVisits"] == 20).await;
    assert!(request["overrideSettings"].get("humanSLProfile").is_none());
}

#[tokio::test(start_paused = true)]
async fn public_actor_start_move_reply_and_stop_survive_the_full_command_loop() {
    // External process setup uses real time; only the move-delivery clock is advanced.
    tokio::time::resume();
    let mut h = harness(true).await;
    let view = within(h.app.start_coach(setup())).await.unwrap();
    assert_eq!(view.human_color, ColorView::Black);
    assert!(
        within(h.app.settings())
            .await
            .unwrap()
            .engine
            .unwrap()
            .human_model
            .is_some()
    );
    within(h.app.play(Move::Play(Color::Black, Point { x: 0, y: 0 })))
        .await
        .unwrap();
    let request = query(h.directory.path(), |r| {
        r["overrideSettings"]["humanSLProfile"] == "rank_20k"
            && r["moves"].as_array().is_some_and(|moves| moves.len() == 1)
    })
    .await;
    assert_eq!(request["includePolicy"], true);
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(2)).await;
    tokio::time::resume();
    // External engine progress retains a real five-second safety deadline.
    let played = within(async {
        loop {
            let game = h.app.game_view().await.unwrap().unwrap();
            assert_eq!(game.phase, GamePhase::Playing, "{game:?}");
            assert!(game.error.is_none());
            if game.move_number == 2 {
                break game;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert_eq!(played.to_play, ColorView::Black);
    assert!(!played.ai_thinking);
    let stopped = within(h.app.stop_game()).await.unwrap();
    assert_eq!(stopped.phase, GamePhase::Finished);
    assert_eq!(stopped.end_reason, Some(GameEndReason::Stopped));
    let summary = within(h.app.coach_view())
        .await
        .unwrap()
        .unwrap()
        .summary
        .unwrap();
    assert_eq!(summary.total_moves, 2);
    assert_eq!(summary.end_reason, Some(GameEndReason::Stopped));
    assert!(within(h.app.coach_view()).await.unwrap().unwrap().active);
    within(h.app.coach_review()).await.unwrap();
    assert!(!within(h.app.coach_view()).await.unwrap().unwrap().active);
    plain_analysis(&mut h).await;
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn public_actor_missing_human_model_is_typed_and_main_analysis_stays_available() {
    let mut h = harness(false).await;
    assert_eq!(
        within(h.app.humansl_model_status()).await.unwrap(),
        HumanSlModelStatus::Missing
    );
    assert!(matches!(
        within(h.app.start_coach(setup())).await,
        Err(AppError::Coach(CoachError::ModelMissing))
    ));
    assert!(within(h.app.coach_view()).await.unwrap().is_none());
    assert!(within(h.app.game_view()).await.unwrap().is_none());
    plain_analysis(&mut h).await;
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn public_actor_human_model_load_failure_restores_saved_engine_and_plain_analysis() {
    let mut h = harness_with_args(true, &["--fail-human-model"]).await;
    let original = within(h.app.settings()).await.unwrap();
    assert!(original.engine.as_ref().unwrap().human_model.is_none());

    let error = within(h.app.start_coach(setup())).await.unwrap_err();
    assert!(
        matches!(
            error,
            AppError::Coach(CoachError::HumanModelLoadFailed { ref reason })
                if reason.contains("injected human model load failure")
        ),
        "{error:?}"
    );

    // The initial Ready was consumed by the harness. This Ready must come from
    // restoring the original engine after the failed HumanSL startup.
    within(async {
        loop {
            if matches!(
                h.events.recv().await.unwrap(),
                AppEvent::EngineStatusChanged(EngineStatus::Ready { .. })
            ) {
                break;
            }
        }
    })
    .await;
    let restored = within(h.app.settings()).await.unwrap();
    assert_eq!(restored.engine, original.engine);
    assert_eq!(restored.engines, original.engines);
    assert_eq!(restored.main_engine_id, original.main_engine_id);
    assert!(restored.engine.as_ref().unwrap().human_model.is_none());
    assert!(within(h.app.coach_view()).await.unwrap().is_none());
    assert!(within(h.app.game_view()).await.unwrap().is_none());

    let reloaded = within(Settings::load(h.directory.path())).await.unwrap();
    assert_eq!(reloaded.engine, original.engine);
    assert_eq!(reloaded.engines, original.engines);
    assert_eq!(reloaded.main_engine_id, original.main_engine_id);
    assert!(reloaded.engine.as_ref().unwrap().human_model.is_none());
    plain_analysis(&mut h).await;
    within(h.app.shutdown()).await.unwrap();
}
