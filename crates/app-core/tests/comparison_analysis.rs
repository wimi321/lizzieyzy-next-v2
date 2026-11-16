#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
use app_core::{
    AiPlayer, AnalysisSource, AnalysisView, AppConfig, AppCore, AppEvent, AppSubscription,
    EngineEntry, EngineProfile, EngineRules, EngineStatus, GameSetup, GameStart, KataGoPlay,
    LayoutMode, Player, PointView, RestrictionKind, SearchParams, Settings,
};
use common::within;
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};

fn binary() -> PathBuf {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY
        .get_or_init(|| fake_katago::binary_path().unwrap())
        .clone()
}

fn profile(root: &Path, name: &str, winrate: f64) -> EngineProfile {
    let dir = root.join(name);
    std::fs::create_dir(&dir).unwrap();
    let script = dir.join("play.json");
    std::fs::write(
        &script,
        json!({"default": {"winrate": winrate, "scoreLead": 10.0 * winrate}}).to_string(),
    )
    .unwrap();
    let program = binary();
    EngineProfile {
        program: program.clone(),
        model: program.clone(),
        config: program,
        extra_args: vec![
            "--request-log".into(),
            dir.join("requests.jsonl").display().to_string(),
            "--play-script".into(),
            script.display().to_string(),
            "--visits-per-sec".into(),
            "2000".into(),
        ],
        ..Default::default()
    }
}

