use super::{Harness, json, local_url, profile, request, within};
use app_core::{
    AppEvent, ColorView, MoveRank, MoveRankEvaluationMode, MoveRankSettings, ProblemMoveSort,
    ProblemMovesQuery, ProblemMovesView, WholeGameStatus,
};

#[tokio::test]
async fn problem_move_ipc_forwards_views_settings_and_validation_without_navigation() {
    let mut harness = Harness::new().await;
    harness.subscribe(91).await;
    harness.document(91, 0).await;
    harness
        .invoke("set_pondering", json!({ "enabled": false }))
        .await;
    harness
        .invoke("open_bytes", json!({ "bytes": b"(;SZ[9];B[aa];W[bb])" }))
        .await;
    let initial = harness.document(91, 1).await;
    let empty: ProblemMovesView = serde_json::from_value(
        harness
            .invoke("problem_moves", json!({ "query": {} }))
            .await,
    )
    .unwrap();
    assert_eq!(empty.revision, 1);
    assert_eq!(empty.current_index, 0);
    assert!(empty.entries.is_empty());
    assert_eq!(empty.error_bar.len(), 3);
    assert!(empty.error_bar.iter().all(|point| point.loss.is_none()));

    let settings = MoveRankSettings {
        mode: MoveRankEvaluationMode::Combined,
        winrate_thresholds: [2.0, 4.0, 8.0, 16.0, 32.0],
        score_thresholds: [1.0, 2.0, 4.0, 8.0, 16.0],
    };
    assert_eq!(
        harness
            .invoke("set_move_rank_settings", json!({ "settings": settings }))
            .await,
        json!(settings)
    );
    let changed = harness.document(91, 1).await;
    assert_eq!(
        changed, initial,
        "classification settings must not edit the game"
    );
    assert_eq!(
        harness.invoke("settings", json!({})).await["move_rank"],
        json!(settings)
    );
    let mut invalid = settings.clone();
    invalid.score_thresholds[1] = 0.0;
    let error = request(
        &harness.main,
        "set_move_rank_settings",
        json!({ "settings": invalid }),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(error["type"], "App");
    assert_eq!(
        harness.invoke("settings", json!({})).await["move_rank"],
        json!(settings)
    );

    let mut engine = profile(harness.directory.path());
    engine.extra_args = vec!["--visits-per-sec".into(), "100000".into()];
    harness
        .invoke("configure_engine", json!({ "profile": engine }))
        .await;
    harness
        .invoke(
            "start_whole_game_analysis",
            json!({ "options": { "visits": 500 } }),
        )
        .await;
    within(async {
        loop {
            if let AppEvent::WholeGameProgress(progress) = harness.next().await.event {
                match progress.status {
                    WholeGameStatus::Completed => break,
                    WholeGameStatus::Error { reason } => panic!("review failed: {reason}"),
                    _ => {}
                }
            }
        }
    })
    .await;
    let query = ProblemMovesQuery {
        minimum_rank: MoveRank::Best,
        color: Some(ColorView::White),
        sort: ProblemMoveSort::MoveDesc,
    };
    let view: ProblemMovesView = serde_json::from_value(
        harness
            .invoke("problem_moves", json!({ "query": query }))
            .await,
    )
    .unwrap();
    assert_eq!(
        view,
        within(harness.core.problem_moves(query)).await.unwrap()
    );
    assert_eq!(view.settings, settings);
    assert_eq!(view.revision, empty.revision);
    assert_eq!(view.current_index, empty.current_index);
    assert!(view.error_bar[0].loss.is_none());
    assert!(view.error_bar[1..].iter().all(|point| point.loss.is_some()));
    assert!(
        view.entries
            .iter()
            .all(|entry| entry.color == ColorView::White)
    );
    assert_eq!(
        view.error_bar
            .iter()
            .map(|point| point.node_id)
            .collect::<Vec<_>>(),
        initial.tree.main_line
    );
    harness.finish().await;
}

#[tokio::test]
async fn problem_move_commands_are_denied_to_other_windows_and_remote_origins() {
    let harness = Harness::new().await;
    let other = tauri::WebviewWindowBuilder::new(&harness.app, "problem-other", Default::default())
        .build()
        .unwrap();
    for command in ["problem_moves", "set_move_rank_settings"] {
        for (window, url) in [
            (&other, local_url()),
            (&harness.main, "https://example.com/"),
        ] {
            let error = request(window, command, json!({}), url).await.unwrap_err();
            assert!(
                error
                    .as_str()
                    .is_some_and(|text| text.contains("not allowed")),
                "{command}: {error}"
            );
        }
    }
    harness.finish().await;
}
