#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
#[path = "common/game.rs"]
mod support;
use app_core::{
    AnalysisSource, AnalysisToolsError, AnalysisView, AppError, AppEvent, AppSubscription,
    BatchOptions, BatchOutput, BatchPhase, GameSetup, KataGoPlay, MatchPlayer, MatchSetup, Player,
    PointView, RestrictionKind, SearchParams, WholeGameOptions, WholeGameStatus,
};
use common::within;
use serde_json::{Value, json};
use support::{query, requests_before};

fn params() -> SearchParams {
    SearchParams {
        playout_doubling_advantage: Some(0.3),
        wide_root_noise: Some(0.4),
        root_policy_temperature: Some(1.2),
    }
}
fn check(request: &Value) {
    assert_eq!(request["overrideSettings"]["playoutDoublingAdvantage"], 0.3);
    assert_eq!(request["overrideSettings"]["wideRootNoise"], 0.4);
    assert_eq!(request["overrideSettings"]["rootPolicyTemperature"], 1.2);
}
async fn normal(events: &mut AppSubscription) -> AnalysisView {
    within(async {
        loop {
            if let AppEvent::AnalysisUpdated(v) = events.recv().await.unwrap()
                && !v.is_during_search
            {
                return v;
            }
        }
    })
    .await
}

#[tokio::test]
async fn search_parameter_bounds_and_defaults_persist_atomically() {
    let directory = tempfile::tempdir().unwrap();
    let (app, _) = within(app_core::spawn(directory.path())).await.unwrap();
    assert_eq!(within(app.search_params()).await.unwrap(), None);
    assert_eq!(
        within(app.set_search_params(Some(params()))).await.unwrap(),
        Some(params())
    );
    for invalid in [
        SearchParams {
            playout_doubling_advantage: Some(-3.01),
            ..Default::default()
        },
        SearchParams {
            playout_doubling_advantage: Some(3.01),
            ..Default::default()
        },
        SearchParams {
            wide_root_noise: Some(-0.01),
            ..Default::default()
        },
        SearchParams {
            wide_root_noise: Some(5.01),
            ..Default::default()
        },
        SearchParams {
            root_policy_temperature: Some(0.009),
            ..Default::default()
        },
        SearchParams {
            root_policy_temperature: Some(100.01),
            ..Default::default()
        },
        SearchParams {
            root_policy_temperature: Some(f64::NAN),
            ..Default::default()
        },
        SearchParams {
            wide_root_noise: Some(f64::INFINITY),
            ..Default::default()
        },
    ] {
        assert!(matches!(
            within(app.set_search_params(Some(invalid))).await,
            Err(AppError::AnalysisTools(
                AnalysisToolsError::InvalidSearchParam(_)
            ))
        ));
        assert_eq!(within(app.search_params()).await.unwrap(), Some(params()));
    }
    assert_eq!(
        within(app_core::Settings::load(directory.path()))
            .await
            .unwrap()
            .search_params,
        Some(params())
    );
    for allowed in [
        SearchParams {
            playout_doubling_advantage: Some(-3.0),
            wide_root_noise: Some(0.0),
            root_policy_temperature: Some(0.01),
        },
        SearchParams {
            playout_doubling_advantage: Some(3.0),
            wide_root_noise: Some(5.0),
            root_policy_temperature: Some(100.0),
        },
    ] {
        assert_eq!(
            within(app.set_search_params(Some(allowed.clone())))
                .await
                .unwrap(),
            Some(allowed)
        );
    }
    assert_eq!(within(app.set_search_params(None)).await.unwrap(), None);
    assert_eq!(
        within(app.set_search_params(Some(SearchParams::default())))
            .await
            .unwrap(),
        None
    );
    within(app.shutdown()).await.unwrap();
    assert_eq!(
        within(app_core::Settings::load(directory.path()))
            .await
            .unwrap()
            .search_params,
        None
    );
}

