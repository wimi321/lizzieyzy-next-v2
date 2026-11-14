#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;
#[path = "remote_api/server.rs"]
mod server;

use app_core::{
    AppConfig, AppCore, AppEvent, AppSubscription, EngineStatus, RemoteEngineConfig,
    RemoteEngineInput, Settings, WholeGameOptions, WholeGameProgress, WholeGameStatus,
    commentary_keys::{KeyStoreFuture, SecretStore},
};
use common::within;
use serde_json::Value;
use server::Server;
use std::{path::Path, sync::Arc, time::Duration};

fn config() -> AppConfig {
    let mut config = AppConfig::default();
    config.analysis.live.max_visits = 1;
    config.analysis.live.report_interval = Duration::from_millis(10);
    config.session.startup_timeout = Duration::from_secs(60);
    config.session.response_timeout = Duration::from_secs(60);
    config.session.write_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(1);
    config.session.max_restarts = 0;
    config
}

async fn app(path: &Path, config: AppConfig) -> (AppCore, AppSubscription) {
    let (core, output) = within(app_core::spawn_with_config(path, config))
        .await
        .unwrap();
    within(core.set_auto_quick_analyze_on_load(false))
        .await
        .unwrap();
    (core, output)
}

async fn review_completed(output: &mut AppSubscription, initial: &WholeGameProgress) {
    let mut previous = initial.completed;
    loop {
        let progress = within(async {
            loop {
                match output.recv().await.expect("application closed") {
                    AppEvent::WholeGameProgress(progress) => {
                        assert_eq!(progress.total, initial.total);
                        assert!(progress.completed >= previous);
                        match progress.status {
                            WholeGameStatus::Completed => return progress,
                            WholeGameStatus::Running if progress.completed > previous => {
                                return progress;
                            }
                            WholeGameStatus::Running => {}
                            other => panic!("unexpected review state: {other:?}"),
                        }
                    }
                    AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                        panic!("{reason}")
                    }
                    _ => {}
                }
            }
        })
        .await;
        if progress.status == WholeGameStatus::Completed {
            assert_eq!(progress.completed, progress.total);
            return;
        }
        previous = progress.completed;
    }
}

#[tokio::test]
async fn remote_probe_selection_live_review_and_cancel_use_the_existing_pipeline() {
    let mut service = Server::start().await;
    let directory = tempfile::tempdir().unwrap();
    let (core, mut events) = app(directory.path(), config()).await;
    let entry = RemoteEngineInput::WebSocket {
        url: service.url.clone(),
    };
    assert_eq!(
        within(core.test_remote_engine(entry.clone()))
            .await
            .unwrap(),
        "1.16.5-fake"
    );
    service.closed().await;
    let added = within(core.add_remote_engine("LAN GPU".into(), entry))
        .await
        .unwrap();
    let id = added.engines[0].id.clone();
    let before_version = added.engines[0].profile.fingerprint().await.unwrap();
    let revision = within(core.open_bytes(b"(;SZ[9]KM[7.5];B[dd];W[ee])".to_vec()))
        .await
        .unwrap();
    let selected = within(core.select_main_engine(id)).await.unwrap();
    let profile = selected.engine.as_ref().unwrap();
    assert!(
        matches!(&profile.remote, Some(RemoteEngineConfig::WebSocket { version: Some(version), .. }) if version == "1.16.5-fake")
    );
    assert_ne!(profile.fingerprint().await.unwrap(), before_version);
    assert_eq!(
        Settings::load(directory.path()).await.unwrap().engine,
        selected.engine
    );
    within(async {
        loop {
            if let AppEvent::AnalysisUpdated(view) = events.recv().await.unwrap()
                && view.revision == revision
                && view.visits > 0
            {
                break;
            }
        }
    })
    .await;
    let live = service
        .request(|request| request.get("moves").is_some())
        .await;
    assert_eq!(
        live["overrideSettings"]["reportAnalysisWinratesAs"],
        "BLACK"
    );
    within(core.set_pondering(false)).await.unwrap();
    let started = within(core.start_whole_game_analysis(WholeGameOptions {
        visits: 3,
        force: true,
        ..WholeGameOptions::default()
    }))
    .await
    .unwrap();
    assert_eq!(started.total, 3);
    review_completed(&mut events, &started).await;
    let batch = service
        .request(|request| {
            request["analyzeTurns"]
                .as_array()
                .is_some_and(|turns| turns.len() == 3)
        })
        .await;
    assert_eq!(batch["analyzeTurns"], serde_json::json!([0, 1, 2]));
    assert_eq!(
        batch["overrideSettings"]["reportAnalysisWinratesAs"],
        "BLACK"
    );
    let tree = within(core.variation_tree()).await.unwrap();
    for node in tree.nodes {
        let result = within(core.whole_game_result(node.id))
            .await
            .unwrap()
            .unwrap();
        assert!(result.visits >= 3);
    }
    within(core.start_whole_game_analysis(WholeGameOptions {
        visits: 1_000_000,
        force: true,
        ..WholeGameOptions::default()
    }))
    .await
    .unwrap();
    let long = service
        .request(|request| request["maxVisits"] == 1_000_000)
        .await;
    assert_eq!(
        within(core.cancel_whole_game_analysis())
            .await
            .unwrap()
            .status,
        WholeGameStatus::Cancelled
    );
    let terminated = service
        .request(|request| request["action"] == "terminate" && request["terminateId"] == long["id"])
        .await;
    assert_eq!(terminated["terminateId"], long["id"]);
    within(core.shutdown()).await.unwrap();
    service.closed().await;
    service.shutdown().await;
}

