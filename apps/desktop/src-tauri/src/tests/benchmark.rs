use super::{Harness, json, local_url, profile, request, within};
use app_core::{AppEvent, BenchmarkPhase, BenchmarkStatus};

#[tokio::test]
async fn benchmark_commands_forward_results_errors_reset_and_replay() {
    let mut harness = Harness::new().await;
    harness.subscribe(101).await;
    harness.document(101, 0).await;
    let initial = harness.invoke("benchmark_status", json!({})).await;
    assert_eq!(initial["phase"], "Idle");
    assert!(initial["last_result"].is_null());
    let error = request(&harness.main, "start_benchmark", json!({}), local_url())
        .await
        .unwrap_err();
    assert_eq!(error["type"], "App");
    assert!(error["data"].as_str().is_some_and(|text| !text.is_empty()));

    // A real IPC invocation reaches AppCore and its independent fake benchmark
    // process; the Tauri adapter does not manufacture benchmark state.
    let mut profile = profile(harness.directory.path());
    // This explicitly synthetic transcript covers both forms of official
    // additional advice; the real Metal capture covers the batch-only case
    // in the protocol golden tests.
    let recording = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../fixtures/katago/benchmark-additional-recommendations.txt");
    profile.extra_args = vec![
        "--benchmark-replay".into(),
        recording.to_str().unwrap().into(),
    ];
    harness
        .invoke("configure_engine", json!({ "profile": profile }))
        .await;
    within(async {
        loop {
            if matches!(
                harness.next().await.event,
                AppEvent::EngineStatusChanged(app_core::EngineStatus::Ready { .. })
            ) {
                break;
            }
        }
    })
    .await;
    let started = harness.invoke("start_benchmark", json!({})).await;
    assert_eq!(started["phase"], "Running");
    let completed: BenchmarkStatus = within(async {
        loop {
            if let AppEvent::BenchmarkProgress(status) = harness.next().await.event {
                match status.phase {
                    BenchmarkPhase::Completed => break status,
                    BenchmarkPhase::Failed => panic!("benchmark failed: {:?}", status.error),
                    _ => {}
                }
            }
        }
    })
    .await;
    let result = completed.last_result.as_ref().unwrap();
    assert_eq!(result.recommended_threads, 4);
    assert_eq!(result.nn_max_batch_size, Some(2));
    assert_eq!(result.num_nn_server_threads_per_model, Some(2));
    // Display the completed measurement for the adopted combination. The
    // original thread-search table remains independent of additional runs.
    assert_eq!(result.visits_per_second, 44.95);
    assert_eq!(result.nn_evals_per_second, 44.27);
    assert_eq!(completed.measurements.len(), 1);
    assert_eq!(completed.measurements[0].visits_per_second, 15.75);
    let settings = harness.invoke("settings", json!({})).await;
    assert_eq!(
        settings["benchmarks"][&result.engine_fingerprint],
        serde_json::to_value(result).unwrap()
    );
    assert_eq!(
        harness.invoke("benchmark_status", json!({})).await,
        serde_json::to_value(&completed).unwrap()
    );
    harness.subscribe(102).await;
    within(async {
        loop {
            let message = harness.next().await;
            if message.callback == 102
                && let AppEvent::BenchmarkProgress(status) = message.event
            {
                assert_eq!(status, completed);
                break;
            }
        }
    })
    .await;
    let reset = harness.invoke("reset_benchmark", json!({})).await;
    assert_eq!(reset["phase"], "Idle");
    assert!(reset["last_result"].is_null());
    assert_eq!(
        harness.invoke("settings", json!({})).await["benchmarks"],
        json!({})
    );
    assert_eq!(harness.invoke("cancel_benchmark", json!({})).await, reset);
    harness.finish().await;
}

#[tokio::test]
async fn benchmark_commands_are_denied_to_other_windows_and_remote_origins() {
    let harness = Harness::new().await;
    let other =
        tauri::WebviewWindowBuilder::new(&harness.app, "benchmark-other", Default::default())
            .build()
            .unwrap();
    for command in [
        "start_benchmark",
        "cancel_benchmark",
        "benchmark_status",
        "reset_benchmark",
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
