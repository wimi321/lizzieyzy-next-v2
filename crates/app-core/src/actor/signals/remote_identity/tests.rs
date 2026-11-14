#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{
    EngineProfile, RemoteEngineConfig,
    actor::{
        Signal,
        tests::{actor, snapshot},
    },
};

use crate::test_support::within;

#[tokio::test]
async fn changed_remote_version_replaces_cache_namespace_before_accepting_a_snapshot() {
    let program = fake_katago::binary_path().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let (mut actor, _events) = actor();
    actor.directory = dir.path().to_owned();
    actor.settings.pondering = false;
    actor.settings.engine = Some(EngineProfile {
        program: program.clone(),
        model: program.clone(),
        config: program,
        ..Default::default()
    });
    within(actor.start_engine()).await;
    let mut status = actor.running.as_ref().unwrap().engine.status();
    let version = within(async {
        loop {
            if let engine_session::EngineStatus::Ready { version } =
                status.borrow_and_update().clone()
            {
                break version;
            }
            status.changed().await.unwrap();
        }
    })
    .await;
    // Keep a real fake-katago session but emulate an older remote identity in
    // app state. The Ready watch is the same boundary used after reconnection.
    let remote = EngineProfile {
        remote: Some(RemoteEngineConfig::WebSocket {
            host: "127.0.0.1".into(),
            port: 9,
            secure: false,
            identity: "a".repeat(64),
            version: Some("older-version".into()),
        }),
        ..Default::default()
    };
    actor.settings.set_legacy_engine(Some(remote.clone()));
    let old_fingerprint = remote.fingerprint().await.unwrap();
    actor.running.as_mut().unwrap().fingerprint = old_fingerprint.clone();
    actor.set_summary_fingerprint(Some(old_fingerprint.clone()));
    let mut retained = snapshot(&actor, 100, 0.8);
    retained.source = analysis::SnapshotSource::Cache;
    actor.signal(Signal::Snapshot(Box::new(retained.clone())));
    assert!(!actor.review.results.is_empty());
    let (_status_tx, restarting) =
        tokio::sync::watch::channel(engine_session::EngineStatus::Restarting {
            attempt: 1,
            reason: "test disconnection".into(),
        });
    let current = std::mem::replace(
        &mut actor.running.as_mut().unwrap().engine_status,
        restarting,
    );
    assert!(within(actor.refresh_remote_identity()).await);
    assert_eq!(actor.running.as_ref().unwrap().fingerprint, old_fingerprint);
    assert!(!actor.review.results.is_empty());
    actor.running.as_mut().unwrap().engine_status = current;
    // A stale but otherwise valid result races ahead of the Engine signal.
    // It must not repopulate the new namespace after the version switch.
    within(actor.cache_signal(Signal::Snapshot(Box::new(retained)))).await;
    let expected = remote
        .with_remote_version(&version)
        .fingerprint()
        .await
        .unwrap();
    assert_ne!(expected, old_fingerprint);
    assert_eq!(actor.running.as_ref().unwrap().fingerprint, expected);
    assert_eq!(
        actor.settings.engine.as_ref().unwrap(),
        &remote.with_remote_version(&version)
    );
    assert!(actor.review.results.is_empty());
    assert!(actor.summaries.is_empty());
    assert!(actor.remote_identity_current());
    assert!(within(actor.refresh_remote_identity()).await);
    within(actor.stop_engine()).await.unwrap();
}
