#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::{AppConfig, AppSubscription, Settings, delivery};
use document::GameDocument;
use std::{collections::HashMap, future::Future, path::Path};
use tempfile::TempDir;

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("safety deadline")
}

fn actor(directory: &Path) -> (Actor, AppSubscription) {
    let (output, subscription) = delivery::channel();
    let mut config = AppConfig::default();
    config.analysis.live.max_visits = 1;
    config.analysis.live.report_interval = Duration::from_secs(60);
    config.session.write_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(60);
    (
        Actor {
            document: GameDocument::new(),
            tree: crate::VariationTreeView::default(),
            summaries: HashMap::new(),
            review: Default::default(),
            summary_fingerprint: None,
            revision: 0,
            directory: directory.to_owned(),
            settings: Settings::default(),
            config,
            running: None,
            trial: None,
            focus: None,
            output,
        },
        subscription,
    )
}

fn profile(directory: &Path, args: &[&str]) -> EngineSetupProfile {
    let model = directory.join("model.bin.gz");
    let config = directory.join("custom.cfg");
    std::fs::write(&model, "fake model").unwrap();
    std::fs::write(&config, "numSearchThreads = 1\n").unwrap();
    EngineSetupProfile {
        program: fake_katago::binary_path().unwrap(),
        model,
        config: Some(config),
        extra_args: args.iter().map(|arg| (*arg).to_owned()).collect(),
    }
}

async fn begin(
    actor: &mut Actor,
    profile: EngineSetupProfile,
) -> oneshot::Receiver<Result<(), AppError>> {
    let (reply, receiver) = oneshot::channel();
    within(actor.begin_trial(profile, PathBuf::from("unused-template"), reply)).await;
    receiver
}

async fn commit(actor: &mut Actor, profile: EngineSetupProfile) {
    let receiver = begin(actor, profile).await;
    let outcome = within(wait(&mut actor.trial)).await;
    within(actor.finish_trial(outcome)).await;
    within(receiver).await.unwrap().unwrap();
}

async fn ready(engine: &EngineHandle) {
    let mut status = engine.status();
    within(async {
        loop {
            if matches!(*status.borrow_and_update(), EngineStatus::Ready { .. }) {
                break;
            }
            status.changed().await.unwrap();
        }
    })
    .await;
}

async fn final_analysis(actor: &mut Actor, output: &mut AppSubscription) {
    within(async {
        loop {
            tokio::select! {
                signal = super::super::next_signal(&mut actor.running) => actor.signal(signal),
                event = output.recv() => match event.unwrap() {
                    AppEvent::AnalysisUpdated(value) if !value.is_during_search => return,
                    AppEvent::EngineStatusChanged(crate::EngineStatus::Failed { reason, .. }) => panic!("{reason}"),
                    _ => {}
                }
            }
        }
    }).await;
}

#[tokio::test]
async fn ready_trial_is_persisted_and_the_same_session_is_adopted() {
    let directory = TempDir::new().unwrap();
    let profile = profile(
        directory.path(),
        &[
            "-override-config",
            "numSearchThreads=2",
            "-override-config=reportAnalysisWinratesAs=BLACK",
        ],
    );
    let (mut actor, mut output) = actor(directory.path());
    let receiver = begin(&mut actor, profile.clone()).await;
    let outcome = within(wait(&mut actor.trial)).await;
    let candidate = actor.trial.as_ref().unwrap().engine.clone();
    within(actor.finish_trial(outcome)).await;
    within(receiver).await.unwrap().unwrap();
    assert_eq!(
        Settings::load(directory.path()).await.unwrap(),
        actor.settings
    );
    assert_eq!(
        actor.settings.engine.as_ref().unwrap().extra_args,
        profile.extra_args
    );
    final_analysis(&mut actor, &mut output).await;
    // Stopping the trial handle stops the adopted session too: success must
    // never start a second, untested process after saving.
    within(candidate.shutdown()).await.unwrap();
    assert_eq!(
        *actor.running.as_ref().unwrap().engine_status.borrow(),
        EngineStatus::Stopped
    );
    within(actor.stop_engine()).await.unwrap();
}

#[tokio::test]
async fn missing_model_and_startup_failure_preserve_settings_and_active_session() {
    let directory = TempDir::new().unwrap();
    let good = profile(directory.path(), &[]);
    let (mut actor, mut output) = actor(directory.path());
    commit(&mut actor, good.clone()).await;
    final_analysis(&mut actor, &mut output).await;
    let saved = std::fs::read(directory.path().join("settings.json")).unwrap();
    let settings = actor.settings.clone();
    let old = actor.running.as_ref().unwrap().engine.clone();
    let mut missing = good.clone();
    missing.model = directory.path().join("missing-model.bin.gz");
    let receiver = begin(&mut actor, missing).await;
    let error = within(receiver).await.unwrap().unwrap_err().to_string();
    assert!(
        error.contains("engine model") && error.contains("missing-model.bin.gz"),
        "{error}"
    );
    let mut failed = good;
    failed.extra_args = vec!["--fail-startup".into()];
    let receiver = begin(&mut actor, failed).await;
    let outcome = within(wait(&mut actor.trial)).await;
    within(actor.finish_trial(outcome)).await;
    let error = within(receiver).await.unwrap().unwrap_err().to_string();
    assert!(error.contains("模型找不到"), "{error}");
    assert_eq!(actor.settings, settings);
    assert_eq!(
        std::fs::read(directory.path().join("settings.json")).unwrap(),
        saved
    );
    assert!(matches!(*old.status().borrow(), EngineStatus::Ready { .. }));
    within(
        actor.apply(super::super::Input::Edit(document::Command::Play(
            go_core::Move::Pass(go_core::Color::Black),
        ))),
    )
    .await
    .unwrap();
    final_analysis(&mut actor, &mut output).await;
    within(old.shutdown()).await.unwrap();
    assert_eq!(
        *actor.running.as_ref().unwrap().engine_status.borrow(),
        EngineStatus::Stopped
    );
    within(actor.stop_engine()).await.unwrap();
}

