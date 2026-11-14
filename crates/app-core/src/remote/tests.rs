#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use crate::test_support::within;
use crate::{
    AppConfig, AppEvent, EngineStatus,
    commentary_keys::{CommentaryKeys, KeyStoreFuture, KeyStoreUnavailable, SecretStore},
};
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::{mpsc, oneshot};

#[derive(Debug)]
struct Store(mpsc::UnboundedSender<StoreCommand>);
enum StoreCommand {
    Read(
        String,
        oneshot::Sender<Result<Option<String>, KeyStoreUnavailable>>,
    ),
    Write(
        String,
        String,
        oneshot::Sender<Result<(), KeyStoreUnavailable>>,
    ),
    Delete(String, oneshot::Sender<Result<(), KeyStoreUnavailable>>),
    Snapshot(oneshot::Sender<BTreeMap<String, String>>),
}
impl Store {
    fn new(available: bool) -> Arc<Self> {
        let (tx, mut rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut values = BTreeMap::new();
            while let Some(command) = rx.recv().await {
                match command {
                    StoreCommand::Read(key, reply) => {
                        let _ = reply.send(if available {
                            Ok(values.get(&key).cloned())
                        } else {
                            Err(KeyStoreUnavailable)
                        });
                    }
                    StoreCommand::Write(key, value, reply) => {
                        let _ = reply.send(if available {
                            values.insert(key, value);
                            Ok(())
                        } else {
                            Err(KeyStoreUnavailable)
                        });
                    }
                    StoreCommand::Delete(key, reply) => {
                        let _ = reply.send(if available {
                            values.remove(&key);
                            Ok(())
                        } else {
                            Err(KeyStoreUnavailable)
                        });
                    }
                    StoreCommand::Snapshot(reply) => {
                        let _ = reply.send(values.clone());
                    }
                }
            }
        });
        Arc::new(Self(tx))
    }
    async fn snapshot(&self) -> BTreeMap<String, String> {
        let (tx, rx) = oneshot::channel();
        self.0.send(StoreCommand::Snapshot(tx)).unwrap();
        within(rx).await.unwrap()
    }
}
impl SecretStore for Store {
    fn read(&self, account: String) -> KeyStoreFuture<'_, Option<String>> {
        Box::pin(async move {
            let (tx, rx) = oneshot::channel();
            self.0
                .send(StoreCommand::Read(account, tx))
                .map_err(|_| KeyStoreUnavailable)?;
            rx.await.map_err(|_| KeyStoreUnavailable)?
        })
    }
    fn write(&self, account: String, key: String) -> KeyStoreFuture<'_, ()> {
        Box::pin(async move {
            let (tx, rx) = oneshot::channel();
            self.0
                .send(StoreCommand::Write(account, key, tx))
                .map_err(|_| KeyStoreUnavailable)?;
            rx.await.map_err(|_| KeyStoreUnavailable)?
        })
    }
    fn delete(&self, account: String) -> KeyStoreFuture<'_, ()> {
        Box::pin(async move {
            let (tx, rx) = oneshot::channel();
            self.0
                .send(StoreCommand::Delete(account, tx))
                .map_err(|_| KeyStoreUnavailable)?;
            rx.await.map_err(|_| KeyStoreUnavailable)?
        })
    }
}
fn ssh(password: &str) -> RemoteEngineInput {
    RemoteEngineInput::Ssh {
        host: "127.0.0.1".into(),
        port: 2222,
        user: "tester".into(),
        command: "katago analysis -model network -config analysis.cfg".into(),
        auth: RemoteSshAuth::Password {
            password: password.into(),
        },
    }
}
fn link() -> RemoteEngineInput {
    RemoteEngineInput::WebSocket {
        url: "ws://127.0.0.1:9876/private-path-token?token=private-query-token".into(),
    }
}

