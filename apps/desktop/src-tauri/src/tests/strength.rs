use super::{Harness, json, local_url, request, within};
use app_core::StrengthEstimateView;

#[tokio::test]
async fn strength_ipc_returns_the_core_snapshot_without_navigation() {
    let mut harness = Harness::new().await;
    harness.subscribe(94).await;
    harness.document(94, 0).await;
    harness
        .invoke("open_bytes", json!({ "bytes": b"(;SZ[19];B[aa];W[bb])" }))
        .await;
    let initial = harness.document(94, 1).await;
    let view: StrengthEstimateView =
        serde_json::from_value(harness.invoke("strength_estimate", json!({})).await).unwrap();
    assert_eq!(view.revision, 1);
    assert_eq!(view.total_moves, 2);
    assert_eq!(view.analyzed_moves, 0);
    assert!(!view.report.has_enough_data);
    assert_eq!(view.phases.bounds, [60, 150]);
    assert_eq!(view.phases.phases.len(), 3);
    assert_eq!(
        serde_json::to_value(&view).unwrap(),
        serde_json::to_value(within(harness.core.strength_estimate(None)).await.unwrap()).unwrap()
    );
    assert_eq!(
        harness.invoke("variation_tree", json!({})).await["revision"],
        initial.tree.structure_revision
    );
    let custom: StrengthEstimateView = serde_json::from_value(
        harness
            .invoke("strength_estimate", json!({ "bounds": [80, 50] }))
            .await,
    )
    .unwrap();
    assert_eq!(custom.phases.bounds, [80, 90]);
    assert_eq!(custom.report, view.report);
    assert_eq!(
        custom,
        within(harness.core.strength_estimate(Some([80, 50])))
            .await
            .unwrap()
    );
    let defaults: StrengthEstimateView = serde_json::from_value(
        harness
            .invoke("strength_estimate", json!({ "bounds": null }))
            .await,
    )
    .unwrap();
    assert_eq!(defaults, view);
    assert_eq!(harness.invoke("step", json!({ "count": 0 })).await, 1);
    harness.core.clone().shutdown().await.unwrap();
    let error = request(&harness.main, "strength_estimate", json!({}), local_url())
        .await
        .unwrap_err();
    assert_eq!(error["type"], "App");
}

#[tokio::test]
async fn strength_ipc_rejects_malformed_boundaries_without_changing_the_document() {
    let harness = Harness::new().await;
    for bounds in [json!([60]), json!([60, 150, 200]), json!([60.5, 150])] {
        assert!(
            request(
                &harness.main,
                "strength_estimate",
                json!({ "bounds": bounds }),
                local_url(),
            )
            .await
            .is_err()
        );
    }
    let view = within(harness.core.strength_estimate(None)).await.unwrap();
    assert_eq!(
        (view.revision, view.total_moves, view.analyzed_moves),
        (0, 0, 0)
    );
    assert_eq!(view.phases.bounds, [60, 150]);
    harness.finish().await;
}

#[tokio::test]
async fn strength_ipc_is_denied_to_other_windows_and_remote_origins() {
    let harness = Harness::new().await;
    let other =
        tauri::WebviewWindowBuilder::new(&harness.app, "strength-other", Default::default())
            .build()
            .unwrap();
    for (window, url) in [
        (&other, local_url()),
        (&harness.main, "https://example.com/"),
    ] {
        let error = request(window, "strength_estimate", json!({}), url)
            .await
            .unwrap_err();
        assert!(
            error
                .as_str()
                .is_some_and(|text| text.contains("not allowed")),
            "{error}"
        );
    }
    harness.finish().await;
}
