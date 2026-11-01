use super::{Harness, json, local_url, profile, request, within};
use app_core::{AppEvent, WholeGameSource};

#[tokio::test]
async fn automatic_curve_progress_source_survives_ipc() {
    let mut harness = Harness::new().await;
    let mut engine = profile(harness.directory.path());
    engine.extra_args = vec!["--visits-per-sec".into(), "1".into()];
    harness.subscribe(38).await;
    harness.document(38, 0).await;
    harness
        .invoke("configure_engine", json!({ "profile": engine }))
        .await;
    harness
        .invoke("open_bytes", json!({ "bytes": b"(;SZ[9];B[aa];W[bb])" }))
        .await;
    let automatic = within(async {
        loop {
            if let AppEvent::WholeGameProgress(progress) = harness.next().await.event
                && progress.source == WholeGameSource::AutomaticQuick
            {
                break progress;
            }
        }
    })
    .await;
    assert_eq!(automatic.visits, 1);
    assert_eq!(automatic.total, 3);
    let current = harness.invoke("whole_game_progress", json!({})).await;
    assert_eq!(current["source"], "AutomaticQuick");
    assert_eq!(current["visits"], 1);
    harness.finish().await;
}

#[tokio::test]
async fn auto_curve_settings_commands_read_update_and_persist() {
    let harness = Harness::new().await;
    assert_eq!(
        harness
            .invoke("auto_quick_analyze_on_load", json!({}))
            .await,
        true
    );
    assert_eq!(
        harness
            .invoke(
                "set_auto_quick_analyze_on_load",
                json!({ "enabled": false })
            )
            .await,
        false
    );
    assert_eq!(
        harness
            .invoke("auto_quick_analyze_on_load", json!({}))
            .await,
        false
    );
    assert_eq!(
        harness.invoke("settings", json!({})).await["auto_quick_analyze_on_load"],
        false
    );
    assert!(
        !app_core::Settings::load(harness.directory.path())
            .await
            .unwrap()
            .auto_quick_analyze_on_load
    );
    harness.finish().await;
}

#[tokio::test]
async fn auto_curve_settings_commands_are_local_main_only() {
    let harness = Harness::new().await;
    let other =
        tauri::WebviewWindowBuilder::new(&harness.app, "auto-curve-other", Default::default())
            .build()
            .unwrap();
    for command in [
        "auto_quick_analyze_on_load",
        "set_auto_quick_analyze_on_load",
    ] {
        for (window, url) in [
            (&other, local_url()),
            (&harness.main, "https://example.com/"),
        ] {
            let error = request(window, command, json!({ "enabled": false }), url)
                .await
                .unwrap_err();
            assert!(
                error
                    .as_str()
                    .is_some_and(|text| text.contains("not allowed"))
            );
        }
    }
    assert_eq!(
        harness
            .invoke("auto_quick_analyze_on_load", json!({}))
            .await,
        true
    );
    harness.finish().await;
}
