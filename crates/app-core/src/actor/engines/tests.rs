#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::{AppEvent, AppSubscription, EngineStatus};
use std::{future::Future, time::Duration};

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("safety deadline")
}

fn setup() -> (tempfile::TempDir, Actor, AppSubscription, EngineProfile) {
    // Build before any safety timer; process and filesystem speed do not decide behavior.
    let binary = fake_katago::binary_path().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let (mut actor, output) = super::super::tests::actor();
    actor.directory = directory.path().to_owned();
    actor.config.analysis.live.max_visits = 1;
    actor.config.analysis.live.report_interval = Duration::from_secs(60);
    actor.config.session.write_timeout = Duration::from_secs(60);
    actor.config.session.shutdown_timeout = Duration::from_secs(60);
    let profile = EngineProfile {
        remote: None,
        human_model: None,
        program: binary.clone(),
        model: binary.clone(),
        config: binary,
        extra_args: vec!["--visits-per-sec".into(), "100000".into()],
        canonical_model_name: Some("Retained catalog name".into()),
    };
    (directory, actor, output, profile)
}

async fn command(actor: &mut Actor, request: Request) -> Result<Settings, AppError> {
    let (reply, response) = oneshot::channel();
    within(actor.engine_command(request, reply)).await;
    if actor.trial.is_some() {
        let result = within(trial::wait(&mut actor.trial)).await;
        within(actor.finish_trial(result)).await;
    }
    within(response).await.unwrap()
}

async fn final_analysis(actor: &mut Actor, output: &mut AppSubscription) {
    within(async {
        loop {
            tokio::select! {
                signal = super::super::next_signal(&mut actor.running) => actor.signal(signal),
                event = output.recv() => match event.unwrap() {
                    AppEvent::AnalysisUpdated(value) if !value.is_during_search => return,
                    AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => panic!("{reason}"),
                    _ => {},
                }
            }
        }
    }).await;
}

#[tokio::test]
async fn list_add_update_reorder_remove_and_validation_are_persisted() {
    let (directory, mut actor, _output, profile) = setup();
    let first = command(&mut actor, Request::Add("Primary".into(), profile.clone()))
        .await
        .unwrap();
    let first_id = first.engines[0].id.clone();
    assert!(first.engine.is_none());
    assert!(first.main_engine_id.is_none());
    let second = command(&mut actor, Request::Add("Backup".into(), profile.clone()))
        .await
        .unwrap();
    let second_id = second.engines[1].id.clone();
    let mut changed = profile.clone();
    changed.model = "missing-model-is-editable.bin.gz".into();
    command(
        &mut actor,
        Request::Update(second_id.clone(), "Renamed".into(), changed.clone()),
    )
    .await
    .unwrap();
    let ordered = command(
        &mut actor,
        Request::Reorder(vec![second_id.clone(), first_id.clone()]),
    )
    .await
    .unwrap();
    assert_eq!(ordered.engines[0].name, "Renamed");
    assert_eq!(ordered.engines[0].profile, changed);
    assert_eq!(
        crate::Settings::load(directory.path()).await.unwrap(),
        ordered
    );
    for request in [
        Request::Add(" ".into(), profile.clone()),
        Request::Update("unknown".into(), "Missing".into(), profile),
        Request::Remove("unknown".into()),
        Request::Reorder(vec![first_id.clone(), first_id.clone()]),
        Request::Reorder(vec![]),
        Request::Select("unknown".into()),
        Request::Restart,
    ] {
        assert!(command(&mut actor, request).await.is_err());
        assert_eq!(actor.settings, ordered);
    }
    let remaining = command(&mut actor, Request::Remove(second_id))
        .await
        .unwrap();
    assert_eq!(remaining.engines.len(), 1);
    assert_eq!(remaining.engines[0].id, first_id);
    assert_eq!(
        crate::Settings::load(directory.path()).await.unwrap(),
        remaining
    );
}

