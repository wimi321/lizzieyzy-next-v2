use super::{Harness, json, local_url, request};

#[test]
fn human_model_load_failure_preserves_reason_through_command_error_round_trip() {
    let error = app_core::CoachError::HumanModelLoadFailed {
        reason: "injected human model load failure".into(),
    };
    let command = crate::error::CommandError::from(app_core::AppError::Coach(error.clone()));
    let wire = serde_json::to_value(command).unwrap();
    assert_eq!(
        wire,
        json!({
            "type": "Coach",
            "data": {
                "type": "HumanModelLoadFailed",
                "data": { "reason": "injected human model load failure" }
            }
        })
    );
    assert_eq!(
        serde_json::from_value::<app_core::CoachError>(wire["data"].clone()).unwrap(),
        error
    );
}

#[tokio::test]
async fn coach_commands_and_missing_model_state_round_trip_with_typed_errors() {
    let harness = Harness::new().await;
    assert_eq!(harness.invoke("coach_view", json!({})).await, json!(null));
    assert_eq!(
        harness.invoke("humansl_model_status", json!({})).await,
        json!({"type":"Missing"})
    );
    assert_eq!(
        harness.invoke("pause_asset_download", json!({})).await["phase"],
        "Idle"
    );
    for (command, arguments) in [
        (
            "start_coach",
            json!({"setup":{"rank":{"type":"Dan","data":10}}}),
        ),
        ("coach_hint", json!({})),
        ("coach_decision", json!({"decision":"Retry"})),
    ] {
        let error = request(&harness.main, command, arguments, local_url())
            .await
            .unwrap_err();
        assert_eq!(error["type"], "Coach");
    }
    let file = harness.directory.path().join("bad-human.bin.gz");
    std::fs::write(&file, b"not the pinned model").unwrap();
    assert!(
        request(
            &harness.main,
            "import_humansl_model",
            json!({"path":file}),
            local_url()
        )
        .await
        .is_err()
    );
    assert_eq!(
        harness.invoke("humansl_model_status", json!({})).await,
        json!({"type":"Missing"})
    );
    harness.finish().await;
}

#[tokio::test]
async fn coach_commands_are_restricted_to_local_main() {
    let harness = Harness::new().await;
    let other = tauri::WebviewWindowBuilder::new(&harness.app, "coach-other", Default::default())
        .build()
        .unwrap();
    for command in [
        "start_coach",
        "coach_view",
        "coach_hint",
        "coach_decision",
        "coach_review",
        "humansl_model_status",
        "import_humansl_model",
        "pause_asset_download",
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
