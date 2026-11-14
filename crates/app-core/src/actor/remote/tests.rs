#![allow(clippy::unwrap_used, clippy::expect_used)]
use crate::test_support::within;
use crate::{EngineEntry, RemoteEngineInput, RemoteSshAuth, remote::Request};

#[tokio::test]
async fn failed_active_remote_edit_restores_previous_credentials_and_profile() {
    let directory = tempfile::tempdir().unwrap();
    let (mut actor, _) = super::super::tests::actor();
    actor.directory = directory.path().to_owned();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let input = |password: &str| RemoteEngineInput::Ssh {
        host: "127.0.0.1".into(),
        port,
        user: "test".into(),
        command: "katago analysis".into(),
        auth: RemoteSshAuth::Password {
            password: password.into(),
        },
    };
    let profile = within(actor.remote.import(input("original-password")))
        .await
        .unwrap()
        .with_remote_version("1.16.5");
    within(actor.remote.commit()).await;
    actor.settings.engines.push(EngineEntry {
        id: "remote".into(),
        name: "remote".into(),
        profile: profile.clone(),
    });
    actor.settings.main_engine_id = Some("remote".into());
    actor.settings.engine = Some(profile.clone());
    actor.settings.save(directory.path()).await.unwrap();
    let before = tokio::fs::read(directory.path().join("settings.json"))
        .await
        .unwrap();
    let (reply, response) = tokio::sync::oneshot::channel();
    within(actor.remote_command(Request::Update(
        "remote".into(),
        "new name".into(),
        input("incorrect-password"),
        reply,
    )))
    .await;
    assert!(actor.trial.is_some());
    // Cancellation is a real rejected trial outcome; no SSH service or external
    // endpoint is needed to test the application transaction around it.
    within(actor.finish_trial(Err(crate::EngineSetupError::Cancelled))).await;
    assert!(within(response).await.unwrap().is_err());
    assert_eq!(actor.settings.engine, Some(profile.clone()));
    assert_eq!(
        tokio::fs::read(directory.path().join("settings.json"))
            .await
            .unwrap(),
        before
    );
    let (transport, _) = within(actor.prepare_engine_transport(&profile))
        .await
        .unwrap();
    assert!(
        matches!(transport, engine_session::TransportSpec::Ssh { auth: engine_session::SshAuth::Password(ref password), .. } if password == "original-password")
    );
}
