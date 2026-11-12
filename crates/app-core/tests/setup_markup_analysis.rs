#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{AnalysisView, AppConfig, AppEvent, AppSubscription, ColorView, EngineProfile};
use go_core::Point;
use std::{future::Future, time::Duration};

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("setup analysis safety deadline")
}

async fn final_analysis(events: &mut AppSubscription, revision: u64) -> AnalysisView {
    within(async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::AnalysisUpdated(view)
                    if view.revision == revision && !view.is_during_search =>
                {
                    return view;
                }
                AppEvent::EngineStatusChanged(app_core::EngineStatus::Failed {
                    reason, ..
                }) => panic!("engine failed: {reason}"),
                _ => {}
            }
        }
    })
    .await
}

#[tokio::test]
async fn fake_engine_requests_follow_turn_changes_and_undo_redo_but_not_markup() {
    let directory = tempfile::tempdir().unwrap();
    let program = fake_katago::binary_path().unwrap();
    let model = directory.path().join("model.bin");
    let config_file = directory.path().join("analysis.cfg");
    let log = directory.path().join("requests.jsonl");
    std::fs::write(&model, "fake").unwrap();
    std::fs::write(&config_file, "").unwrap();
    let engine = EngineProfile {
        program,
        model,
        config: config_file,
        extra_args: vec![
            "--visits-per-sec".into(),
            "100000".into(),
            "--request-log".into(),
            log.to_str().unwrap().into(),
        ],
        canonical_model_name: None,
    };
    let mut config = AppConfig::default();
    config.analysis.live.max_visits = 1;
    config.session.startup_timeout = Duration::from_secs(60);
    config.session.response_timeout = Duration::from_secs(60);
    config.session.write_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(60);
    let (app, mut events) = within(app_core::spawn_with_config(directory.path(), config))
        .await
        .unwrap();
    within(app.set_auto_quick_analyze_on_load(false))
        .await
        .unwrap();
    within(app.open_bytes(b"(;SZ[9];B[aa])".to_vec()))
        .await
        .unwrap();
    let revision = within(app.step(1)).await.unwrap();
    within(app.configure_engine(Some(engine))).await.unwrap();
    let before = final_analysis(&mut events, revision).await;
    assert!(
        before
            .candidates
            .iter()
            .flat_map(|c| c.pv.first())
            .all(|mv| match mv {
                app_core::MoveView::Play { color, .. } | app_core::MoveView::Pass { color } =>
                    *color == ColorView::White,
            })
    );
    let old = within(app.whole_game_analysis(before.node_id))
        .await
        .unwrap()
        .unwrap();
    let revision = within(app.toggle_to_play()).await.unwrap();
    let toggled = final_analysis(&mut events, revision).await;
    let changed = within(app.whole_game_analysis(toggled.node_id))
        .await
        .unwrap()
        .unwrap();
    assert_ne!(old.key, changed.key);
    assert!(
        toggled
            .candidates
            .iter()
            .flat_map(|c| c.pv.first())
            .all(|mv| match mv {
                app_core::MoveView::Play { color, .. } | app_core::MoveView::Pass { color } =>
                    *color == ColorView::Black,
            })
    );
    let revision = within(app.undo()).await.unwrap();
    let undone = final_analysis(&mut events, revision).await;
    assert_eq!(undone.node_id, before.node_id);
    let revision = within(app.redo()).await.unwrap();
    let redone = final_analysis(&mut events, revision).await;
    assert_eq!(redone.node_id, toggled.node_id);
    let revision = within(app.set_markup(Point { x: 3, y: 3 }, document::MarkupKind::Triangle))
        .await
        .unwrap();
    final_analysis(&mut events, revision).await;
    let revision = within(app.clear_markup()).await.unwrap();
    final_analysis(&mut events, revision).await;
    let revision = within(app.go_to(0)).await.unwrap();
    final_analysis(&mut events, revision).await;
    let revision = within(app.toggle_to_play()).await.unwrap();
    final_analysis(&mut events, revision).await;
    within(app.shutdown()).await.unwrap();
    let queries: Vec<serde_json::Value> = std::fs::read_to_string(log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|q: &serde_json::Value| q.get("moves").is_some())
        .collect();
    assert_eq!(queries.len(), 6, "markup changes must not restart analysis");
    for index in [0, 2] {
        assert_eq!(queries[index]["initialPlayer"], "B");
        assert_eq!(queries[index]["moves"], serde_json::json!([["B", "A9"]]));
    }
    for index in [1, 3] {
        assert_eq!(queries[index]["initialPlayer"], "B");
        assert_eq!(
            queries[index]["initialStones"],
            serde_json::json!([["B", "A9"]])
        );
        assert_eq!(queries[index]["moves"], serde_json::json!([]));
    }
    assert_eq!(queries[4]["initialPlayer"], "B");
    assert_eq!(queries[5]["initialPlayer"], "W");
    assert_eq!(queries[4]["moves"], serde_json::json!([]));
    assert_eq!(queries[5]["moves"], serde_json::json!([]));
}

#[tokio::test]
async fn setup_and_first_player_invalidate_stale_analysis_and_conversion_starts_new_namespace() {
    let directory = tempfile::tempdir().unwrap();
    let model = directory.path().join("model.bin");
    let config_file = directory.path().join("analysis.cfg");
    std::fs::write(&model, "fake").unwrap();
    std::fs::write(&config_file, "").unwrap();
    let engine = EngineProfile {
        program: fake_katago::binary_path().unwrap(),
        model,
        config: config_file,
        extra_args: vec!["--visits-per-sec".into(), "100000".into()],
        canonical_model_name: None,
    };
    let mut config = AppConfig::default();
    config.analysis.live.max_visits = 1;
    let (app, mut events) = within(app_core::spawn_with_config(directory.path(), config))
        .await
        .unwrap();
    let revision = within(app.open_bytes(b"(;SZ[9])".to_vec())).await.unwrap();
    within(app.configure_engine(Some(engine))).await.unwrap();
    let initial = final_analysis(&mut events, revision).await;
    let old_key = within(app.whole_game_analysis(0))
        .await
        .unwrap()
        .unwrap()
        .key;
    within(app.set_pondering(false)).await.unwrap();
    let revision = within(app.set_first_player(go_core::Color::White))
        .await
        .unwrap();
    assert!(within(app.whole_game_result(0)).await.unwrap().is_none());
    let revision_undo = within(app.undo()).await.unwrap();
    let restored = final_analysis(&mut events, revision_undo).await;
    assert_eq!(restored.summary, initial.summary);
    let redone = within(app.redo()).await.unwrap();
    assert!(redone > revision);
    within(app.set_pondering(true)).await.unwrap();
    final_analysis(&mut events, redone).await;
    assert_ne!(
        within(app.whole_game_analysis(0))
            .await
            .unwrap()
            .unwrap()
            .key,
        old_key
    );
    within(app.set_pondering(false)).await.unwrap();
    let revision = within(app.set_setup_stone(Point { x: 1, y: 1 }, Some(go_core::Color::Black)))
        .await
        .unwrap();
    assert!(within(app.whole_game_result(0)).await.unwrap().is_none());
    within(app.set_pondering(true)).await.unwrap();
    final_analysis(&mut events, revision).await;
    let revision = within(app.convert_to_start_position()).await.unwrap();
    let converted = final_analysis(&mut events, revision).await;
    assert_eq!(converted.node_id, 0);
    assert!(within(app.undo()).await.is_err());
    within(app.shutdown()).await.unwrap();
}