async fn setup(visits: u64) -> (tempfile::TempDir, AppCore, AppSubscription, AppConfig) {
    let directory = tempfile::tempdir().unwrap();
    let main = profile(directory.path(), "main", 0.2);
    let comparison = profile(directory.path(), "comparison", 0.8);
    let mut settings = Settings {
        engine: Some(main.clone()),
        main_engine_id: Some("main".into()),
        comparison_engine_id: Some("comparison".into()),
        engines: vec![
            EngineEntry {
                id: "main".into(),
                name: "Main".into(),
                profile: main,
            },
            EngineEntry {
                id: "comparison".into(),
                name: "Comparison".into(),
                profile: comparison,
            },
        ],
        auto_quick_analyze_on_load: false,
        pondering: false,
        ..Settings::default()
    };
    settings.display.layout_mode = LayoutMode::DoubleEngine;
    within(settings.save(directory.path())).await.unwrap();
    let mut config = AppConfig::default();
    config.analysis.live.max_visits = visits;
    config.analysis.live.report_interval = Duration::from_millis(50);
    config.session.shutdown_timeout = Duration::from_millis(100);
    config.session.response_timeout = Duration::from_secs(60);
    let (app, output) = within(app_core::spawn_with_config(
        directory.path(),
        config.clone(),
    ))
    .await
    .unwrap();
    within(async {
        loop {
            if matches!(
                app.comparison_state().await.unwrap().status,
                EngineStatus::Ready { .. }
            ) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    (directory, app, output, config)
}

fn requests(directory: &Path, side: &str) -> Vec<Value> {
    std::fs::read_to_string(directory.join(side).join("requests.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

async fn query(directory: &Path, side: &str, predicate: impl Fn(&Value) -> bool) -> Value {
    within(async {
        loop {
            if let Some(query) = requests(directory, side).into_iter().rev().find(&predicate) {
                return query;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
}

async fn equal_queries(directory: &Path, predicate: impl Fn(&Value) -> bool) -> [Value; 2] {
    let mut main = query(directory, "main", &predicate).await;
    let mut comparison = query(directory, "comparison", predicate).await;
    let original = [main.clone(), comparison.clone()];
    main.as_object_mut().unwrap().remove("id");
    comparison.as_object_mut().unwrap().remove("id");
    assert_eq!(main, comparison);
    original
}

async fn both_results(
    events: &mut AppSubscription,
    revision: u64,
    source: AnalysisSource,
) -> [AnalysisView; 2] {
    within(async {
        let mut values = [None, None];
        loop {
            let (side, value) = match events.recv().await.unwrap() {
                AppEvent::AnalysisUpdated(value) => (0, value),
                AppEvent::ComparisonAnalysisUpdated(value) => (1, value),
                _ => continue,
            };
            if value.revision == revision && value.source == source && !value.is_during_search {
                values[side] = Some(value);
            }
            if let [Some(a), Some(b)] = &values {
                return [a.clone(), b.clone()];
            }
        }
    })
    .await
}

#[tokio::test]
async fn two_sessions_follow_navigation_pause_limits_rules_restrictions_and_search_settings() {
    let (directory, app, _events, _) = setup(100_000).await;
    within(app.open_bytes(b"(;SZ[9]KM[6.5]RU[japanese];B[aa];W[bb])".to_vec()))
        .await
        .unwrap();
    within(app.set_pondering(true)).await.unwrap();
    equal_queries(directory.path(), |q| {
        q["moves"] == json!([]) && q["boardXSize"] == 9
    })
    .await;
    within(app.step(1)).await.unwrap();
    let first = equal_queries(directory.path(), |q| q["moves"] == json!([["B", "A9"]])).await;
    assert_eq!(first[0]["rules"], "japanese");
    assert_eq!(first[0]["komi"], 6.5);
    within(app.set_pondering(false)).await.unwrap();
    within(app.set_pondering(true)).await.unwrap();
    let resumed = equal_queries(directory.path(), |q| {
        q["moves"] == json!([["B", "A9"]]) && q["id"] != first[0]["id"] && q["id"] != first[1]["id"]
    })
    .await;
    for (index, side) in ["main", "comparison"].iter().enumerate() {
        // The resumed query is the barrier after terminate was delivered.
        assert!(
            requests(directory.path(), side)
                .iter()
                .any(|q| q["terminateId"] == first[index]["id"])
        );
    }
    let mut engine = within(app.settings()).await.unwrap().engine_settings;
    engine.rules = EngineRules::Chinese;
    engine.read_komi = false;
    engine.default_komi = 3.5;
    engine.report_interval_ms = 80;
    within(app.set_engine_settings(engine)).await.unwrap();
    equal_queries(directory.path(), |q| {
        q["komi"] == 3.5 && q["rules"] == "chinese"
    })
    .await;
    within(app.set_search_params(Some(SearchParams {
        playout_doubling_advantage: Some(0.5),
        wide_root_noise: Some(0.2),
        root_policy_temperature: Some(1.2),
    })))
    .await
    .unwrap();
    equal_queries(directory.path(), |q| {
        q["overrideSettings"]["wideRootNoise"] == 0.2
    })
    .await;
    within(app.set_analysis_restriction(RestrictionKind::Allow, vec![PointView { x: 3, y: 3 }]))
        .await
        .unwrap();
    let restricted = equal_queries(directory.path(), |q| q["allowMoves"].is_array()).await;
    assert_eq!(restricted[0]["allowMoves"][0]["moves"], json!(["D6"]));
    within(app.clear_analysis_restriction()).await.unwrap();
    let unbound = equal_queries(directory.path(), |q| {
        q["moves"].as_array().is_some_and(|moves| moves.len() == 1)
            && q["overrideSettings"]["wideRootNoise"] == 0.2
            && q.get("allowMoves").is_none()
            && q["id"] != resumed[0]["id"]
            && q["id"] != resumed[1]["id"]
    })
    .await;
    let mut limits = within(app.analysis_limits()).await.unwrap();
    limits.limit_playout = true;
    limits.limit_playouts = 1;
    within(app.set_analysis_limits(limits)).await.unwrap();
    for side in ["main", "comparison"] {
        query(directory.path(), side, |q| {
            q["action"] == "terminate" && q["terminateId"].as_str().is_some()
        })
        .await;
    }
    // Advancing to a new move resumes both automatic limits with the same cap.
    within(app.step(1)).await.unwrap();
    let next = equal_queries(directory.path(), |q| {
        q["moves"].as_array().is_some_and(|moves| moves.len() == 2)
    })
    .await;
    for (index, side) in ["main", "comparison"].iter().enumerate() {
        query(directory.path(), side, |q| {
            q["terminateId"] == next[index]["id"]
        })
        .await;
        assert!(
            requests(directory.path(), side)
                .iter()
                .any(|q| q["terminateId"] == unbound[index]["id"])
        );
    }
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn primary_game_query_terminates_both_live_focuses_without_reaching_comparison() {
    const LIVE_VISITS: u64 = 100_000;
    const GAME_VISITS: u64 = 1_000_000;
    let (directory, app, _events, _) = setup(LIVE_VISITS).await;
    within(app.set_pondering(true)).await.unwrap();
    let live = equal_queries(directory.path(), |q| q["maxVisits"] == LIVE_VISITS).await;
    within(app.start_game(GameSetup {
        black: Player::Ai(AiPlayer::KataGo(KataGoPlay {
            seconds: None,
            visits: Some(GAME_VISITS),
            pure_network: false,
            resign: None,
            random_opening: None,
        })),
        white: Player::Human,
        start: GameStart::CurrentPosition,
        human_clock: None,
        ponder: false,
        auto_save: false,
    }))
    .await
    .unwrap();
    let game = query(directory.path(), "main", |q| q["maxVisits"] == GAME_VISITS).await;
    assert_eq!(game["moves"], live[0]["moves"]);
    assert_ne!(game["id"], live[0]["id"]);
    for (index, side) in ["main", "comparison"].iter().enumerate() {
        query(directory.path(), side, |q| {
            q["action"] == "terminate" && q["terminateId"] == live[index]["id"]
        })
        .await;
    }
    // Observe both the main play request and the comparison termination before
    // checking absence; command acknowledgement alone does not flush log writes.
    assert!(
        requests(directory.path(), "comparison")
            .iter()
            .all(|q| q["maxVisits"] != GAME_VISITS)
    );
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn live_and_persistent_results_are_isolated_and_comparison_survives_main_close() {
    let (directory, app, mut events, config) = setup(20).await;
    within(app.set_pondering(true)).await.unwrap();
    let views = both_results(&mut events, 0, AnalysisSource::Live).await;
    assert_eq!((views[0].winrate, views[1].winrate), (0.2, 0.8));
    assert_eq!(
        within(app.whole_game_analysis(0))
            .await
            .unwrap()
            .unwrap()
            .response
            .root_info
            .winrate,
        0.2
    );
    assert_eq!(
        within(app.comparison_state())
            .await
            .unwrap()
            .analysis
            .unwrap()
            .winrate,
        0.8
    );
    within(app.set_pondering(false)).await.unwrap();
    within(app.shutdown()).await.unwrap();

    let (app, mut events) = within(app_core::spawn_with_config(directory.path(), config))
        .await
        .unwrap();
    let cached = both_results(&mut events, 0, AnalysisSource::Cache).await;
    assert_eq!((cached[0].winrate, cached[1].winrate), (0.2, 0.8));
    assert_eq!((cached[0].visits, cached[1].visits), (20, 20));
    within(app.close_engine()).await.unwrap();
    within(app.set_pondering(true)).await.unwrap();
    let revision = within(app.play(go_core::Move::Play(
        go_core::Color::Black,
        go_core::Point { x: 3, y: 3 },
    )))
    .await
    .unwrap();
    within(async {
        loop {
            if let AppEvent::ComparisonAnalysisUpdated(view) = events.recv().await.unwrap()
                && view.revision == revision
                && !view.is_during_search
            {
                assert_eq!(view.winrate, 0.8);
                break;
            }
        }
    })
    .await;
    assert!(within(app.whole_game_analysis(1)).await.unwrap().is_none());
    let mut failed = within(app.settings()).await.unwrap().engines[0]
        .profile
        .clone();
    failed.program = directory.path().join("missing-main-engine");
    within(app.configure_engine(Some(failed))).await.unwrap();
    let revision = within(app.play(go_core::Move::Play(
        go_core::Color::White,
        go_core::Point { x: 4, y: 4 },
    )))
    .await
    .unwrap();
    within(async {
        loop {
            if let AppEvent::ComparisonAnalysisUpdated(view) = events.recv().await.unwrap()
                && view.revision == revision
                && !view.is_during_search
            {
                assert_eq!(view.winrate, 0.8);
                break;
            }
        }
    })
    .await;
    assert!(within(app.whole_game_analysis(2)).await.unwrap().is_none());
    within(app.shutdown()).await.unwrap();
}
