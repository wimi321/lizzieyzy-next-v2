#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use crate::{AppConfig, EngineEntry, Settings, test_support::within};

#[tokio::test]
async fn missing_login_is_typed_and_does_not_stop_other_engines_or_document_commands() {
    let directory = tempfile::tempdir().unwrap();
    let (app, _) = crate::spawn(directory.path()).await.unwrap();
    let settings = within(app.add_engine("Zhizi".into(), ZhiziEngineOptions::default().profile()))
        .await
        .unwrap();
    let error = within(app.select_main_engine(settings.engines[0].id.clone()))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        AppError::Remote(RemoteEngineError::ZhiziLoginRequired)
    ));
    within(app.open_bytes(b"(;SZ[9];B[aa])".to_vec()))
        .await
        .unwrap();
    assert!(!within(app.zhizi_status()).await.unwrap().logged_in);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn startup_uses_first_usable_local_profile_without_modifying_saved_zhizi_selection() {
    let binary = fake_katago::binary_path().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let zhizi = ZhiziEngineOptions::default().profile();
    let local = EngineProfile {
        program: binary.clone(),
        model: binary.clone(),
        config: binary,
        ..Default::default()
    };
    let settings = Settings {
        engine: Some(zhizi.clone()),
        main_engine_id: Some("cloud".into()),
        engines: vec![
            EngineEntry {
                id: "cloud".into(),
                name: "Zhizi".into(),
                profile: zhizi,
            },
            EngineEntry {
                id: "broken".into(),
                name: "Missing".into(),
                profile: EngineProfile::default(),
            },
            EngineEntry {
                id: "local".into(),
                name: "Local".into(),
                profile: local,
            },
        ],
        ..Default::default()
    };
    settings.save(directory.path()).await.unwrap();
    let before = tokio::fs::read(directory.path().join("settings.json"))
        .await
        .unwrap();
    let (app, mut output) = crate::spawn_with_config(directory.path(), AppConfig::default())
        .await
        .unwrap();
    within(async {
        loop {
            if let AppEvent::EngineStatusChanged(EngineStatus::Ready { version }) =
                output.recv().await.unwrap()
            {
                assert!(version.contains("fake"));
                break;
            }
        }
    })
    .await;
    within(async {
        loop {
            if let AppEvent::AnalysisUpdated(view) = output.recv().await.unwrap() {
                assert!(view.visits > 0);
                break;
            }
        }
    })
    .await;
    assert_eq!(
        tokio::fs::read(directory.path().join("settings.json"))
            .await
            .unwrap(),
        before
    );
    assert_eq!(
        within(app.settings())
            .await
            .unwrap()
            .main_engine_id
            .as_deref(),
        Some("cloud")
    );
    // A later preference save must not persist the temporary local engine.
    within(app.set_play_sound(false)).await.unwrap();
    let saved = Settings::load(directory.path()).await.unwrap();
    assert_eq!(saved.main_engine_id.as_deref(), Some("cloud"));
    assert!(matches!(
        saved.engine.unwrap().remote,
        Some(RemoteEngineConfig::Zhizi { .. })
    ));
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn startup_without_login_and_without_local_engine_still_opens_documents() {
    let directory = tempfile::tempdir().unwrap();
    Settings {
        engine: Some(ZhiziEngineOptions::default().profile()),
        ..Default::default()
    }
    .save(directory.path())
    .await
    .unwrap();
    let (app, mut output) = crate::spawn(directory.path()).await.unwrap();
    within(async {
        loop {
            if let AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) =
                output.recv().await.unwrap()
            {
                assert_eq!(reason, "请先登录智子");
                break;
            }
        }
    })
    .await;
    within(app.open_bytes(b"(;SZ[9];B[aa])".to_vec()))
        .await
        .unwrap();
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn whole_game_review_finishes_through_local_gtp_bridge_and_retains_black_values() {
    use crate::{WholeGameOptions, WholeGameStatus, review::ReviewInput};
    use std::{collections::BTreeMap, path::PathBuf, time::Duration};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output =
        std::process::Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .args(["build", "--locked", "--manifest-path"])
            .arg(root.join("tools/fake-gtp/Cargo.toml"))
            .arg("--target-dir")
            .arg(root.join("target"))
            .arg("--message-format=json")
            .output()
            .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let program = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .find_map(|line| {
            let value: serde_json::Value = serde_json::from_str(line).ok()?;
            (value["reason"] == "compiler-artifact" && value["target"]["name"] == "fake-gtp")
                .then(|| value["executable"].as_str().map(PathBuf::from))
                .flatten()
        })
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let (mut actor, _) = super::super::tests::actor();
    actor.directory = directory.path().to_owned();
    actor.settings.pondering = false;
    actor.settings.auto_quick_analyze_on_load = false;
    actor.settings.cache_enabled = false;
    within(actor.open(b"(;SZ[9]KM[7.5];B[aa];W[bb];B[cc])".to_vec(), None))
        .await
        .unwrap();
    let engine = engine_session::spawn(
        TransportSpec::GtpBridge {
            inner: Box::new(TransportSpec::LocalProcess {
                program,
                args: vec!["--kata".into()],
                cwd: None,
                env: BTreeMap::new(),
            }),
        },
        engine_session::SessionConfig {
            max_restarts: 0,
            startup_timeout: Duration::from_secs(60),
            response_timeout: Duration::from_secs(60),
            ..Default::default()
        },
    );
    let mut status = engine.status();
    let version = within(async {
        loop {
            match status.borrow_and_update().clone() {
                engine_session::EngineStatus::Ready { version } => break version,
                engine_session::EngineStatus::Failed { reason, .. } => panic!("{reason}"),
                _ => {}
            }
            status.changed().await.unwrap();
        }
    })
    .await;
    let profile = ZhiziEngineOptions::default()
        .profile()
        .with_remote_version(&version);
    let fingerprint = profile.fingerprint().await.unwrap();
    actor.settings.engine = Some(profile);
    let (coordinator, snapshots) = actor.coordinator(&engine, &fingerprint).unwrap();
    actor.running = Some(Running {
        engine: engine.clone(),
        engine_status: status,
        analysis_status: coordinator.status(),
        coordinator,
        snapshots,
        fingerprint,
    });
    let progress = within(actor.review_command(ReviewInput::Start(WholeGameOptions {
        visits: 100,
        ..Default::default()
    })))
    .await
    .unwrap();
    assert_eq!(progress.total, 4);
    while actor.review.progress.as_ref().unwrap().status == WholeGameStatus::Running {
        let result = within(super::super::review::receive(&mut actor.review)).await;
        within(actor.review_signal(result)).await;
    }
    let progress = actor.review.progress.as_ref().unwrap();
    assert_eq!(progress.status, WholeGameStatus::Completed);
    assert_eq!(progress.completed, 4);
    assert_eq!(actor.review.results.len(), 4);
    for (node, _) in &actor.review.line {
        let view = actor.review_view(node.0).unwrap().unwrap();
        assert!(view.visits >= 100);
        let to_play = actor.document.position_at(*node).unwrap().board.to_play();
        let expected = if to_play == go_core::Color::Black {
            0.65
        } else {
            0.35
        };
        assert!((view.winrate - expected).abs() < 0.00001);
    }
    within(actor.stop_engine()).await.unwrap();
}
