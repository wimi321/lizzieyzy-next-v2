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
    assert_eq!(
        serde_json::to_value(&view).unwrap(),
        serde_json::to_value(within(harness.core.strength_estimate()).await.unwrap()).unwrap()
    );
    assert_eq!(
        harness.invoke("variation_tree", json!({})).await["revision"],
        initial.tree.structure_revision
    );
    assert_eq!(harness.invoke("step", json!({ "count": 0 })).await, 1);
    harness.core.clone().shutdown().await.unwrap();
    let error = request(&harness.main, "strength_estimate", json!({}), local_url())
        .await
        .unwrap_err();
    assert_eq!(error["type"], "App");
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