#[tokio::test]
async fn secrets_share_store_without_changing_commentary_accounts_or_leaking_to_settings_or_logs() {
    use tracing_subscriber::prelude::*;
    let directory = tempfile::tempdir().unwrap();
    let store = Store::new(true);
    let support = crate::support::spawn(directory.path().join("diagnostic"))
        .await
        .unwrap();
    let _guard =
        tracing::subscriber::set_default(tracing_subscriber::registry().with(support.layer()));
    let (app, _) = crate::spawn_with_config(
        directory.path(),
        AppConfig {
            commentary_key_store: Some(store.clone()),
            ..AppConfig::default()
        },
    )
    .await
    .unwrap();
    within(app.add_remote_engine("Self hosted".into(), link()))
        .await
        .unwrap();
    within(app.add_remote_engine("SSH password".into(), ssh("private-ssh-password")))
        .await
        .unwrap();
    let RemoteEngineInput::Ssh {
        host,
        port,
        user,
        command,
        ..
    } = ssh("")
    else {
        unreachable!()
    };
    within(app.add_remote_engine(
        "SSH key".into(),
        RemoteEngineInput::Ssh {
            host,
            port,
            user,
            command,
            auth: RemoteSshAuth::PrivateKey {
                path: directory.path().join("key"),
                passphrase: Some("private-key-passphrase".into()),
            },
        },
    ))
    .await
    .unwrap();
    let settings = within(app.settings()).await.unwrap(); // actor barrier includes secret commit
    assert_eq!(settings.engines.len(), 3);
    let mut commentary = CommentaryKeys::new(Some(store.clone()));
    within(commentary.set("http://127.0.0.1:9000/v1", "private-ai-key".into(), true))
        .await
        .unwrap();
    let keys = store.snapshot().await;
    assert_eq!(keys.len(), 4);
    assert_eq!(
        keys.get("http://127.0.0.1:9000/v1").unwrap(),
        "private-ai-key"
    );
    assert_eq!(
        keys.keys().filter(|key| key.starts_with("remote:")).count(),
        3
    );
    tracing::info!(input = ?link(), auth = ?RemoteSshAuth::Password { password: "private-ssh-password".into() }, "remote operation completed");
    within(app.shutdown()).await.unwrap();
    within(support.flush()).await.unwrap();
    let disk = tokio::fs::read_to_string(directory.path().join("settings.json"))
        .await
        .unwrap();
    let logs = tokio::fs::read_to_string(directory.path().join("diagnostic/logs/app.log"))
        .await
        .unwrap();
    assert!(logs.contains("remote operation completed"));
    for secret in [
        "private-path-token",
        "private-query-token",
        "private-ssh-password",
        "private-key-passphrase",
        "private-ai-key",
        "ws://127.0.0.1:9876",
    ] {
        assert!(!disk.contains(secret), "settings leaked {secret}");
        assert!(!logs.contains(secret), "logs leaked {secret}");
    }
    support.shutdown().await.unwrap();
}

#[tokio::test]
async fn unavailable_store_keeps_secrets_only_for_current_lifetime_and_startup_still_succeeds() {
    let store = Store::new(false);
    let mut secrets = RemoteSecrets::new(Some(store.clone()));
    let profile = within(secrets.import(link())).await.unwrap();
    within(secrets.commit()).await;
    let remote = profile.remote.as_ref().unwrap();
    assert!(matches!(
        within(secrets.transport(remote, PathBuf::from("unused/known_hosts")))
            .await
            .unwrap(),
        engine_session::TransportSpec::WebSocket { .. }
    ));
    assert!(store.snapshot().await.is_empty());
    let directory = tempfile::tempdir().unwrap();
    let settings = Settings {
        engine: Some(profile),
        ..Settings::default()
    };
    settings.save(directory.path()).await.unwrap();
    let (app, mut output) = within(crate::spawn_with_config(
        directory.path(),
        AppConfig {
            commentary_key_store: Some(store),
            ..AppConfig::default()
        },
    ))
    .await
    .unwrap();
    within(async {
        loop {
            if let AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) =
                output.recv().await.unwrap()
            {
                assert!(reason.contains("credentials"));
                break;
            }
        }
    })
    .await;
    assert_eq!(within(app.settings()).await.unwrap().engines.len(), 1);
    within(app.open_bytes(b"(;SZ[9];B[aa])".to_vec()))
        .await
        .unwrap();
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn credential_staging_rolls_back_failed_edits_and_commits_successful_edits() {
    let store = Store::new(true);
    let mut secrets = RemoteSecrets::new(Some(store.clone()));
    let first = within(secrets.import(ssh("old-password"))).await.unwrap();
    within(secrets.commit()).await;
    let saved = store.snapshot().await;
    within(secrets.import(ssh("rejected-password")))
        .await
        .unwrap();
    assert_eq!(store.snapshot().await, saved);
    secrets.rollback();
    let transport = within(secrets.transport(first.remote.as_ref().unwrap(), "known_hosts".into()))
        .await
        .unwrap();
    assert!(
        matches!(transport, engine_session::TransportSpec::Ssh { auth: engine_session::SshAuth::Password(ref value), .. } if value == "old-password")
    );
    within(secrets.import(ssh("new-password"))).await.unwrap();
    within(secrets.commit()).await;
    let mut restarted = RemoteSecrets::new(Some(store));
    let transport =
        within(restarted.transport(first.remote.as_ref().unwrap(), "known_hosts".into()))
            .await
            .unwrap();
    assert!(
        matches!(transport, engine_session::TransportSpec::Ssh { auth: engine_session::SshAuth::Password(ref value), .. } if value == "new-password")
    );
}

