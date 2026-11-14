#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::{AppConfig, AppEvent, DownloadPhase, assets::AssetCatalog};
use std::{future::Future, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

mod diagnostics;

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("safety deadline")
}

fn config() -> AppConfig {
    let mut config = AppConfig::default();
    config.analysis.live.max_visits = 1;
    config.analysis.live.report_interval = Duration::from_secs(60);
    config.session.write_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(60);
    config
}

#[tokio::test]
async fn download_events_and_weight_switch_share_the_existing_transaction_and_rollback() {
    let binary = fake_katago::binary_path().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let data_directory = directory.path().join("application-data");
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/weight", listener.local_addr().unwrap());
    let bytes = b"a small pinned test weight";
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            request.push(socket.read_u8().await.unwrap());
        }
        socket
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    bytes.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        socket.write_all(bytes).await.unwrap();
    });
    let mut config = config();
    config.cache_directory = Some(data_directory.clone());
    config.asset_catalog = Some(AssetCatalog::test_weight("test-weight", &url, bytes));
    let (core, mut events) = crate::spawn_with_config(directory.path(), config)
        .await
        .unwrap();
    let initial = core.official_weights().await.unwrap();
    assert_eq!(initial.len(), 1);
    assert!(!initial[0].downloaded);
    assert!(
        core.use_official_weight("test-weight".into())
            .await
            .is_err()
    );
    let fault = directory.path().join("startup-fault-consumed");
    tokio::fs::write(&fault, b"already consumed").await.unwrap();
    let original = EngineProfile {
        remote: None,
        human_model: None,
        program: binary.clone(),
        model: binary.clone(),
        config: binary,
        extra_args: vec![
            "--fail-startup".into(),
            "--fault-once-file".into(),
            fault.to_string_lossy().into_owned(),
        ],
        canonical_model_name: None,
    };
    let settings = core
        .add_engine("Current".into(), original.clone())
        .await
        .unwrap();
    let before = within(core.select_main_engine(settings.engines[0].id.clone()))
        .await
        .unwrap();
    core.start_asset_download("test-weight".into())
        .await
        .unwrap();
    let completed = within(async {
        loop {
            if let Some(AppEvent::AssetDownloadProgress(status)) = events.recv().await {
                match status.phase {
                    DownloadPhase::Completed => break status,
                    DownloadPhase::Failed => panic!("{:?}", status.error),
                    _ => {}
                }
            }
        }
    })
    .await;
    within(server).await.unwrap();
    assert_eq!(completed.downloaded_bytes, bytes.len() as u64);
    let downloaded = std::path::PathBuf::from(completed.path.unwrap());
    assert!(downloaded.starts_with(&data_directory));
    assert_eq!(tokio::fs::read(&downloaded).await.unwrap(), bytes);
    assert_eq!(core.settings().await.unwrap(), before);
    assert!(core.official_weights().await.unwrap()[0].downloaded);

    // The current fake process has already passed its startup gate. Only the
    // candidate fails, proving that switching uses the P2-11 rollback path.
    tokio::fs::remove_file(&fault).await.unwrap();
    let error = within(core.use_official_weight("test-weight".into()))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("模型找不到"), "{error}");
    assert_eq!(core.settings().await.unwrap(), before);
    assert_eq!(Settings::load(directory.path()).await.unwrap(), before);
    let revision = within(core.play(go_core::Move::Pass(go_core::Color::Black)))
        .await
        .unwrap();
    within(async {
        loop {
            if let Some(AppEvent::AnalysisUpdated(view)) = events.recv().await
                && view.revision == revision
                && !view.is_during_search
            {
                assert!(view.visits > 0);
                break;
            }
        }
    })
    .await;
    let switched = within(core.use_official_weight("test-weight".into()))
        .await
        .unwrap();
    assert_eq!(switched.main_engine_id, before.main_engine_id);
    assert_eq!(switched.engine.as_ref().unwrap().model, downloaded);
    assert_eq!(switched.engines[0].name, "Current");
    assert_eq!(Settings::load(directory.path()).await.unwrap(), switched);
    within(core.clone().shutdown()).await.unwrap();
    assert!(matches!(
        core.asset_download_status().await,
        Err(AppError::Closed)
    ));
}

#[tokio::test]
async fn stale_preparation_does_not_overwrite_a_new_main_engine() {
    let binary = fake_katago::binary_path().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = crate::spawn_with_config(directory.path(), config())
        .await
        .unwrap();
    let profile = EngineProfile {
        remote: None,
        human_model: None,
        program: binary.clone(),
        model: binary.clone(),
        config: binary,
        extra_args: vec![],
        canonical_model_name: None,
    };
    let added = core
        .add_engine("First".into(), profile.clone())
        .await
        .unwrap();
    let selected = within(core.select_main_engine(added.engines[0].id.clone()))
        .await
        .unwrap();
    let expected = selected_entry(&selected).unwrap();
    let added = core
        .add_engine("Second".into(), profile.clone())
        .await
        .unwrap();
    let second = within(core.select_main_engine(added.engines[1].id.clone()))
        .await
        .unwrap();
    let error = core
        .replace_main_profile(expected, profile)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("changed during preparation"));
    assert_eq!(core.settings().await.unwrap(), second);
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn ordinary_startup_tolerates_missing_human_model_and_never_sets_a_human_profile() {
    let binary = fake_katago::binary_path().unwrap();
    for present in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let human = directory.path().join("human-model.bin.gz");
        if present {
            std::fs::write(&human, b"fake human model").unwrap();
        }
        let requests = directory.path().join("requests.jsonl");
        let launches = directory.path().join("launches.jsonl");
        let profile = EngineProfile {
            program: binary.clone(),
            model: binary.clone(),
            config: binary.clone(),
            remote: None,
            human_model: Some(human.clone()),
            extra_args: vec![
                "--request-log".into(),
                requests.to_string_lossy().into_owned(),
                "--launch-log".into(),
                launches.to_string_lossy().into_owned(),
            ],
            ..Default::default()
        };
        Settings {
            engine: Some(profile),
            ..Default::default()
        }
        .save(directory.path())
        .await
        .unwrap();
        let (core, mut events) = crate::spawn_with_config(directory.path(), config())
            .await
            .unwrap();
        within(async {
            loop {
                if let Some(AppEvent::AnalysisUpdated(view)) = events.recv().await
                    && !view.is_during_search
                    && view.visits > 0
                {
                    break;
                }
            }
        })
        .await;
        // Receiving a final response is the barrier after the fake writes its
        // request log; never conclude absence immediately after a command.
        let received: Vec<serde_json::Value> = std::fs::read_to_string(&requests)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .filter(|value: &serde_json::Value| value.get("moves").is_some())
            .collect();
        assert!(!received.is_empty());
        assert!(
            received
                .iter()
                .all(|query| query["overrideSettings"].get("humanSLProfile").is_none())
        );
        let launch = std::fs::read_to_string(&launches).unwrap();
        let args: Vec<String> = serde_json::from_str(launch.lines().next().unwrap()).unwrap();
        assert_eq!(args.iter().any(|arg| arg == "-human-model"), present);
        assert_eq!(human.exists(), present);
        within(core.shutdown()).await.unwrap();
    }
}
