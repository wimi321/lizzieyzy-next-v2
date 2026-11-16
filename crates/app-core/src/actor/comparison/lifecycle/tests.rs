#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::{AppEvent, AppSubscription, EngineProfile, SettingsError, engines::Request};
use std::{future::Future, path::PathBuf, sync::OnceLock, time::Duration};
use tokio::sync::oneshot;

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("safety deadline")
}

fn setup() -> (tempfile::TempDir, Actor, AppSubscription, EngineProfile) {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    let binary = BINARY
        .get_or_init(|| fake_katago::binary_path().unwrap())
        .clone();
    let directory = tempfile::tempdir().unwrap();
    let (mut actor, output) = crate::actor::tests::actor();
    actor.directory = directory.path().to_owned();
    actor.config.analysis.live.max_visits = 1;
    actor.config.analysis.live.report_interval = Duration::from_millis(50);
    actor.config.session.response_timeout = Duration::from_secs(60);
    actor.config.session.write_timeout = Duration::from_secs(60);
    actor.config.session.shutdown_timeout = Duration::from_millis(100);
    actor.config.session.max_restarts = 0;
    actor.settings.auto_quick_analyze_on_load = false;
    let profile = EngineProfile {
        program: binary.clone(),
        model: binary.clone(),
        config: binary,
        extra_args: vec!["--visits-per-sec".into(), "100000".into()],
        ..Default::default()
    };
    (directory, actor, output, profile)
}

async fn finish_trials(actor: &mut Actor) {
    within(async {
        while actor.trial.is_some() || actor.comparison.trial.is_some() {
            tokio::select! {
                signal = trial::wait(&mut actor.trial) => actor.trial_signal(signal).await,
                signal = trial::wait(&mut actor.comparison.trial) => actor.comparison_trial_signal(signal).await,
            }
        }
    }).await;
}

async fn command(actor: &mut Actor, request: Request) -> Result<Settings, AppError> {
    let (reply, response) = oneshot::channel();
    within(actor.engine_command(request, reply)).await;
    finish_trials(actor).await;
    within(response).await.unwrap()
}

async fn layout(actor: &mut Actor, mode: LayoutMode) {
    let mut display = actor.settings.display.clone();
    display.layout_mode = mode;
    within(actor.set_display_settings(display)).await.unwrap();
    finish_trials(actor).await;
}

async fn add(actor: &mut Actor, name: &str, profile: EngineProfile) -> String {
    command(actor, Request::Add(name.into(), profile))
        .await
        .unwrap()
        .engines
        .last()
        .unwrap()
        .id
        .clone()
}

async fn analysis(actor: &mut Actor, output: &mut AppSubscription, comparison: bool) {
    within(async {
        loop {
            tokio::select! {
                signal = crate::actor::next_signal(&mut actor.running) => actor.cache_signal(signal).await,
                signal = crate::actor::next_signal(&mut actor.comparison.running) => actor.comparison_signal(signal).await,
                event = output.recv() => match event.unwrap() {
                    AppEvent::AnalysisUpdated(value) if !comparison && !value.is_during_search => return,
                    AppEvent::ComparisonAnalysisUpdated(value) if comparison && !value.is_during_search => return,
                    _ => {},
                }
            }
        }
    }).await;
}

async fn cleanup(actor: &mut Actor) {
    within(actor.stop_comparison()).await.unwrap();
    within(actor.stop_engine()).await.unwrap();
}