#[tokio::test]
async fn selecting_restarting_closing_and_removing_main_engine_control_real_fake_sessions() {
    let (directory, mut actor, mut output, profile) = setup();
    let settings = command(&mut actor, Request::Add("One".into(), profile.clone()))
        .await
        .unwrap();
    let one = settings.engines[0].id.clone();
    let mut second_profile = profile.clone();
    second_profile
        .extra_args
        .extend(["--time-scale".into(), "2".into()]);
    let settings = command(
        &mut actor,
        Request::Add("Two".into(), second_profile.clone()),
    )
    .await
    .unwrap();
    let two = settings.engines[1].id.clone();
    let settings = command(&mut actor, Request::Select(one)).await.unwrap();
    assert_eq!(settings.engine, Some(profile));
    final_analysis(&mut actor, &mut output).await;
    let old = actor.running.as_ref().unwrap().engine.clone();
    let selected = command(&mut actor, Request::Select(two.clone()))
        .await
        .unwrap();
    assert_eq!(selected.main_engine_id, Some(two.clone()));
    assert_eq!(selected.engine, Some(second_profile.clone()));
    assert_eq!(
        *old.status().borrow(),
        engine_session::EngineStatus::Stopped
    );
    final_analysis(&mut actor, &mut output).await;
    let old = actor.running.as_ref().unwrap().engine.clone();
    assert_eq!(
        command(&mut actor, Request::Restart).await.unwrap(),
        selected
    );
    assert_eq!(
        *old.status().borrow(),
        engine_session::EngineStatus::Stopped
    );
    final_analysis(&mut actor, &mut output).await;
    let closed = command(&mut actor, Request::Close).await.unwrap();
    assert!(closed.engine.is_none());
    assert_eq!(closed.main_engine_id, Some(two.clone()));
    assert_eq!(closed.engines, selected.engines);
    assert!(actor.running.is_none());
    assert!(actor.focus.is_none());
    assert_eq!(
        crate::Settings::load(directory.path()).await.unwrap(),
        closed
    );
    assert!(matches!(
        within(output.recv()).await,
        Some(AppEvent::EngineStatusChanged(EngineStatus::Stopped))
    ));
    command(&mut actor, Request::Restart).await.unwrap();
    final_analysis(&mut actor, &mut output).await;
    let old = actor.running.as_ref().unwrap().engine.clone();
    let removed = command(&mut actor, Request::Remove(two)).await.unwrap();
    assert_eq!(
        *old.status().borrow(),
        engine_session::EngineStatus::Stopped
    );
    assert!(removed.main_engine_id.is_none());
    assert!(removed.engine.is_none());
    assert_eq!(removed.engines.len(), 1);
    assert_eq!(
        crate::Settings::load(directory.path()).await.unwrap(),
        removed
    );
}

#[tokio::test]
async fn failed_selection_and_active_edit_preserve_current_profile_and_analysis() {
    let (directory, mut actor, mut output, profile) = setup();
    let added = command(&mut actor, Request::Add("Current".into(), profile.clone()))
        .await
        .unwrap();
    let current = added.engines[0].id.clone();
    command(&mut actor, Request::Select(current.clone()))
        .await
        .unwrap();
    final_analysis(&mut actor, &mut output).await;
    let mut bad = profile.clone();
    bad.extra_args.push("--fail-startup".into());
    let added = command(&mut actor, Request::Add("Broken".into(), bad.clone()))
        .await
        .unwrap();
    let broken = added.engines[1].id.clone();
    let bytes = tokio::fs::read(directory.path().join("settings.json"))
        .await
        .unwrap();
    let old = actor.running.as_ref().unwrap().engine.clone();
    for request in [
        Request::Select(broken),
        Request::Update(current.clone(), "Broken edit".into(), bad),
    ] {
        let error = command(&mut actor, request).await.unwrap_err().to_string();
        assert!(error.contains("模型找不到"), "{error}");
        assert_eq!(actor.settings, added);
        assert_eq!(
            tokio::fs::read(directory.path().join("settings.json"))
                .await
                .unwrap(),
            bytes
        );
        assert!(matches!(
            *old.status().borrow(),
            engine_session::EngineStatus::Ready { .. }
        ));
    }
    let mut changed = profile;
    changed
        .extra_args
        .extend(["--time-scale".into(), "3".into()]);
    let updated = command(
        &mut actor,
        Request::Update(current, "Updated".into(), changed.clone()),
    )
    .await
    .unwrap();
    assert_eq!(updated.engine, Some(changed.clone()));
    assert_eq!(updated.engines[0].profile, changed);
    assert_eq!(updated.engines[0].name, "Updated");
    final_analysis(&mut actor, &mut output).await;
    within(actor.stop_engine()).await.unwrap();
}

