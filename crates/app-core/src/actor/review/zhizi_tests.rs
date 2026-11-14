#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use crate::actor::{Input, Running};
use crate::{Settings, WholeGameOptions, ZhiziEngineOptions, test_support::within};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};

fn fake() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            let output = std::process::Command::new(
                std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()),
            )
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
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .find_map(|line| {
                    let value: serde_json::Value = serde_json::from_str(line).ok()?;
                    (value["reason"] == "compiler-artifact"
                        && value["target"]["name"] == "fake-gtp")
                        .then(|| value["executable"].as_str().map(PathBuf::from))
                        .flatten()
                })
                .unwrap()
        })
        .as_path()
}
async fn actor_with_live_bridge() -> (Actor, tempfile::TempDir, PathBuf) {
    let binary = fake().to_owned();
    let directory = tempfile::tempdir().unwrap();
    let log = directory.path().join("gtp.log");
    let (mut actor, _) = crate::actor::tests::actor();
    actor.directory = directory.path().to_owned();
    actor.settings.pondering = true;
    actor.settings.auto_quick_analyze_on_load = false;
    actor.settings.cache_enabled = false;
    within(actor.open(b"(;SZ[9];B[aa];W[bb];B[cc])".to_vec(), None))
        .await
        .unwrap();
    let engine = engine_session::spawn(
        engine_session::TransportSpec::GtpBridge {
            inner: Box::new(engine_session::TransportSpec::LocalProcess {
                program: binary,
                args: vec![
                    "--kata".into(),
                    "--command-log".into(),
                    log.as_os_str().to_owned(),
                ],
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
    actor.settings.save(directory.path()).await.unwrap();
    let (coordinator, snapshots) = actor.coordinator(&engine, &fingerprint).unwrap();
    actor.running = Some(Running {
        engine_status: status,
        analysis_status: coordinator.status(),
        engine,
        coordinator,
        snapshots,
        fingerprint,
    });
    within(actor.update_focus()).await;
    live(&mut actor).await;
    (actor, directory, log)
}
async fn live(actor: &mut Actor) {
    let snapshot = within(actor.running.as_mut().unwrap().snapshots.recv())
        .await
        .unwrap();
    assert!(snapshot.response.root_info.visits > 0);
}
async fn finish(actor: &mut Actor) {
    while actor.review_running() {
        let result = within(receive(&mut actor.review)).await;
        within(actor.review_signal(result)).await;
    }
    assert_eq!(
        actor.review.progress.as_ref().unwrap().status,
        WholeGameStatus::Completed
    );
}
fn options(visits: u64) -> ReviewInput {
    ReviewInput::Start(WholeGameOptions {
        visits,
        force: true,
        ..Default::default()
    })
}

#[tokio::test]
async fn default_live_search_yields_to_review_navigation_does_not_cancel_and_live_resumes() {
    let (mut actor, directory, log) = actor_with_live_bridge().await;
    within(actor.review_command(options(300))).await.unwrap();
    assert!(actor.settings.pondering);
    assert!(!actor.pondering());
    assert_eq!(actor.analysis_state().pause_reason, None);
    // Move the UI focus during real GTP work; it must neither submit an
    // unbounded live search nor repeatedly terminate/restart the review.
    for delta in [1, 1, -1, 1] {
        within(actor.apply(Input::Edit(document::Command::Step(delta))))
            .await
            .unwrap();
    }
    finish(&mut actor).await;
    assert_eq!(actor.review.progress.as_ref().unwrap().completed, 4);
    assert!(actor.pondering());
    live(&mut actor).await;
    assert!(Settings::load(directory.path()).await.unwrap().pondering);
    within(actor.stop_engine()).await.unwrap();
    let log = std::fs::read_to_string(log).unwrap();
    // One initial live search, four review turns, one resumed live search.
    assert_eq!(log.matches("kata-analyze").count(), 6, "{log}");
}

#[tokio::test]
async fn pause_cancel_and_user_pondering_changes_preserve_intent_without_blocking_review() {
    let (mut actor, directory, _) = actor_with_live_bridge().await;
    within(actor.review_command(options(1_000_000)))
        .await
        .unwrap();
    within(actor.pause_review()).await.unwrap();
    assert!(actor.pondering());
    live(&mut actor).await;
    within(actor.resume_review()).await.unwrap();
    assert!(!actor.pondering());
    within(actor.cancel_review()).await;
    assert!(actor.pondering());
    live(&mut actor).await;
    within(actor.review_command(options(200))).await.unwrap();
    within(actor.set_pondering(false)).await.unwrap();
    within(actor.set_pondering(true)).await.unwrap(); // must not restart live during the review
    assert!(!actor.pondering());
    finish(&mut actor).await;
    assert!(actor.pondering());
    live(&mut actor).await;
    within(actor.review_command(options(200))).await.unwrap();
    within(actor.set_pondering(false)).await.unwrap();
    finish(&mut actor).await;
    assert!(!actor.pondering());
    assert!(!Settings::load(directory.path()).await.unwrap().pondering);
    within(actor.stop_engine()).await.unwrap();
}

#[tokio::test]
async fn batch_uses_the_same_single_slot_review_with_saved_live_intent() {
    let (mut actor, directory, _) = actor_with_live_bridge().await;
    let file = directory.path().join("input.sgf");
    tokio::fs::write(&file, b"(;SZ[9];B[aa];W[bb])")
        .await
        .unwrap();
    within(
        actor.batch_command(crate::batch::Request::Start(crate::BatchOptions {
            files: vec![file.clone(), file.clone()],
            visits: 200,
            scope: WholeGameScope::Line,
            force: true,
            output: crate::BatchOutput {
                suffix: "review".into(),
                directory: None,
            },
        })),
    )
    .await
    .unwrap();
    for _ in 0..2 {
        within(actor.advance_batch()).await;
        assert!(!actor.pondering());
        assert!(actor.settings.pondering);
        finish(&mut actor).await;
        within(actor.sync_batch_progress()).await;
    }
    within(actor.advance_batch()).await;
    assert_eq!(actor.batch.status.phase, crate::BatchPhase::Completed);
    for item in &actor.batch.status.items {
        let crate::BatchItemState::Completed { output } = &item.state else {
            panic!("{item:?}");
        };
        assert!(
            tokio::fs::read_to_string(output)
                .await
                .unwrap()
                .contains("LZ[")
        );
    }
    assert_eq!(
        tokio::fs::read(&file).await.unwrap(),
        b"(;SZ[9];B[aa];W[bb])"
    );
    assert!(actor.pondering());
    within(actor.stop_engine()).await.unwrap();
}
