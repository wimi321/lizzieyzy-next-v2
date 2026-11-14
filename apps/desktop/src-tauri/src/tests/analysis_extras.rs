use super::{Harness, json, local_url, request};

#[tokio::test]
async fn transient_analysis_controls_round_trip_without_persisting() {
    let harness = Harness::new().await;
    harness
        .invoke("set_pondering", json!({ "enabled": false }))
        .await;
    let before = harness.invoke("settings", json!({})).await;
    harness
        .invoke(
            "set_analysis_extras",
            json!({ "extras": { "policy": true, "moves_ownership": true } }),
        )
        .await;
    harness
        .invoke("set_pure_network", json!({ "enabled": true }))
        .await;
    assert_eq!(
        harness.invoke("analysis_state", json!({})).await,
        json!({ "pondering": false, "pause_reason": "User", "pure_network": true })
    );
    assert_eq!(harness.invoke("settings", json!({})).await, before);
    harness
        .invoke("set_pure_network", json!({ "enabled": false }))
        .await;
    assert_eq!(
        harness.invoke("analysis_state", json!({})).await,
        json!({ "pondering": false, "pause_reason": "User", "pure_network": false })
    );
    assert!(
        request(
            &harness.main,
            "set_analysis_extras",
            json!({ "extras": { "policy": "yes" } }),
            local_url()
        )
        .await
        .is_err()
    );
    assert!(
        request(
            &harness.main,
            "set_pure_network",
            json!({ "enabled": 1 }),
            local_url()
        )
        .await
        .is_err()
    );
    harness.finish().await;
}

#[tokio::test]
async fn transient_analysis_commands_are_local_main_only() {
    let harness = Harness::new().await;
    let other = tauri::WebviewWindowBuilder::new(&harness.app, "extras-other", Default::default())
        .build()
        .unwrap();
    for command in ["set_analysis_extras", "set_pure_network"] {
        for (window, url) in [
            (&other, local_url()),
            (&harness.main, "https://example.com/"),
        ] {
            let error = request(
                window,
                command,
                json!({ "enabled": true, "extras": { "policy": true } }),
                url,
            )
            .await
            .unwrap_err();
            assert!(
                error
                    .as_str()
                    .is_some_and(|text| text.contains("not allowed"))
            );
        }
    }
    harness.finish().await;
}

#[test]
fn analysis_conflicts_preserve_typed_ipc_errors() {
    for (error, kind) in [
        (
            app_core::AppError::WholeGameAnalysisActive,
            "WholeGameAnalysisActive",
        ),
        (
            app_core::AppError::BatchAnalysisRunning,
            "BatchAnalysisRunning",
        ),
        (app_core::AppError::PureNetworkActive, "PureNetworkActive"),
    ] {
        assert_eq!(
            serde_json::to_value(crate::error::CommandError::from(error)).unwrap(),
            json!({ "type": kind })
        );
    }
}
