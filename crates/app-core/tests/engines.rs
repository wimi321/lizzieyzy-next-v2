#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{AppConfig, AppEvent, EngineProfile, EngineStatus};
use std::{future::Future, time::Duration};

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("safety deadline")
}

#[tokio::test]
async fn public_engine_commands_persist_selection_and_closed_state_across_restart() {
    let binary = fake_katago::binary_path().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut config = AppConfig::default();
    config.analysis.live.max_visits = 1;
    config.session.shutdown_timeout = Duration::from_secs(60);
    config.session.write_timeout = Duration::from_secs(60);
    let (app, mut events) = within(app_core::spawn_with_config(
        directory.path(),
        config.clone(),
    ))
    .await
    .unwrap();
    let profile = EngineProfile {
        program: binary.clone(),
        model: binary.clone(),
        config: binary,
        ..EngineProfile::default()
    };
    let settings = within(app.add_engine("Primary".into(), profile.clone()))
        .await
        .unwrap();
    let id = settings.engines[0].id.clone();
    within(app.select_main_engine(id.clone())).await.unwrap();
    within(async {
        loop {
            if let Some(AppEvent::AnalysisUpdated(view)) = events.recv().await {
                assert_eq!(view.node_id, 0);
                if !view.is_during_search {
                    break;
                }
            }
        }
    })
    .await;
    within(app.restart_engine()).await.unwrap();
    within(app.update_engine(id.clone(), "Renamed".into(), profile.clone()))
        .await
        .unwrap();
    let changed = within(app.reorder_engines(vec![id.clone()])).await.unwrap();
    assert_eq!(changed.engines[0].name, "Renamed");
    let closed = within(app.close_engine()).await.unwrap();
    assert!(closed.engine.is_none());
    within(app.shutdown()).await.unwrap();
    let (app, mut events) = within(app_core::spawn_with_config(directory.path(), config))
        .await
        .unwrap();
    assert_eq!(within(app.settings()).await.unwrap(), closed);
    within(async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::EngineStatusChanged(status) => {
                    assert_eq!(status, EngineStatus::Stopped);
                    break;
                }
                AppEvent::AnalysisUpdated(_) => panic!("closed engine must not analyze"),
                _ => {}
            }
        }
    })
    .await;
    assert_eq!(
        within(app.auto_configure_engine(Default::default(), "unused-template".into()))
            .await
            .unwrap(),
        profile
    );
    assert_eq!(within(app.settings()).await.unwrap(), closed);
    let empty = within(app.remove_engine(id)).await.unwrap();
    assert!(empty.engines.is_empty());
    assert!(empty.main_engine_id.is_none());
    within(app.shutdown()).await.unwrap();
}