#[tokio::test]
async fn lost_readiness_and_cancelled_caller_cannot_commit() {
    let directory = TempDir::new().unwrap();
    let good = profile(directory.path(), &[]);
    let (mut actor, _output) = actor(directory.path());
    commit(&mut actor, good.clone()).await;
    let saved = std::fs::read(directory.path().join("settings.json")).unwrap();
    for cancel in [false, true] {
        let receiver = begin(&mut actor, good.clone()).await;
        within(wait(&mut actor.trial)).await.unwrap();
        let candidate = actor.trial.as_ref().unwrap().engine.clone();
        if cancel {
            drop(receiver);
        } else {
            within(candidate.clone().shutdown()).await.unwrap();
            within(actor.finish_trial(Ok(()))).await;
            assert!(within(receiver).await.unwrap().is_err());
        }
        if cancel {
            within(actor.finish_trial(Ok(()))).await;
        }
        assert_eq!(*candidate.status().borrow(), EngineStatus::Stopped);
        assert_eq!(
            std::fs::read(directory.path().join("settings.json")).unwrap(),
            saved
        );
        assert!(matches!(
            *actor.running.as_ref().unwrap().engine_status.borrow(),
            EngineStatus::Ready { .. }
        ));
    }
    within(actor.stop_engine()).await.unwrap();
}

#[tokio::test]
async fn timeout_is_driven_by_injected_hang_and_reaps_only_candidate() {
    let directory = TempDir::new().unwrap();
    let good = profile(directory.path(), &[]);
    let (mut actor, mut output) = actor(directory.path());
    commit(&mut actor, good.clone()).await;
    final_analysis(&mut actor, &mut output).await;
    let saved = std::fs::read(directory.path().join("settings.json")).unwrap();
    let marker = directory.path().join("hung");
    let mut bad = good;
    bad.extra_args = vec![
        "--hang-after-queries".into(),
        "0".into(),
        "--fault-once-file".into(),
        marker.to_str().unwrap().into(),
    ];
    // The running session already has long normal deadlines. Only this
    // deliberately hung candidate needs a short forced-cleanup deadline.
    actor.config.session.shutdown_timeout = Duration::from_millis(20);
    actor.config.engine_setup_timeout = Duration::from_secs(10);
    let receiver = begin(&mut actor, bad).await;
    let candidate = actor.trial.as_ref().unwrap().engine.clone();
    within(async {
        while !marker.is_file() {
            tokio::task::yield_now().await;
        }
    })
    .await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(11)).await;
    tokio::time::resume();
    let outcome = within(wait(&mut actor.trial)).await;
    assert!(matches!(outcome, Err(EngineSetupError::TimedOut(_))));
    within(actor.finish_trial(outcome)).await;
    assert!(
        within(receiver)
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("timed out")
    );
    assert_eq!(*candidate.status().borrow(), EngineStatus::Stopped);
    assert_eq!(
        std::fs::read(directory.path().join("settings.json")).unwrap(),
        saved
    );
    assert!(matches!(
        *actor.running.as_ref().unwrap().engine_status.borrow(),
        EngineStatus::Ready { .. }
    ));
    within(actor.stop_engine()).await.unwrap();
}

#[tokio::test]
async fn settings_write_failure_leaves_active_engine_and_profile_unchanged() {
    let directory = TempDir::new().unwrap();
    let good = profile(directory.path(), &[]);
    let (mut actor, mut output) = actor(directory.path());
    commit(&mut actor, good.clone()).await;
    final_analysis(&mut actor, &mut output).await;
    let settings = actor.settings.clone();
    // A regular file cannot contain settings.json, on every supported OS.
    actor.directory = directory.path().join("blocked-directory");
    std::fs::write(&actor.directory, "block").unwrap();
    let receiver = begin(&mut actor, good).await;
    let outcome = within(wait(&mut actor.trial)).await;
    within(actor.finish_trial(outcome)).await;
    assert!(within(receiver).await.unwrap().is_err());
    assert_eq!(actor.settings, settings);
    ready(&actor.running.as_ref().unwrap().engine).await;
    within(actor.stop_engine()).await.unwrap();
}