#[tokio::test]
async fn live_whole_batch_and_game_queries_use_search_settings_without_restarting_engine() {
    let directory = tempfile::tempdir().unwrap();
    let launch = directory.path().join("launch.jsonl");
    let mut cfg = support::config(directory.path());
    cfg.analysis.live.max_visits = 11;
    let mut h = support::harness_in(
        directory,
        cfg,
        vec!["--launch-log".into(), launch.to_string_lossy().into()],
    )
    .await;
    within(h.app.set_search_params(Some(params())))
        .await
        .unwrap();
    within(h.app.open_bytes(b"(;SZ[9];B[aa];W[bb])".to_vec()))
        .await
        .unwrap();
    within(h.app.set_pondering(true)).await.unwrap();
    check(&query(h.directory.path(), |q| q["maxVisits"] == 11).await);
    normal(&mut h.events).await;
    within(
        h.app
            .set_analysis_restriction(RestrictionKind::Allow, vec![PointView { x: 3, y: 3 }]),
    )
    .await
    .unwrap();
    check(&query(h.directory.path(), |q| q.get("allowMoves").is_some()).await);
    within(h.app.start_whole_game_analysis(WholeGameOptions {
        visits: 13,
        force: true,
        ..Default::default()
    }))
    .await
    .unwrap();
    let review = query(h.directory.path(), |q| {
        q["priority"] == -100 && q["maxVisits"] == 13
    })
    .await;
    check(&review);
    assert!(review.get("allowMoves").is_none());
    assert!(review.get("avoidMoves").is_none());
    within(async {
        loop {
            let p = h.app.whole_game_progress().await.unwrap().unwrap();
            if p.status == WholeGameStatus::Completed {
                break;
            }
            assert!(!matches!(p.status, WholeGameStatus::Error { .. }), "{p:?}");
            tokio::task::yield_now().await;
        }
    })
    .await;
    within(h.app.set_pondering(false)).await.unwrap();
    let input = h.directory.path().join("batch.sgf");
    std::fs::write(&input, b"(;SZ[9];B[cc];W[dd])").unwrap();
    within(h.app.start_batch_analysis(BatchOptions {
        files: vec![input],
        visits: 17,
        scope: Default::default(),
        force: true,
        output: BatchOutput {
            suffix: "review".into(),
            directory: None,
        },
    }))
    .await
    .unwrap();
    let batch = query(h.directory.path(), |q| {
        q["priority"] == -100 && q["maxVisits"] == 17
    })
    .await;
    check(&batch);
    assert!(batch.get("allowMoves").is_none());
    assert!(batch.get("avoidMoves").is_none());
    within(async {
        loop {
            let s = h.app.batch_analysis_status().await.unwrap();
            if s.phase == BatchPhase::Completed {
                break;
            }
            assert_ne!(s.phase, BatchPhase::Stopped, "{s:?}");
            tokio::task::yield_now().await;
        }
    })
    .await;
    let setup = GameSetup {
        black: Player::Ai(support::ai(19)),
        white: Player::Human,
        ..support::setup()
    };
    within(h.app.start_game(setup)).await.unwrap();
    let game = query(h.directory.path(), |q| q["maxVisits"] == 19).await;
    check(&game);
    assert!(game.get("allowMoves").is_none());
    assert!(game.get("avoidMoves").is_none());
    support::moves(&h.app, 1).await;
    within(h.app.stop_game()).await.unwrap();
    within(h.app.set_search_params(None)).await.unwrap();
    within(h.app.set_pondering(true)).await.unwrap();
    let marker = query(h.directory.path(), |q| {
        q.get("moves").is_some() && q["overrideSettings"].get("rootPolicyTemperature").is_none()
    })
    .await;
    let prefix = requests_before(h.directory.path(), |q| q["id"] == marker["id"]).await;
    for q in prefix.iter().filter(|q| q.get("moves").is_some()) {
        check(q);
    }
    assert_eq!(
        std::fs::read_to_string(launch).unwrap().lines().count(),
        1,
        "parameter changes reuse the engine process"
    );
    within(h.app.shutdown()).await.unwrap();
}

