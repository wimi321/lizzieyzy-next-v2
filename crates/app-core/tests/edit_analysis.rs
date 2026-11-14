#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{
    AnalysisView, AppConfig, AppCore, AppEvent, AppSubscription, DocumentView, EngineProfile,
    EngineStatus, GameInfoEdit, NewGameOptions, WholeGameStatus,
};
use std::{future::Future, path::Path, time::Duration};

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("edit analysis operation exceeded safety deadline")
}

fn profile(directory: &Path) -> EngineProfile {
    let program = fake_katago::binary_path().unwrap();
    let model = directory.join("model.bin");
    let config = directory.join("analysis.cfg");
    std::fs::write(&model, "fake model").unwrap();
    std::fs::write(&config, "").unwrap();
    EngineProfile {
        remote: None,
        human_model: None,
        program,
        model,
        config,
        extra_args: vec![
            "--visits-per-sec".into(),
            "100000".into(),
            "--request-log".into(),
            directory.join("requests.jsonl").to_str().unwrap().into(),
        ],
        canonical_model_name: None,
    }
}

async fn app(directory: &Path) -> (AppCore, AppSubscription) {
    let mut config = AppConfig::default();
    config.analysis.live.max_visits = 1;
    config.session.startup_timeout = Duration::from_secs(60);
    config.session.response_timeout = Duration::from_secs(60);
    config.session.write_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(60);
    within(app_core::spawn_with_config(directory, config))
        .await
        .unwrap()
}

async fn document(events: &mut AppSubscription, expected: u64) -> DocumentView {
    within(async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::DocumentChanged { revision, view } if revision == expected => {
                    return view;
                }
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("unexpected engine failure: {reason}");
                }
                _ => {}
            }
        }
    })
    .await
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
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("unexpected engine failure: {reason}");
                }
                _ => {}
            }
        }
    })
    .await
}

fn game_info(komi: f64) -> GameInfoEdit {
    GameInfoEdit {
        black_name: Some("Edited player".into()),
        black_rank: None,
        white_name: None,
        white_rank: None,
        komi,
        rules: "chinese".into(),
        result: None,
        date: None,
        event: None,
    }
}

#[tokio::test]
async fn game_info_changes_hide_stale_graph_analysis_and_undo_restores_matching_results() {
    let directory = tempfile::tempdir().unwrap();
    let engine = profile(directory.path());
    let (handle, mut events) = app(directory.path()).await;
    let initial = within(handle.open_bytes(b"(;SZ[9]KM[7.5]RU[chinese])".to_vec()))
        .await
        .unwrap();
    document(&mut events, initial).await;
    within(handle.configure_engine(Some(engine))).await.unwrap();
    let original = final_analysis(&mut events, initial).await;
    within(handle.set_pondering(false)).await.unwrap();

    let edited = within(handle.set_game_info(game_info(6.5))).await.unwrap();
    let view = document(&mut events, edited).await;
    assert!(view.modified);
    assert_eq!(view.game_info.komi, 6.5);
    assert!(
        view.graph
            .points
            .iter()
            .all(|point| point.summary.is_none())
    );
    assert!(within(handle.whole_game_result(0)).await.unwrap().is_none());

    let undone = within(handle.undo()).await.unwrap();
    let restored = document(&mut events, undone).await;
    assert!(!restored.modified);
    assert_eq!(restored.game_info.komi, 7.5);
    assert_eq!(restored.graph.points[0].summary, Some(original.summary));
    assert_eq!(
        within(handle.whole_game_result(0))
            .await
            .unwrap()
            .unwrap()
            .summary,
        original.summary,
    );

    let redone = within(handle.redo()).await.unwrap();
    let changed_again = document(&mut events, redone).await;
    assert!(changed_again.modified);
    assert!(changed_again.graph.points[0].summary.is_none());
    within(handle.set_pondering(true)).await.unwrap();
    let current = final_analysis(&mut events, redone).await;
    assert_eq!(current.visits, 1);
    assert_eq!(current.summary.visits, 1);
    let stored = within(handle.whole_game_analysis(0))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.response.root_info.score_lead, current.score_lead);
    within(handle.shutdown()).await.unwrap();
    let queries: Vec<serde_json::Value> =
        std::fs::read_to_string(directory.path().join("requests.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .filter(|query: &serde_json::Value| query.get("moves").is_some())
            .collect();
    assert_eq!(
        queries
            .iter()
            .map(|query| query["komi"].as_f64().unwrap())
            .collect::<Vec<_>>(),
        vec![7.5, 6.5]
    );
}

#[tokio::test]
async fn new_game_replaces_deferred_automatic_review_and_analyzes_only_new_handicap_root() {
    let directory = tempfile::tempdir().unwrap();
    let engine = profile(directory.path());
    let (handle, mut events) = app(directory.path()).await;
    within(handle.open_bytes(b"(;SZ[19];B[aa];W[bb];B[cc])".to_vec()))
        .await
        .unwrap();
    let revision = within(handle.new_game(NewGameOptions {
        width: 9,
        height: 9,
        komi: 0.5,
        handicap: 2,
        rules: "japanese".into(),
        black_name: None,
        black_rank: None,
        white_name: None,
        white_rank: None,
    }))
    .await
    .unwrap();
    let view = document(&mut events, revision).await;
    assert_eq!(view.file_path, None);
    assert!(!view.modified);
    within(handle.configure_engine(Some(engine))).await.unwrap();
    final_analysis(&mut events, revision).await;
    assert!(
        within(handle.whole_game_progress())
            .await
            .unwrap()
            .is_none_or(|progress| { progress.status == WholeGameStatus::Cancelled })
    );
    within(handle.shutdown()).await.unwrap();
    let queries: Vec<serde_json::Value> =
        std::fs::read_to_string(directory.path().join("requests.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .filter(|query: &serde_json::Value| query.get("moves").is_some())
            .collect();
    assert_eq!(queries.len(), 1);
    assert_eq!(queries[0]["boardXSize"], 9);
    assert_eq!(queries[0]["boardYSize"], 9);
    assert_eq!(queries[0]["komi"], 0.5);
    assert_eq!(queries[0]["rules"], "japanese");
    assert_eq!(queries[0]["initialPlayer"], "W");
    assert_eq!(queries[0]["initialStones"].as_array().unwrap().len(), 2);
    assert_eq!(queries[0]["moves"], serde_json::json!([]));
    assert_eq!(queries[0]["priority"], 0);
}
