#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod auto_setup_support;
use app_core as core_types;
use app_core::{AppError, AutoSetupError, EngineProfile, Settings, engine_setup::SearchRoots};
use auto_setup_support::*;
use tempfile::tempdir;

#[tokio::test]
async fn automatic_setup_saves_profile_and_starts_analysis() {
    let dir = tempdir().unwrap();
    let install = dir.path().join("installed");
    let executable = program(&install, "1.16.5-fake", false);
    let model = install.join("default.bin.gz");
    std::fs::write(&model, b"fake model").unwrap();
    let settings = dir.path().join("settings");
    let (core, mut output) = app_core::spawn_with_config(&settings, config())
        .await
        .unwrap();
    let selected = within(core.auto_configure_engine(
        SearchRoots {
            windows_dirs: vec![install.clone()],
            ..Default::default()
        },
        template(dir.path()),
    ))
    .await
    .unwrap();
    assert_eq!(selected.program, executable);
    assert_eq!(selected.model, model);
    assert_eq!(selected.config, settings.join("analysis.cfg"));
    assert_eq!(
        Settings::load(&settings).await.unwrap().engine,
        Some(selected)
    );
    analyzed(&mut output).await;
    assert_eq!(
        std::fs::read_to_string(install.join("trials")).unwrap(),
        "trial\n"
    );
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn first_trial_failure_falls_back_to_next_program() {
    let dir = tempdir().unwrap();
    let bad = dir.path().join("bad");
    let good = dir.path().join("good");
    program(&bad, "1.16.5-fake", true);
    let expected = program(&good, "1.16.5-fake", false);
    std::fs::write(bad.join("model.bin.gz"), b"fake model").unwrap();
    let (core, mut output) = app_core::spawn_with_config(dir.path().join("settings"), config())
        .await
        .unwrap();
    let selected = within(core.auto_configure_engine(
        SearchRoots {
            windows_dirs: vec![bad.clone(), good.clone()],
            ..Default::default()
        },
        template(dir.path()),
    ))
    .await
    .unwrap();
    assert_eq!(selected.program, expected);
    assert_eq!(
        std::fs::read_to_string(bad.join("trials")).unwrap(),
        "trial\n"
    );
    assert_eq!(
        std::fs::read_to_string(good.join("trials")).unwrap(),
        "trial\n"
    );
    analyzed(&mut output).await;
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn all_failed_trials_return_reasons_and_do_not_save() {
    let dir = tempdir().unwrap();
    let installs: Vec<_> = ["one", "two"]
        .into_iter()
        .map(|name| dir.path().join(name))
        .collect();
    for install in &installs {
        program(install, "1.16.5-fake", true);
    }
    std::fs::write(installs[0].join("model.bin.gz"), b"fake model").unwrap();
    let settings = dir.path().join("settings");
    let (core, _output) = app_core::spawn_with_config(&settings, config())
        .await
        .unwrap();
    let error = within(core.auto_configure_engine(
        SearchRoots {
            windows_dirs: installs,
            ..Default::default()
        },
        template(dir.path()),
    ))
    .await
    .unwrap_err();
    let AppError::AutoSetup(AutoSetupError::AllFailed { attempts, .. }) = error else {
        panic!("{error}")
    };
    assert_eq!(attempts.len(), 2);
    assert!(
        attempts
            .iter()
            .all(|attempt| attempt.reason.contains("模型找不到"))
    );
    assert!(core.settings().await.unwrap().engine.is_none());
    assert!(!settings.join("settings.json").exists());
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn saved_profile_is_returned_without_discovery_or_reconfiguration() {
    let dir = tempdir().unwrap();
    let executable = program(dir.path(), "1.16.5-fake", false);
    let model = dir.path().join("model.bin.gz");
    std::fs::write(&model, "fake model").unwrap();
    let profile = EngineProfile {
        program: executable,
        model,
        config: template(dir.path()),
        extra_args: Vec::new(),
        canonical_model_name: Some("remembered-name.bin.gz".into()),
    };
    let (core, mut output) = app_core::spawn_with_config(dir.path(), config())
        .await
        .unwrap();
    within(core.configure_engine(Some(profile.clone())))
        .await
        .unwrap();
    analyzed(&mut output).await;
    let saved = std::fs::read(dir.path().join("settings.json")).unwrap();
    assert_eq!(
        within(core.auto_configure_engine(SearchRoots::default(), "nonexistent-template".into()))
            .await
            .unwrap(),
        profile
    );
    assert_eq!(
        std::fs::read(dir.path().join("settings.json")).unwrap(),
        saved
    );
    assert!(!dir.path().join("versions").exists());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("trials")).unwrap(),
        "trial\n"
    );
    within(core.shutdown()).await.unwrap();
}