#[tokio::test]
async fn legacy_configure_and_trial_adapters_keep_list_and_selected_profile_in_sync() {
    let (_directory, mut actor, mut output, profile) = setup();
    within(actor.configure_engine(Some(profile.clone())))
        .await
        .unwrap();
    final_analysis(&mut actor, &mut output).await;
    let id = actor.settings.main_engine_id.clone();
    assert_eq!(actor.settings.engines[0].profile, profile);
    let mut changed = profile.clone();
    changed
        .extra_args
        .extend(["--time-scale".into(), "2".into()]);
    within(actor.configure_engine(Some(changed.clone())))
        .await
        .unwrap();
    final_analysis(&mut actor, &mut output).await;
    assert_eq!(actor.settings.main_engine_id, id);
    assert_eq!(actor.settings.engines.len(), 1);
    assert_eq!(actor.settings.engines[0].profile, changed);
    let (reply, response) = oneshot::channel();
    within(actor.begin_trial(
        crate::EngineSetupProfile {
            program: profile.program,
            model: profile.model,
            config: Some(profile.config),
            extra_args: vec![],
        },
        "unused-template".into(),
        reply,
    ))
    .await;
    let ready = within(trial::wait(&mut actor.trial)).await;
    within(actor.finish_trial(ready)).await;
    within(response).await.unwrap().unwrap();
    assert_eq!(actor.settings.engines.len(), 1);
    assert_eq!(actor.settings.main_engine_id, id);
    assert_eq!(
        actor.settings.engine.as_ref(),
        Some(&actor.settings.engines[0].profile)
    );
    final_analysis(&mut actor, &mut output).await;
    within(actor.configure_engine(None)).await.unwrap();
    assert_eq!(actor.settings.main_engine_id, id);
    assert_eq!(actor.settings.engines.len(), 1);
    assert!(actor.settings.engine.is_none());
    assert!(actor.running.is_none());
}

#[tokio::test]
async fn pending_selection_preserves_unrelated_settings_and_is_superseded_by_list_edits() {
    let (directory, mut actor, _output, profile) = setup();
    let added = command(&mut actor, Request::Add("Current".into(), profile.clone()))
        .await
        .unwrap();
    let id = added.engines[0].id.clone();
    let (reply, response) = oneshot::channel();
    within(actor.engine_command(Request::Select(id.clone()), reply)).await;
    assert!(actor.trial.is_some());
    let display = crate::DisplaySettings {
        show_coordinates: false,
        ..crate::DisplaySettings::default()
    };
    within(actor.set_display_settings(display.clone()))
        .await
        .unwrap();
    let ready = within(trial::wait(&mut actor.trial)).await;
    within(actor.finish_trial(ready)).await;
    let selected = within(response).await.unwrap().unwrap();
    assert_eq!(selected.display, display);
    assert_eq!(
        crate::Settings::load(directory.path()).await.unwrap(),
        selected
    );
    let (reply, response) = oneshot::channel();
    within(actor.engine_command(Request::Select(id), reply)).await;
    assert!(actor.trial.is_some());
    let new = command(&mut actor, Request::Add("New".into(), profile))
        .await
        .unwrap();
    assert!(
        within(response)
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("cancelled")
    );
    assert!(actor.trial.is_none());
    assert_eq!(new.engines.len(), 2);
    assert_eq!(new.engine, selected.engine);
    assert_eq!(new.display, display);
    within(actor.stop_engine()).await.unwrap();
}

#[tokio::test]
async fn failed_persistence_preserves_running_engine_for_close_remove_and_active_update() {
    let (directory, mut actor, mut output, profile) = setup();
    let added = command(&mut actor, Request::Add("Current".into(), profile.clone()))
        .await
        .unwrap();
    let id = added.engines[0].id.clone();
    let selected = command(&mut actor, Request::Select(id.clone()))
        .await
        .unwrap();
    final_analysis(&mut actor, &mut output).await;
    let original = actor.running.as_ref().unwrap().engine.clone();
    let saved = tokio::fs::read(directory.path().join("settings.json"))
        .await
        .unwrap();
    actor.directory = directory.path().join("not-a-directory");
    tokio::fs::write(&actor.directory, b"block").await.unwrap();
    let mut changed = profile.clone();
    changed
        .extra_args
        .extend(["--time-scale".into(), "2".into()]);
    for request in [
        Request::Close,
        Request::Remove(id.clone()),
        Request::Add("Other".into(), profile),
        Request::Update(id, "Updated".into(), changed),
    ] {
        assert!(command(&mut actor, request).await.is_err());
        assert_eq!(actor.settings, selected);
        assert_eq!(
            tokio::fs::read(directory.path().join("settings.json"))
                .await
                .unwrap(),
            saved
        );
        assert!(matches!(
            *original.status().borrow(),
            engine_session::EngineStatus::Ready { .. }
        ));
    }
    within(
        actor.apply(super::super::Input::Edit(document::Command::Play(
            go_core::Move::Pass(go_core::Color::Black),
        ))),
    )
    .await
    .unwrap();
    final_analysis(&mut actor, &mut output).await;
    within(actor.stop_engine()).await.unwrap();
}
