#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use app_core::{
    AppError, CoachSetup, EngineEntry, EngineProfile, EngineSettings, RemoteEngineConfig,
    RemoteEngineError, Settings, SshAuthConfig, engine_setup::SearchRoots,
};
use common::within;

fn remote_profiles() -> [EngineProfile; 2] {
    [
        RemoteEngineConfig::WebSocket {
            host: "127.0.0.1".into(),
            port: 9,
            secure: false,
            identity: "a".repeat(64),
            version: None,
        },
        RemoteEngineConfig::Ssh {
            host: "127.0.0.1".into(),
            port: 9,
            user: "test-user".into(),
            command: "katago analysis".into(),
            auth: SshAuthConfig::Password,
            version: None,
        },
    ]
    .map(|remote| EngineProfile {
        remote: Some(remote),
        ..Default::default()
    })
}

fn unsupported<T: std::fmt::Debug>(result: Result<T, AppError>) {
    assert!(
        matches!(
            result,
            Err(AppError::Remote(RemoteEngineError::Unsupported))
        ),
        "{result:?}"
    );
}

#[tokio::test]
async fn selected_remote_rejects_local_operations_before_files_processes_or_network() {
    // Keep the saved selection closed: no transport can be opened by setup.
    // Empty program/model/config and absent credentials prove the restriction
    // wins over local file probing and remote authentication failures.
    for profile in remote_profiles() {
        let dir = tempfile::tempdir().unwrap();
        let settings = Settings {
            engines: vec![EngineEntry {
                id: "remote".into(),
                name: "Remote".into(),
                profile,
            }],
            main_engine_id: Some("remote".into()),
            engine: None,
            ..Default::default()
        };
        settings.save(dir.path()).await.unwrap();
        let (app, _events) = within(app_core::spawn(dir.path())).await.unwrap();
        let before = tokio::fs::read(dir.path().join("settings.json"))
            .await
            .unwrap();
        unsupported(within(app.start_benchmark()).await);
        unsupported(within(app.reset_benchmark()).await);
        unsupported(
            within(
                app.auto_configure_engine(
                    SearchRoots::default(),
                    dir.path().join("absent-template"),
                ),
            )
            .await,
        );
        unsupported(within(app.start_coach(CoachSetup::default())).await);
        assert_eq!(
            tokio::fs::read(dir.path().join("settings.json"))
                .await
                .unwrap(),
            before
        );
        assert!(!dir.path().join("analysis.cfg").exists());
        assert!(!dir.path().join("ssh").exists());
        // Ordinary document commands and query settings remain available.
        within(app.open_bytes(b"(;SZ[9];B[aa];W[bb])".to_vec()))
            .await
            .unwrap();
        within(app.step(1)).await.unwrap();
        let changed = EngineSettings {
            report_interval_ms: 250,
            ..Default::default()
        };
        assert_eq!(
            within(app.set_engine_settings(changed.clone()))
                .await
                .unwrap(),
            changed
        );
        within(app.shutdown()).await.unwrap();
    }
}