async fn match_query_with_side_params(mut side_params: Value) -> (support::Harness, Value) {
    let directory = tempfile::tempdir().unwrap();
    let script = directory.path().join("script.json");
    std::fs::write(&script, json!({"default":{"hang":true}}).to_string()).unwrap();
    let cfg = support::config(directory.path());
    let h = support::harness_in(
        directory,
        cfg,
        vec!["--play-script".into(), script.to_string_lossy().into()],
    )
    .await;
    within(h.app.set_search_params(Some(SearchParams {
        playout_doubling_advantage: Some(1.0),
        wide_root_noise: Some(0.5),
        root_policy_temperature: Some(1.2),
    })))
    .await
    .unwrap();
    let id = within(h.app.settings())
        .await
        .unwrap()
        .main_engine_id
        .unwrap();
    side_params["engine_id"] = json!(id);
    side_params["play"] = json!(KataGoPlay {
        visits: Some(103),
        seconds: None,
        resign: None,
        ..Default::default()
    });
    // Exercise omitted fields through the same JSON shape accepted by IPC.
    let player: MatchPlayer =
        serde_json::from_value(json!({"type": "KataGo", "data": side_params})).unwrap();
    within(h.app.start_match(MatchSetup {
        a: player.clone(),
        b: player,
        auto_save: false,
        ..Default::default()
    }))
    .await
    .unwrap();
    let request = query(h.directory.path(), |q| q["maxVisits"] == 103).await;
    assert_eq!(request["overrideSettings"]["rootPolicyTemperature"], 1.2);
    (h, request)
}

#[tokio::test]
async fn match_side_pda_overrides_global_and_unset_wrn_inherits() {
    let (h, request) = match_query_with_side_params(json!({"pda": 2.0})).await;
    assert_eq!(request["overrideSettings"]["playoutDoublingAdvantage"], 2.0);
    assert_eq!(request["overrideSettings"]["wideRootNoise"], 0.5);
    assert!(matches!(
        within(
            h.app
                .set_analysis_restriction(RestrictionKind::Avoid, vec![PointView { x: 0, y: 0 }])
        )
        .await,
        Err(AppError::AnalysisTools(AnalysisToolsError::GameActive))
    ));
    within(h.app.stop_match()).await.unwrap();
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn match_side_zero_wrn_overrides_global_and_unset_pda_inherits() {
    let (h, request) = match_query_with_side_params(json!({"wrn": 0.0})).await;
    assert_eq!(request["overrideSettings"]["wideRootNoise"], 0.0);
    assert_eq!(request["overrideSettings"]["playoutDoublingAdvantage"], 1.0);
    within(h.app.stop_match()).await.unwrap();
    within(h.app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn persistent_results_are_separated_by_search_parameters_and_default_cache_survives() {
    let directory = tempfile::tempdir().unwrap();
    let mut cfg = support::config(directory.path());
    cfg.analysis.live.max_visits = 31;
    let mut h = support::harness_in(directory, cfg, vec![]).await;
    within(h.app.set_cache_enabled(true)).await.unwrap();
    within(h.app.open_bytes(b"(;SZ[9])".to_vec()))
        .await
        .unwrap();
    within(h.app.set_pondering(true)).await.unwrap();
    let original = normal(&mut h.events).await;
    assert_eq!(original.visits, 31);
    within(h.app.set_pondering(false)).await.unwrap();
    within(h.app.set_search_params(Some(params())))
        .await
        .unwrap();
    assert!(
        within(h.app.whole_game_analysis(0))
            .await
            .unwrap()
            .is_none()
    );
    within(h.app.shutdown()).await.unwrap();
    let mut cfg = support::config(h.directory.path());
    cfg.analysis.live.max_visits = 1;
    let (app, mut events) = within(app_core::spawn_with_config(h.directory.path(), cfg))
        .await
        .unwrap();
    within(app.open_bytes(b"(;SZ[9])".to_vec())).await.unwrap();
    within(app.set_pondering(true)).await.unwrap();
    let changed = normal(&mut events).await;
    assert_eq!(changed.source, AnalysisSource::Live);
    assert_eq!(
        changed.visits, 1,
        "new search settings must miss the old 31-visit cache"
    );
    within(app.set_pondering(false)).await.unwrap();
    within(app.set_search_params(None)).await.unwrap();
    let restored = normal(&mut events).await;
    assert_eq!(restored.source, AnalysisSource::Cache);
    assert_eq!(restored.visits, 31);
    assert_eq!(restored.candidates, original.candidates);
    within(app.shutdown()).await.unwrap();
}
