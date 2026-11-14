#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::{
    AppCore, EngineEntry, Settings,
    actor::{Command, trial},
    test_support::within,
    zhizi::{ZhiziClient, ZhiziService},
};
use engine_session::EngineHandle;
use std::{path::Path, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

fn local_profile(binary: &Path) -> EngineProfile {
    EngineProfile {
        program: binary.into(),
        model: binary.into(),
        config: binary.into(),
        ..Default::default()
    }
}

async fn attach(actor: &mut Actor, binary: &Path) -> EngineHandle {
    let profile = local_profile(binary);
    let engine = engine_session::spawn(
        profile.transport().unwrap(),
        engine_session::SessionConfig {
            startup_timeout: Duration::from_secs(60),
            response_timeout: Duration::from_secs(60),
            ..Default::default()
        },
    );
    let mut status = engine.status();
    within(async {
        loop {
            match status.borrow_and_update().clone() {
                engine_session::EngineStatus::Ready { .. } => break,
                engine_session::EngineStatus::Failed { reason, .. } => panic!("{reason}"),
                _ => {}
            }
            status.changed().await.unwrap();
        }
    })
    .await;
    let fingerprint = profile.fingerprint().await.unwrap();
    let (coordinator, snapshots) = actor.coordinator(&engine, &fingerprint).unwrap();
    actor.running = Some(Running {
        engine: engine.clone(),
        engine_status: status,
        analysis_status: coordinator.status(),
        coordinator,
        snapshots,
        fingerprint,
    });
    engine
}

#[tokio::test]
async fn changed_credentials_stop_cloud_session_without_persisting_engine_or_pondering_changes() {
    let binary = fake_katago::binary_path().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let (mut actor, _) = crate::actor::tests::actor();
    actor.directory = directory.path().into();
    actor.settings = Settings {
        engine: Some(ZhiziEngineOptions::default().profile()),
        pondering: true,
        ..Default::default()
    };
    actor.settings.save(directory.path()).await.unwrap();
    let original = tokio::fs::read(directory.path().join("settings.json"))
        .await
        .unwrap();
    let engine = attach(&mut actor, &binary).await;
    within(actor.zhizi_credentials_changed()).await.unwrap();
    assert!(actor.running.is_none());
    assert_eq!(
        *engine.status().borrow(),
        engine_session::EngineStatus::Stopped
    );
    assert!(actor.settings.pondering);
    assert!(matches!(
        actor.settings.engine.as_ref().unwrap().remote,
        Some(RemoteEngineConfig::Zhizi { .. })
    ));
    assert_eq!(
        tokio::fs::read(directory.path().join("settings.json"))
            .await
            .unwrap(),
        original
    );
}

#[tokio::test]
async fn changed_credentials_preserve_local_running_sessions_and_temporary_startup_fallback() {
    let binary = fake_katago::binary_path().unwrap();
    for fallback in [false, true] {
        let (mut actor, _) = crate::actor::tests::actor();
        actor.settings.engine = Some(if fallback {
            ZhiziEngineOptions::default().profile()
        } else {
            local_profile(&binary)
        });
        actor.startup_local_fallback = fallback;
        let engine = attach(&mut actor, &binary).await;
        within(actor.zhizi_credentials_changed()).await.unwrap();
        assert!(actor.running.is_some());
        assert!(matches!(
            *engine.status().borrow(),
            engine_session::EngineStatus::Ready { .. }
        ));
        within(actor.stop_engine()).await.unwrap();
    }
}

struct Http {
    client: ZhiziClient,
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
}
impl Http {
    async fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client =
            ZhiziClient::with_base_url(&format!("http://{}", listener.local_addr().unwrap()))
                .unwrap();
        let (stop, mut stopping) = oneshot::channel();
        let task = tokio::spawn(async move {
            loop {
                let (mut stream, _) = tokio::select! { _ = &mut stopping => break, result = listener.accept() => result.unwrap() };
                let mut request = Vec::new();
                loop {
                    let mut buffer = [0; 4096];
                    let read = stream.read(&mut buffer).await.unwrap();
                    if read == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..read]);
                    if let Some(end) = request.windows(4).position(|v| v == b"\r\n\r\n") {
                        let length = String::from_utf8_lossy(&request[..end])
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        if request.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                let body = r#"{"token":"local-test-token"}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });
        Self { client, stop, task }
    }
    async fn close(self) {
        let _ = self.stop.send(());
        within(self.task).await.unwrap();
    }
}

#[tokio::test]
async fn changed_credentials_cancel_only_pending_zhizi_trial_and_preserve_local_engine() {
    let binary = fake_katago::binary_path().unwrap();
    let http = Http::new().await;
    let service = ZhiziService::spawn(Some(http.client.clone()), None);
    within(service.login_password("account@example.test".into(), "password".into(), false))
        .await
        .unwrap();
    let (mut actor, _) = crate::actor::tests::actor();
    actor.zhizi = service;
    actor.settings.engine = Some(local_profile(&binary));
    let engine = attach(&mut actor, &binary).await;
    for zhizi in [false, true] {
        let profile = if zhizi {
            ZhiziEngineOptions::default().profile()
        } else {
            local_profile(&binary)
        };
        let entry = EngineEntry {
            id: "candidate".into(),
            name: "Candidate".into(),
            profile: profile.clone(),
        };
        let (reply, result) = oneshot::channel();
        within(actor.begin_list_trial(
            vec![entry],
            "candidate".into(),
            profile,
            trial::Reply::Settings(reply),
        ))
        .await;
        assert!(actor.trial.is_some());
        within(actor.zhizi_credentials_changed()).await.unwrap();
        if zhizi {
            assert!(actor.trial.is_none());
            assert!(matches!(
                within(result).await.unwrap(),
                Err(AppError::EngineSetup(crate::EngineSetupError::Cancelled))
            ));
        } else {
            assert!(actor.trial.is_some());
            within(actor.cancel_trial()).await;
        }
        assert!(actor.running.is_some());
        assert!(matches!(
            *engine.status().borrow(),
            engine_session::EngineStatus::Ready { .. }
        ));
    }
    within(actor.stop_engine()).await.unwrap();
    http.close().await;
}

#[tokio::test]
async fn public_login_code_reset_and_logout_wait_for_session_cleanup_acknowledgement() {
    let http = Http::new().await;
    let service = ZhiziService::spawn(Some(http.client.clone()), None);
    let (commands, mut receiver) = mpsc::channel(8);
    let app = AppCore {
        commands,
        runtime_platform: false,
    };
    let (cleaned, mut cleanups) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        while let Some(command) = receiver.recv().await {
            match command {
                Command::ZhiziService(reply) => {
                    let _ = reply.send(service.clone());
                }
                Command::ZhiziCredentialsChanged(reply) => {
                    cleaned.send((reply, service.status().logged_in)).unwrap();
                }
                _ => panic!("unexpected command"),
            }
        }
    });
    for operation in 0..4 {
        let request = async {
            match operation {
                0 => {
                    app.zhizi_login_password(
                        "account@example.test".into(),
                        "password".into(),
                        false,
                    )
                    .await
                }
                1 => {
                    app.zhizi_login_code("account@example.test".into(), "123456".into(), false)
                        .await
                }
                2 => {
                    app.zhizi_reset_password(
                        "account@example.test".into(),
                        "123456".into(),
                        "new-password".into(),
                        false,
                    )
                    .await
                }
                _ => app.zhizi_logout().await,
            }
        };
        tokio::pin!(request);
        let (reply, logged_in) = within(async {
            tokio::select! {
                biased;
                result = &mut request => panic!("returned before cleanup acknowledgement: {result:?}"),
                cleanup = cleanups.recv() => cleanup.unwrap(),
            }
        }).await;
        assert_eq!(logged_in, operation != 3);
        reply.send(Ok(())).unwrap();
        within(request).await.unwrap();
    }
    drop(app);
    within(task).await.unwrap();
    http.close().await;
}
