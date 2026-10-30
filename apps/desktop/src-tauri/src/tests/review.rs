use super::{Harness, json, local_url, profile, request, within};
use app_core::{AppEvent, WholeGameProgress, WholeGameStatus};

#[tokio::test]
async fn whole_game_commands_forward_controls_results_and_replay_progress() {
    let mut harness = Harness::new().await;
    let mut engine = profile(harness.directory.path());
    engine.extra_args = vec!["--visits-per-sec".into(), "100000".into()];
    harness.subscribe(81).await;
    harness.document(81, 0).await;
    assert!(
        harness
            .invoke("whole_game_progress", json!({}))
            .await
            .is_null()
    );
    harness
        .invoke("open_bytes", json!({ "bytes": b"(;SZ[9];B[aa];W[bb])" }))
        .await;
    let document = harness.document(81, 1).await;
    harness
        .invoke("configure_engine", json!({ "profile": engine }))
        .await;
    let started = harness
        .invoke(
            "start_whole_game_analysis",
            json!({ "options": { "visits": 1_000_000 } }),
        )
        .await;
    assert_eq!(started["total"], 3);
    assert_eq!(started["visits"], 1_000_000);
    assert_eq!(started["status"]["type"], "Running");
    let paused = harness.invoke("pause_whole_game_analysis", json!({})).await;
    assert_eq!(paused["status"]["type"], "Paused");
    assert_eq!(
        harness.invoke("whole_game_progress", json!({})).await,
        paused
    );
    let resumed = harness
        .invoke("resume_whole_game_analysis", json!({}))
        .await;
    assert_eq!(resumed["status"]["type"], "Running");
    let cancelled = harness
        .invoke("cancel_whole_game_analysis", json!({}))
        .await;
    assert_eq!(cancelled["status"]["type"], "Cancelled");
    assert_eq!(
        harness.invoke("whole_game_progress", json!({})).await,
        cancelled
    );

    let invalid = request(
        &harness.main,
        "start_whole_game_analysis",
        json!({ "options": { "visits": 499 } }),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(invalid["type"], "App");
    assert_eq!(
        harness.invoke("whole_game_progress", json!({})).await,
        cancelled
    );

    harness
        .invoke(
            "start_whole_game_analysis",
            json!({ "options": { "visits": 500 } }),
        )
        .await;
    let completed = within(async {
        loop {
            if let AppEvent::WholeGameProgress(progress) = harness.next().await.event {
                match &progress.status {
                    WholeGameStatus::Completed => break progress,
                    WholeGameStatus::Error { reason } => panic!("review failed: {reason}"),
                    _ => {}
                }
            }
        }
    })
    .await;
    assert_eq!((completed.completed, completed.total), (3, 3));
    for node_id in document.tree.main_line {
        let result = harness
            .invoke("whole_game_result", json!({ "nodeId": node_id }))
            .await;
        assert_eq!(result["node_id"], node_id);
        assert!(result["visits"].as_u64().unwrap() >= 500);
        assert!(!result["candidates"].as_array().unwrap().is_empty());
        assert!(!result["candidates"][0]["pv"].as_array().unwrap().is_empty());
    }

    // A replacement Channel receives the cached terminal progress together
    // with document and engine state, without starting another analysis task.
    harness.subscribe(82).await;
    let replayed: WholeGameProgress = within(async {
        loop {
            let message = harness.next().await;
            if message.callback == 82
                && let AppEvent::WholeGameProgress(progress) = message.event
            {
                break progress;
            }
        }
    })
    .await;
    assert_eq!(replayed, completed);
    harness.finish().await;
}

#[tokio::test]
async fn review_commands_are_denied_to_other_windows_and_remote_origins() {
    let harness = Harness::new().await;
    let other = tauri::WebviewWindowBuilder::new(&harness.app, "review-other", Default::default())
        .build()
        .unwrap();
    for command in [
        "start_whole_game_analysis",
        "pause_whole_game_analysis",
        "resume_whole_game_analysis",
        "cancel_whole_game_analysis",
        "whole_game_progress",
        "whole_game_result",
    ] {
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