/// A deterministic read-only saved credential. No operating-system keychain.
struct SavedLink(String);
impl std::fmt::Debug for SavedLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SavedLink([redacted])")
    }
}
impl SecretStore for SavedLink {
    fn read(&self, _: String) -> KeyStoreFuture<'_, Option<String>> {
        Box::pin(async { Ok(Some(self.0.clone())) })
    }
    fn write(&self, _: String, _: String) -> KeyStoreFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn delete(&self, _: String) -> KeyStoreFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}

#[tokio::test]
async fn unavailable_remote_and_missing_memory_secret_do_not_block_app_startup() {
    let mut service = Server::start().await;
    let directory = tempfile::tempdir().unwrap();
    let saved_link = Arc::new(SavedLink(service.url.clone()));
    let mut saved_config = config();
    saved_config.commentary_key_store = Some(saved_link.clone());
    let (core, _) = app(directory.path(), saved_config).await;
    let added = within(core.add_remote_engine(
        "LAN".into(),
        RemoteEngineInput::WebSocket {
            url: service.url.clone(),
        },
    ))
    .await
    .unwrap();
    within(core.select_main_engine(added.engines[0].id.clone()))
        .await
        .unwrap();
    within(core.shutdown()).await.unwrap();
    service.closed().await;
    service.shutdown().await;
    for stored in [true, false] {
        let mut retry = config();
        if stored {
            retry.commentary_key_store = Some(saved_link.clone());
        }
        let (core, mut output) = app(directory.path(), retry).await;
        // Commands remain usable whether the connection failed or its secret
        // existed only in the preceding application's memory.
        within(core.open_bytes(b"(;SZ[9];B[aa])".to_vec()))
            .await
            .unwrap();
        within(core.step(1)).await.unwrap();
        within(async {
            loop {
                if let AppEvent::EngineStatusChanged(EngineStatus::Failed { .. }) =
                    output.recv().await.unwrap()
                {
                    break;
                }
            }
        })
        .await;
        let settings = within(core.settings()).await.unwrap();
        assert!(settings.engine.as_ref().unwrap().remote.is_some());
        let persisted: Value =
            serde_json::from_slice(&std::fs::read(directory.path().join("settings.json")).unwrap())
                .unwrap();
        assert!(!persisted.to_string().contains("fixture-only"));
        within(core.shutdown()).await.unwrap();
    }
}