#[tokio::test]
async fn remote_fingerprints_cover_identity_and_actual_query_version() {
    let mut secrets = RemoteSecrets::default();
    let profile = within(secrets.import(ssh("password")))
        .await
        .unwrap()
        .with_remote_version("1.16.5");
    let expected = profile.fingerprint().await.unwrap();
    for variant in 0..6 {
        let mut changed = profile.clone();
        let Some(RemoteEngineConfig::Ssh {
            host,
            port,
            user,
            command,
            auth,
            version,
        }) = changed.remote.as_mut()
        else {
            unreachable!()
        };
        match variant {
            0 => *host = "localhost".into(),
            1 => *port += 1,
            2 => *user = "other".into(),
            3 => command.push_str(" -override-config numSearchThreads=4"),
            4 => *auth = SshAuthConfig::PrivateKey { path: "key".into() },
            _ => *version = Some("1.18.0".into()),
        };
        assert_ne!(changed.fingerprint().await.unwrap(), expected);
    }
    let changed_password = within(secrets.import(ssh("other-password")))
        .await
        .unwrap()
        .with_remote_version("1.16.5");
    assert_eq!(changed_password.fingerprint().await.unwrap(), expected);
    let first = within(secrets.import(link())).await.unwrap();
    let other = within(secrets.import(RemoteEngineInput::WebSocket {
        url: "ws://127.0.0.1:9876/other-path".into(),
    }))
    .await
    .unwrap();
    assert_ne!(
        first.fingerprint().await.unwrap(),
        other.fingerprint().await.unwrap()
    );
    assert_ne!(
        first.fingerprint().await.unwrap(),
        profile.fingerprint().await.unwrap()
    );
}

#[tokio::test]
async fn switching_to_local_skips_remote_and_missing_files_and_persists_first_usable_entry() {
    let binary = fake_katago::binary_path().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let (app, _) = crate::spawn(directory.path()).await.unwrap();
    assert!(matches!(
        within(app.switch_to_local_engine()).await,
        Err(AppError::Remote(RemoteEngineError::NoLocalEngine))
    ));
    within(app.add_remote_engine("remote".into(), link()))
        .await
        .unwrap();
    within(app.add_engine("missing".into(), EngineProfile::default()))
        .await
        .unwrap();
    let local = EngineProfile {
        program: binary.clone(),
        model: binary.clone(),
        config: binary,
        ..EngineProfile::default()
    };
    let settings = within(app.add_engine("local".into(), local.clone()))
        .await
        .unwrap();
    let expected = settings.engines[2].id.clone();
    let settings = within(app.switch_to_local_engine()).await.unwrap();
    assert_eq!(settings.main_engine_id.as_ref(), Some(&expected));
    assert_eq!(settings.engine, Some(local));
    let disk = Settings::load(directory.path()).await.unwrap();
    assert_eq!(disk.main_engine_id, Some(expected));
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn invalid_remote_edits_do_not_change_store_or_saved_settings() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::new(true);
    let (app, _) = crate::spawn_with_config(
        directory.path(),
        AppConfig {
            commentary_key_store: Some(store.clone()),
            ..AppConfig::default()
        },
    )
    .await
    .unwrap();
    let settings = within(app.add_remote_engine("ssh".into(), ssh("good-password")))
        .await
        .unwrap();
    within(app.settings()).await.unwrap();
    let original = store.snapshot().await;
    assert!(
        within(app.update_remote_engine(
            settings.engines[0].id.clone(),
            " ".into(),
            ssh("bad-password")
        ))
        .await
        .is_err()
    );
    assert!(
        within(app.update_remote_engine("missing-id".into(), "name".into(), ssh("bad-password")))
            .await
            .is_err()
    );
    assert_eq!(store.snapshot().await, original);
    assert_eq!(
        within(app.settings()).await.unwrap().engines,
        settings.engines
    );
    within(app.shutdown()).await.unwrap();
}