#[tokio::test]
async fn selection_is_typed_persisted_and_layout_owns_process_lifetime() {
    let (directory, mut actor, mut output, profile) = setup();
    let main = add(&mut actor, "Main", profile.clone()).await;
    let comparison = add(&mut actor, "Comparison", profile).await;
    command(&mut actor, Request::Select(main.clone()))
        .await
        .unwrap();
    assert!(matches!(
        command(&mut actor, Request::SelectComparison(Some(main.clone()))).await,
        Err(AppError::Settings(SettingsError::SameAsMain))
    ));
    let selected = command(
        &mut actor,
        Request::SelectComparison(Some(comparison.clone())),
    )
    .await
    .unwrap();
    assert!(actor.comparison.running.is_none());
    assert_eq!(selected.comparison_engine_id, Some(comparison.clone()));
    assert_eq!(
        within(Settings::load(directory.path())).await.unwrap(),
        selected
    );
    assert!(matches!(
        command(&mut actor, Request::Select(comparison.clone())).await,
        Err(AppError::Settings(SettingsError::SameAsComparison))
    ));
    assert!(
        command(
            &mut actor,
            Request::SelectComparison(Some("missing".into()))
        )
        .await
        .is_err()
    );
    layout(&mut actor, LayoutMode::DoubleEngine).await;
    analysis(&mut actor, &mut output, true).await;
    let old = actor.comparison.running.as_ref().unwrap().engine.clone();
    let original_main = actor.running.as_ref().unwrap().engine.clone();
    layout(&mut actor, LayoutMode::Normal).await;
    assert_eq!(
        *old.status().borrow(),
        engine_session::EngineStatus::Stopped
    );
    assert!(actor.comparison.running.is_none());
    assert_eq!(
        actor.settings.comparison_engine_id,
        Some(comparison.clone())
    );
    assert!(matches!(
        *original_main.status().borrow(),
        engine_session::EngineStatus::Ready { .. }
    ));
    layout(&mut actor, LayoutMode::DoubleEngine).await;
    analysis(&mut actor, &mut output, true).await;
    let config = actor.config.clone();
    cleanup(&mut actor).await;
    let (app, mut events) = within(crate::spawn_with_config(
        directory.path().to_owned(),
        config,
    ))
    .await
    .unwrap();
    within(async {
        loop {
            if let Some(AppEvent::ComparisonAnalysisUpdated(_)) = events.recv().await {
                break;
            }
        }
    })
    .await;
    assert_eq!(
        within(app.settings()).await.unwrap().comparison_engine_id,
        Some(comparison)
    );
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn comparison_switch_and_edit_only_commit_ready_candidates() {
    let (directory, mut actor, mut output, profile) = setup();
    let main = add(&mut actor, "Main", profile.clone()).await;
    let one = add(&mut actor, "One", profile.clone()).await;
    let mut other = profile.clone();
    other.extra_args.extend(["--time-scale".into(), "2".into()]);
    let two = add(&mut actor, "Two", other.clone()).await;
    let mut broken = profile;
    broken.extra_args.push("--fail-startup".into());
    let bad = add(&mut actor, "Broken", broken.clone()).await;
    command(&mut actor, Request::Select(main)).await.unwrap();
    layout(&mut actor, LayoutMode::DoubleEngine).await;
    command(&mut actor, Request::SelectComparison(Some(one.clone())))
        .await
        .unwrap();
    analysis(&mut actor, &mut output, true).await;
    let original = actor.comparison.running.as_ref().unwrap().engine.clone();
    let settings = actor.settings.clone();
    let saved = within(tokio::fs::read(directory.path().join("settings.json")))
        .await
        .unwrap();
    for request in [
        Request::SelectComparison(Some(bad)),
        Request::Update(one.clone(), "Broken edit".into(), broken),
    ] {
        assert!(
            command(&mut actor, request)
                .await
                .unwrap_err()
                .to_string()
                .contains("模型找不到")
        );
        assert_eq!(actor.settings, settings);
        assert_eq!(
            within(tokio::fs::read(directory.path().join("settings.json")))
                .await
                .unwrap(),
            saved
        );
        assert!(matches!(
            *original.status().borrow(),
            engine_session::EngineStatus::Ready { .. }
        ));
        assert!(!matches!(
            actor.comparison.status,
            EngineStatus::Failed { .. }
        ));
    }
    command(&mut actor, Request::SelectComparison(Some(two.clone())))
        .await
        .unwrap();
    assert_eq!(
        *original.status().borrow(),
        engine_session::EngineStatus::Stopped
    );
    analysis(&mut actor, &mut output, true).await;
    let before_edit = actor.comparison.running.as_ref().unwrap().engine.clone();
    other.extra_args.extend(["--threads".into(), "1".into()]);
    command(
        &mut actor,
        Request::Update(two.clone(), "Changed".into(), other.clone()),
    )
    .await
    .unwrap();
    assert_eq!(
        *before_edit.status().borrow(),
        engine_session::EngineStatus::Stopped
    );
    assert_eq!(
        actor
            .settings
            .engines
            .iter()
            .find(|entry| entry.id == two)
            .unwrap()
            .profile,
        other
    );
    analysis(&mut actor, &mut output, true).await;
    let before_delete = actor.comparison.running.as_ref().unwrap().engine.clone();
    command(&mut actor, Request::Remove(two)).await.unwrap();
    assert!(actor.settings.comparison_engine_id.is_none());
    assert_eq!(
        *before_delete.status().borrow(),
        engine_session::EngineStatus::Stopped
    );
    assert!(actor.running.is_some());
    cleanup(&mut actor).await;
}

#[tokio::test]
async fn failed_start_does_not_retry_until_requested_and_main_is_unaffected() {
    let (_directory, mut actor, mut output, profile) = setup();
    let main = add(&mut actor, "Main", profile.clone()).await;
    let mut broken = profile.clone();
    broken.extra_args.push("--fail-startup".into());
    let comparison = add(&mut actor, "Broken", broken).await;
    command(&mut actor, Request::Select(main)).await.unwrap();
    command(
        &mut actor,
        Request::SelectComparison(Some(comparison.clone())),
    )
    .await
    .unwrap();
    layout(&mut actor, LayoutMode::DoubleEngine).await;
    assert!(matches!(
        actor.comparison.status,
        EngineStatus::Failed { .. }
    ));
    assert!(actor.comparison.running.is_none());
    within(actor.reconcile_comparison()).await;
    assert!(actor.comparison.trial.is_none());
    within(
        actor.apply(crate::actor::Input::Edit(document::Command::Play(
            go_core::Move::Pass(go_core::Color::Black),
        ))),
    )
    .await
    .unwrap();
    analysis(&mut actor, &mut output, false).await;
    command(
        &mut actor,
        Request::Update(comparison, "Repaired".into(), profile),
    )
    .await
    .unwrap();
    command(&mut actor, Request::RestartComparison)
        .await
        .unwrap();
    analysis(&mut actor, &mut output, true).await;
    cleanup(&mut actor).await;
}

#[tokio::test]
async fn comparison_crash_is_isolated_and_explicit_restart_recovers() {
    let (directory, mut actor, mut output, profile) = setup();
    let main = add(&mut actor, "Main", profile.clone()).await;
    let mut crashing = profile;
    crashing.extra_args.extend([
        "--crash-after-queries".into(),
        "1".into(),
        "--fault-once-file".into(),
        directory
            .path()
            .join("comparison-crashed")
            .to_string_lossy()
            .into_owned(),
    ]);
    let comparison = add(&mut actor, "Crashing", crashing).await;
    command(&mut actor, Request::Select(main)).await.unwrap();
    layout(&mut actor, LayoutMode::DoubleEngine).await;
    command(&mut actor, Request::SelectComparison(Some(comparison)))
        .await
        .unwrap();
    within(async {
        loop {
            if matches!(actor.comparison.status, EngineStatus::Failed { .. }) {
                break;
            }
            let signal = crate::actor::next_signal(&mut actor.comparison.running).await;
            actor.comparison_signal(signal).await;
        }
    })
    .await;
    assert!(matches!(
        *actor.running.as_ref().unwrap().engine.status().borrow(),
        engine_session::EngineStatus::Ready { .. }
    ));
    within(
        actor.apply(crate::actor::Input::Edit(document::Command::Play(
            go_core::Move::Pass(go_core::Color::Black),
        ))),
    )
    .await
    .unwrap();
    analysis(&mut actor, &mut output, false).await;
    command(&mut actor, Request::RestartComparison)
        .await
        .unwrap();
    analysis(&mut actor, &mut output, true).await;
    cleanup(&mut actor).await;
}

#[tokio::test]
async fn close_comparison_others_all_and_main_only_have_distinct_semantics() {
    let (_directory, mut actor, mut output, profile) = setup();
    let main = add(&mut actor, "Main", profile.clone()).await;
    let comparison = add(&mut actor, "Comparison", profile).await;
    command(&mut actor, Request::Select(main.clone()))
        .await
        .unwrap();
    layout(&mut actor, LayoutMode::DoubleEngine).await;
    for close in [
        Request::CloseComparison,
        Request::CloseOthers,
        Request::SelectComparison(None),
    ] {
        command(
            &mut actor,
            Request::SelectComparison(Some(comparison.clone())),
        )
        .await
        .unwrap();
        let engine = actor.comparison.running.as_ref().unwrap().engine.clone();
        command(&mut actor, close).await.unwrap();
        assert!(actor.settings.comparison_engine_id.is_none());
        assert!(actor.running.is_some());
        assert_eq!(
            *engine.status().borrow(),
            engine_session::EngineStatus::Stopped
        );
    }
    command(
        &mut actor,
        Request::SelectComparison(Some(comparison.clone())),
    )
    .await
    .unwrap();
    command(&mut actor, Request::Close).await.unwrap();
    assert!(actor.running.is_none());
    assert_eq!(
        actor.settings.comparison_engine_id,
        Some(comparison.clone())
    );
    analysis(&mut actor, &mut output, true).await;
    command(&mut actor, Request::Restart).await.unwrap();
    let engine = actor.comparison.running.as_ref().unwrap().engine.clone();
    let main_engine = actor.running.as_ref().unwrap().engine.clone();
    command(&mut actor, Request::CloseAll).await.unwrap();
    assert!(actor.running.is_none() && actor.comparison.running.is_none());
    assert!(actor.settings.engine.is_none() && actor.settings.comparison_engine_id.is_none());
    assert_eq!(actor.settings.main_engine_id, Some(main));
    assert_eq!(
        *engine.status().borrow(),
        engine_session::EngineStatus::Stopped
    );
    assert_eq!(
        *main_engine.status().borrow(),
        engine_session::EngineStatus::Stopped
    );
    cleanup(&mut actor).await;
}

#[test]
fn old_settings_default_comparison_and_invalid_references_are_rejected() {
    let old: Settings = serde_json::from_str("{\"version\":1}").unwrap();
    assert!(old.comparison_engine_id.is_none());
    let mut settings = Settings {
        comparison_engine_id: Some("gtp-only".into()),
        ..Default::default()
    };
    assert!(settings.clone().normalized().is_err());
    settings.engines.push(crate::EngineEntry {
        id: "gtp-only".into(),
        name: "Analysis".into(),
        profile: Default::default(),
    });
    assert!(settings.clone().normalized().is_ok());
    settings.main_engine_id = settings.comparison_engine_id.clone();
    assert!(matches!(
        settings.normalized(),
        Err(SettingsError::SameAsMain)
    ));
}

#[tokio::test]
async fn pending_replacement_keeps_old_session_until_ready_and_respects_layout_cancellation() {
    let (directory, mut actor, mut output, profile) = setup();
    let original_id = add(&mut actor, "Original", profile.clone()).await;
    let gate = directory.path().join("candidate-ready");
    let mut candidate = profile;
    candidate.extra_args.extend([
        "--startup-gate-file".into(),
        gate.to_string_lossy().into_owned(),
    ]);
    let candidate_id = add(&mut actor, "Candidate", candidate).await;
    layout(&mut actor, LayoutMode::DoubleEngine).await;
    command(
        &mut actor,
        Request::SelectComparison(Some(original_id.clone())),
    )
    .await
    .unwrap();
    analysis(&mut actor, &mut output, true).await;
    let old = actor.comparison.running.as_ref().unwrap().engine.clone();
    let (reply, mut response) = oneshot::channel();
    within(actor.engine_command(Request::SelectComparison(Some(candidate_id.clone())), reply))
        .await;
    assert!(actor.comparison.trial.is_some());
    assert!(response.try_recv().is_err());
    assert_eq!(actor.settings.comparison_engine_id, Some(original_id));
    assert!(matches!(
        *old.status().borrow(),
        engine_session::EngineStatus::Ready { .. }
    ));
    within(tokio::fs::write(gate, b"ready")).await.unwrap();
    finish_trials(&mut actor).await;
    let selected = within(response).await.unwrap().unwrap();
    assert_eq!(selected.comparison_engine_id, Some(candidate_id.clone()));
    assert_eq!(
        *old.status().borrow(),
        engine_session::EngineStatus::Stopped
    );
    analysis(&mut actor, &mut output, true).await;
    let (reply, response) = oneshot::channel();
    within(actor.engine_command(Request::RestartComparison, reply)).await;
    assert!(actor.comparison.trial.is_some());
    layout(&mut actor, LayoutMode::Normal).await;
    assert!(within(response).await.unwrap().is_err());
    assert!(actor.comparison.running.is_none() && actor.comparison.trial.is_none());
    assert_eq!(actor.settings.comparison_engine_id, Some(candidate_id));
    cleanup(&mut actor).await;
}

#[tokio::test]
async fn comparison_persistence_failure_preserves_the_running_selection() {
    let (directory, mut actor, mut output, profile) = setup();
    let id = add(&mut actor, "Comparison", profile.clone()).await;
    let other = add(&mut actor, "Other", profile).await;
    layout(&mut actor, LayoutMode::DoubleEngine).await;
    command(&mut actor, Request::SelectComparison(Some(id.clone())))
        .await
        .unwrap();
    analysis(&mut actor, &mut output, true).await;
    let original = actor.comparison.running.as_ref().unwrap().engine.clone();
    let previous = actor.settings.clone();
    actor.directory = directory.path().join("blocked");
    within(tokio::fs::write(&actor.directory, b"not a directory"))
        .await
        .unwrap();
    for request in [
        Request::CloseComparison,
        Request::Remove(id),
        Request::SelectComparison(Some(other)),
    ] {
        assert!(command(&mut actor, request).await.is_err());
        assert_eq!(actor.settings, previous);
        assert!(matches!(
            *original.status().borrow(),
            engine_session::EngineStatus::Ready { .. }
        ));
    }
    cleanup(&mut actor).await;
}

#[tokio::test]
async fn changing_stopped_comparison_selection_never_relabels_previous_engine_results() {
    let (_directory, mut actor, mut output, profile) = setup();
    let one = add(&mut actor, "One", profile.clone()).await;
    let mut broken = profile;
    broken.extra_args.push("--fail-startup".into());
    let two = add(&mut actor, "Two", broken).await;
    layout(&mut actor, LayoutMode::DoubleEngine).await;
    command(&mut actor, Request::SelectComparison(Some(one)))
        .await
        .unwrap();
    analysis(&mut actor, &mut output, true).await;
    assert!(!actor.comparison.results.is_empty());
    layout(&mut actor, LayoutMode::Normal).await;
    // Layout alone preserves results for an SGF save and same-engine re-entry.
    assert!(!actor.comparison.results.is_empty());
    command(&mut actor, Request::SelectComparison(Some(two)))
        .await
        .unwrap();
    assert!(actor.comparison.results.is_empty());
    layout(&mut actor, LayoutMode::DoubleEngine).await;
    assert!(matches!(
        actor.comparison.status,
        EngineStatus::Failed { .. }
    ));
    assert!(actor.comparison_view().analysis.is_none());
    cleanup(&mut actor).await;
}

#[tokio::test]
async fn identical_profiles_in_distinct_roles_never_share_persistent_results() {
    use analysis::persistent_cache::{DEFAULT_MAX_BYTES, PersistentCache};
    let (directory, mut actor, _output, profile) = setup();
    actor.settings.pondering = false;
    let database = directory.path().join("comparison-cache.sqlite3");
    let cache = PersistentCache::spawn(database.clone(), DEFAULT_MAX_BYTES, true);
    actor.cache = Some(cache.clone());
    let main = add(&mut actor, "Main", profile.clone()).await;
    let comparison = add(&mut actor, "Comparison", profile).await;
    command(&mut actor, Request::Select(main)).await.unwrap();
    layout(&mut actor, LayoutMode::DoubleEngine).await;
    command(&mut actor, Request::SelectComparison(Some(comparison)))
        .await
        .unwrap();
    let main_profile = actor.running.as_ref().unwrap().fingerprint.clone();
    let comparison_profile = actor
        .comparison
        .running
        .as_ref()
        .unwrap()
        .fingerprint
        .clone();
    assert_eq!(main_profile, comparison_profile);
    let main_key = actor.cache_fingerprint_for(&main_profile).unwrap();
    let comparison_key = actor
        .comparison_cache_fingerprint(&comparison_profile)
        .unwrap();
    assert_ne!(main_key, comparison_key);
    let position = actor
        .document
        .position_at(actor.document.current())
        .unwrap();
    let key = position.analysis_key();
    let node = actor.document.current();
    // Both real fake sessions stay paused. Deterministic frames still travel
    // through the production acceptance and asynchronous cache writer per role.
    actor.settings.pondering = true;
    let main = crate::actor::tests::snapshot(&actor, 100, 0.25);
    let comparison = crate::actor::tests::snapshot(&actor, 200, 0.75);
    within(actor.cache_signal(crate::actor::Signal::Snapshot(Box::new(main.clone())))).await;
    assert_eq!(
        within(cache.get(key, &main_key)).await,
        Some(main.response.clone())
    );
    assert_eq!(within(cache.get(key, &comparison_key)).await, None);
    assert!(
        within(actor.comparison_cached_focus(node, &position))
            .await
            .is_none()
    );
    within(actor.comparison_signal(crate::actor::Signal::Snapshot(Box::new(comparison.clone()))))
        .await;
    assert_eq!(actor.review.results[&node].response, main.response);
    assert_eq!(
        actor.comparison.results[&node].response,
        comparison.response
    );
    within(cache.flush()).await;
    let reader = PersistentCache::spawn(database, DEFAULT_MAX_BYTES, true);
    assert_eq!(
        within(reader.get(key, &main_key)).await,
        Some(main.response.clone())
    );
    assert_eq!(
        within(reader.get(key, &comparison_key)).await,
        Some(comparison.response.clone())
    );
    actor.review.results.clear();
    actor.comparison.results.clear();
    assert_eq!(
        within(actor.cached_focus(node, &position)).await,
        Some(main.response)
    );
    assert_eq!(
        within(actor.comparison_cached_focus(node, &position)).await,
        Some(comparison.response)
    );
    cleanup(&mut actor).await;
    within(cache.shutdown()).await;
    within(reader.shutdown()).await;
}

#[tokio::test]
async fn concurrent_startup_trials_merge_only_their_own_entries_in_either_order() {
    for comparison_first in [false, true] {
        let (directory, mut actor, _output, profile) = setup();
        actor.settings.pondering = false;
        let main = add(&mut actor, "Main", profile.clone()).await;
        let comparison = add(&mut actor, "Comparison", profile.clone()).await;
        actor.settings.main_engine_id = Some(main.clone());
        actor.settings.engine = Some(profile.clone());
        actor.settings.comparison_engine_id = Some(comparison.clone());
        actor.settings.display.layout_mode = LayoutMode::DoubleEngine;
        let mut main_profile = profile.clone();
        main_profile
            .extra_args
            .extend(["--time-scale".into(), "2".into()]);
        let mut comparison_profile = profile;
        comparison_profile
            .extra_args
            .extend(["--time-scale".into(), "3".into()]);
        let mut main_entries = actor.settings.engines.clone();
        main_entries[0].name = "Confirmed main".into();
        main_entries[0].profile = main_profile.clone();
        let mut comparison_entries = actor.settings.engines.clone();
        comparison_entries[1].name = "Confirmed comparison".into();
        comparison_entries[1].profile = comparison_profile.clone();
        // Remote application startup uses these same independent pending slots.
        // The local profiles exercise real readiness without network fixtures.
        within(actor.begin_target_trial(
            main_entries,
            main.clone(),
            main_profile.clone(),
            trial::Reply::Startup,
            false,
        ))
        .await;
        within(actor.begin_target_trial(
            comparison_entries,
            comparison.clone(),
            comparison_profile.clone(),
            trial::Reply::Startup,
            true,
        ))
        .await;
        if comparison_first {
            finish_comparison(&mut actor).await;
            finish_main(&mut actor).await;
        } else {
            finish_main(&mut actor).await;
            finish_comparison(&mut actor).await;
        }
        assert_eq!(actor.settings.engines[0].name, "Confirmed main");
        assert_eq!(actor.settings.engines[0].profile, main_profile);
        assert_eq!(actor.settings.engines[1].name, "Confirmed comparison");
        assert_eq!(actor.settings.engines[1].profile, comparison_profile);
        assert_eq!(actor.settings.main_engine_id, Some(main));
        assert_eq!(actor.settings.comparison_engine_id, Some(comparison));
        assert_eq!(
            within(Settings::load(directory.path())).await.unwrap(),
            actor.settings
        );
        cleanup(&mut actor).await;
    }
}

async fn finish_main(actor: &mut Actor) {
    within(async {
        while actor.trial.is_some() {
            let signal = trial::wait(&mut actor.trial).await;
            actor.trial_signal(signal).await;
        }
    })
    .await;
}

async fn finish_comparison(actor: &mut Actor) {
    within(async {
        while actor.comparison.trial.is_some() {
            let signal = trial::wait(&mut actor.comparison.trial).await;
            actor.comparison_trial_signal(signal).await;
        }
    })
    .await;
}

#[tokio::test]
async fn pending_candidate_cannot_overwrite_a_changed_target_entry() {
    let (_directory, mut actor, _output, profile) = setup();
    actor.settings.pondering = false;
    let id = add(&mut actor, "Comparison", profile.clone()).await;
    actor.settings.display.layout_mode = LayoutMode::DoubleEngine;
    let (reply, response) = oneshot::channel();
    within(actor.begin_target_trial(
        actor.settings.engines.clone(),
        id,
        profile,
        trial::Reply::Settings(reply),
        true,
    ))
    .await;
    // A runtime remote identity refresh can change an entry independently of
    // the list-edit commands, which normally cancel all older candidates.
    actor.settings.engines[0].name = "Newer entry".into();
    finish_comparison(&mut actor).await;
    assert!(matches!(
        within(response).await.unwrap(),
        Err(AppError::EngineSetup(crate::EngineSetupError::ChangedFiles))
    ));
    assert_eq!(actor.settings.engines[0].name, "Newer entry");
    assert!(actor.settings.comparison_engine_id.is_none());
    assert!(actor.comparison.running.is_none());
    cleanup(&mut actor).await;
}

#[tokio::test]
async fn comparison_startup_neither_commits_nor_rolls_back_another_pending_secret() {
    for complete in [false, true] {
        let (directory, mut actor, _output, profile) = setup();
        actor.settings.pondering = false;
        let comparison = add(&mut actor, "Comparison", profile).await;
        actor.settings.comparison_engine_id = Some(comparison);
        actor.settings.display.layout_mode = LayoutMode::DoubleEngine;
        // Constructing this transport does not connect to the loopback URL.
        // It represents credentials owned by a simultaneous main edit.
        let pending_profile = within(actor.remote.import(crate::RemoteEngineInput::WebSocket {
            url: "ws://127.0.0.1:12345/pending-test-token".into(),
        }))
        .await
        .unwrap();
        let remote = pending_profile.remote.as_ref().unwrap();
        within(actor.reconcile_comparison()).await;
        if complete {
            finish_comparison(&mut actor).await;
        } else {
            within(actor.cancel_comparison_trial()).await;
        }
        assert!(
            within(
                actor
                    .remote
                    .transport(remote, directory.path().join("known_hosts"))
            )
            .await
            .is_ok()
        );
        actor.remote.rollback();
        assert!(matches!(
            within(
                actor
                    .remote
                    .transport(remote, directory.path().join("known_hosts"))
            )
            .await,
            Err(AppError::Remote(crate::RemoteEngineError::MissingSecret))
        ));
        cleanup(&mut actor).await;
    }
}

#[tokio::test]
async fn main_configuration_and_validation_failures_preserve_comparison_startup() {
    let (directory, mut actor, mut output, profile) = setup();
    let main = add(&mut actor, "Main", profile.clone()).await;
    let gate = directory.path().join("comparison-ready");
    let launches = directory.path().join("comparison-launches");
    let mut comparison_profile = profile.clone();
    comparison_profile.extra_args.extend([
        "--startup-gate-file".into(),
        gate.to_string_lossy().into_owned(),
        "--launch-log".into(),
        launches.to_string_lossy().into_owned(),
    ]);
    let comparison = add(&mut actor, "Comparison", comparison_profile).await;
    actor.settings.main_engine_id = Some(main);
    actor.settings.comparison_engine_id = Some(comparison.clone());
    let mut display = actor.settings.display.clone();
    display.layout_mode = LayoutMode::DoubleEngine;
    within(actor.set_display_settings(display)).await.unwrap();
    assert!(actor.comparison.trial.is_some());

    within(actor.configure_engine(None)).await.unwrap();
    assert!(actor.comparison.trial.is_some());
    within(actor.configure_engine(Some(profile.clone())))
        .await
        .unwrap();
    assert!(actor.comparison.trial.is_some());

    let mut invalid = profile.clone();
    invalid.extra_args.push("-model=conflicting".into());
    assert!(within(actor.configure_engine(Some(invalid))).await.is_err());
    assert!(actor.comparison.trial.is_some());
    let (reply, response) = oneshot::channel();
    within(actor.engine_command(Request::Select(comparison.clone()), reply)).await;
    assert!(matches!(
        within(response).await.unwrap(),
        Err(AppError::Settings(SettingsError::SameAsComparison))
    ));
    assert!(actor.comparison.trial.is_some());

    let (reply, response) = oneshot::channel();
    within(actor.begin_trial(
        crate::EngineSetupProfile {
            program: profile.program,
            model: directory.path().join("missing-model"),
            config: Some(profile.config),
            extra_args: Vec::new(),
        },
        "unused-template".into(),
        reply,
    ))
    .await;
    assert!(within(response).await.unwrap().is_err());
    assert!(actor.comparison.trial.is_some());

    within(tokio::fs::write(&gate, "ready")).await.unwrap();
    finish_trials(&mut actor).await;
    analysis(&mut actor, &mut output, true).await;
    assert_eq!(actor.settings.comparison_engine_id, Some(comparison));
    assert!(actor.comparison.running.is_some());
    assert_eq!(
        within(tokio::fs::read_to_string(launches))
            .await
            .unwrap()
            .lines()
            .count(),
        1,
        "unrelated main operations must preserve the original startup"
    );
    cleanup(&mut actor).await;
}

#[tokio::test]
async fn cancelled_user_selection_reports_stopped_without_a_running_comparison() {
    for invalid_main_selection in [false, true] {
        let (directory, mut actor, mut output, mut profile) = setup();
        profile.extra_args.extend([
            "--startup-gate-file".into(),
            directory
                .path()
                .join("never-ready")
                .to_string_lossy()
                .into(),
        ]);
        let comparison = add(&mut actor, "Comparison", profile).await;
        layout(&mut actor, LayoutMode::DoubleEngine).await;
        let (reply, response) = oneshot::channel();
        within(actor.engine_command(Request::SelectComparison(Some(comparison)), reply)).await;
        assert_eq!(actor.comparison.status, EngineStatus::Starting);
        if invalid_main_selection {
            let (reply, response) = oneshot::channel();
            within(actor.engine_command(Request::Select("missing".into()), reply)).await;
            assert!(within(response).await.unwrap().is_err());
        } else {
            within(actor.configure_engine(None)).await.unwrap();
        }
        assert!(matches!(
            within(response).await.unwrap(),
            Err(AppError::EngineSetup(crate::EngineSetupError::Cancelled))
        ));
        assert!(actor.settings.comparison_engine_id.is_none());
        assert!(actor.comparison.trial.is_none());
        assert!(actor.comparison.running.is_none());
        assert_eq!(actor.comparison.status, EngineStatus::Stopped);
        within(async {
            loop {
                if let AppEvent::ComparisonChanged(view) = output.recv().await.unwrap() {
                    assert_eq!(view.status, EngineStatus::Stopped);
                    break;
                }
            }
        })
        .await;
        cleanup(&mut actor).await;
    }
}
