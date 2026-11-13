use super::{Harness, json, local_url, profile, request, within};
use app_core::{AppEvent, BatchAnalysisStatus, FlashAnalysisSettings};

#[tokio::test]
async fn flash_settings_and_idle_queue_commands_round_trip_through_ipc() {
    let harness = Harness::new().await;
    assert_eq!(
        harness.invoke("flash_analysis", json!({})).await,
        json!(FlashAnalysisSettings::default())
    );
    let settings = json!({ "visits": 27, "always_override": true });
    assert_eq!(
        harness
            .invoke("set_flash_analysis", json!({ "settings": settings }))
            .await,
        settings
    );
    assert_eq!(harness.invoke("flash_analysis", json!({})).await, settings);
    assert_eq!(
        harness.invoke("settings", json!({})).await["flash_analysis"],
        settings
    );
    assert_eq!(
        json!(
            app_core::Settings::load(harness.directory.path())
                .await
                .unwrap()
                .flash_analysis
        ),
        settings
    );
    assert!(
        request(
            &harness.main,
            "set_flash_analysis",
            json!({ "settings": { "visits": 0 } }),
            local_url()
        )
        .await
        .is_err()
    );
    assert_eq!(harness.invoke("flash_analysis", json!({})).await, settings);
    let initial = harness.invoke("batch_analysis_status", json!({})).await;
    assert_eq!(initial["phase"], "Idle");
    assert_eq!(initial["items"], json!([]));
    let added = harness
        .invoke(
            "add_batch_analysis_files",
            json!({ "files": ["a.sgf", "b.gib", "c.sgf"] }),
        )
        .await;
    let ids: Vec<_> = added["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_u64().unwrap())
        .collect();
    let first = harness
        .invoke(
            "move_batch_analysis_item",
            json!({ "id": ids[2], "direction": "First" }),
        )
        .await;
    assert_eq!(first["items"][0]["path"], "c.sgf");
    let down = harness
        .invoke(
            "move_batch_analysis_item",
            json!({ "id": ids[2], "direction": "Down" }),
        )
        .await;
    assert_eq!(down["items"][1]["path"], "c.sgf");
    let up = harness
        .invoke(
            "move_batch_analysis_item",
            json!({ "id": ids[1], "direction": "Up" }),
        )
        .await;
    assert_eq!(up["items"][1]["path"], "b.gib");
    let removed = harness
        .invoke("remove_batch_analysis_item", json!({ "id": ids[1] }))
        .await;
    assert_eq!(removed["items"].as_array().unwrap().len(), 2);
    assert_eq!(
        harness.invoke("clear_batch_analysis", json!({})).await["items"],
        json!([])
    );
    harness.finish().await;
}

#[tokio::test]
async fn batch_status_is_replayed_through_the_existing_channel() {
    let mut harness = Harness::new().await;
    harness.subscribe(91).await;
    harness.document(91, 0).await;
    let expected: BatchAnalysisStatus = serde_json::from_value(
        harness
            .invoke("add_batch_analysis_files", json!({ "files": ["one.sgf"] }))
            .await,
    )
    .unwrap();
    within(async {
        loop {
            if let AppEvent::BatchAnalysisProgress(status) = harness.next().await.event
                && status == expected
            {
                break;
            }
        }
    })
    .await;
    harness.subscribe(92).await;
    within(async {
        loop {
            let message = harness.next().await;
            if message.callback == 92
                && let AppEvent::BatchAnalysisProgress(status) = message.event
                && status == expected
            {
                break;
            }
        }
    })
    .await;
    harness.finish().await;
}

#[tokio::test]
async fn batch_controls_forward_and_running_guard_stays_typed_over_ipc() {
    let harness = Harness::new().await;
    harness
        .core
        .set_auto_quick_analyze_on_load(false)
        .await
        .unwrap();
    harness.core.set_pondering(false).await.unwrap();
    let file = harness.directory.path().join("batch.sgf");
    std::fs::write(&file, "(;SZ[9];B[aa];W[bb])").unwrap();
    // The explicit startup gate prevents completion, independent of process speed.
    let gate = harness.directory.path().join("release-engine");
    let mut engine = profile(harness.directory.path());
    engine.extra_args = vec![
        "--startup-gate-file".into(),
        gate.to_string_lossy().into_owned(),
    ];
    harness
        .invoke("configure_engine", json!({ "profile": engine }))
        .await;
    let started = harness
        .invoke(
            "start_batch_analysis",
            json!({ "options": {
        "files": [file], "visits": 17, "scope": { "type": "AllBranches" }, "force": true,
        "output": { "suffix": "analyzed", "directory": null },
    } }),
        )
        .await;
    assert_eq!(started["phase"], "Running");
    assert_eq!(
        harness.invoke("pause_batch_analysis", json!({})).await["phase"],
        "Paused"
    );
    for (command, arguments) in [
        ("open_path", json!({ "path": file })),
        ("set_comment", json!({ "text": "edit" })),
        (
            "start_whole_game_analysis",
            json!({ "options": { "visits": 1 } }),
        ),
    ] {
        let error = request(&harness.main, command, arguments, local_url())
            .await
            .unwrap_err();
        assert_eq!(error["type"], "BatchAnalysisRunning", "{command}: {error}");
    }
    assert_eq!(
        harness.invoke("resume_batch_analysis", json!({})).await["phase"],
        "Running"
    );
    let stopped = harness.invoke("stop_batch_analysis", json!({})).await;
    assert_eq!(stopped["phase"], "Stopped");
    assert_eq!(stopped["items"][0]["state"]["type"], "Waiting");
    std::fs::write(gate, "ready").unwrap();
    harness.finish().await;
}

#[tokio::test]
async fn batch_and_flash_commands_are_local_main_only() {
    let harness = Harness::new().await;
    let other = tauri::WebviewWindowBuilder::new(&harness.app, "batch-other", Default::default())
        .build()
        .unwrap();
    for command in [
        "start_batch_analysis",
        "pause_batch_analysis",
        "resume_batch_analysis",
        "stop_batch_analysis",
        "add_batch_analysis_files",
        "remove_batch_analysis_item",
        "move_batch_analysis_item",
        "clear_batch_analysis",
        "batch_analysis_status",
        "flash_analysis",
        "set_flash_analysis",
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

#[tokio::test]
async fn invalid_review_range_and_batch_output_preserve_typed_ipc_errors() {
    let harness = Harness::new().await;
    harness
        .core
        .set_auto_quick_analyze_on_load(false)
        .await
        .unwrap();
    harness
        .invoke("open_bytes", json!({ "bytes": b"(;SZ[9];B[aa];W[bb])" }))
        .await;
    for (first, last, expected_first, expected_last) in [
        (Some(0), None, 0, 2),
        (None, Some(3), 1, 3),
        (Some(2), Some(1), 2, 1),
    ] {
        let error = request(&harness.main, "start_whole_game_analysis", json!({ "options": {
            "visits": 17, "scope": { "type": "Range", "data": { "first_move": first, "last_move": last } },
        } }), local_url()).await.unwrap_err();
        assert_eq!(
            error,
            json!({ "type": "InvalidWholeGameRange", "data": {
            "first_move": expected_first, "last_move": expected_last, "last_available": 2,
        } })
        );
    }
    let error = request(
        &harness.main,
        "start_batch_analysis",
        json!({ "options": {
        "files": ["one.sgf"], "visits": 17, "output": { "suffix": "../bad" },
    } }),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(
        error,
        json!({ "type": "Batch", "data": { "type": "InvalidSuffix" } })
    );
    harness.finish().await;
}
