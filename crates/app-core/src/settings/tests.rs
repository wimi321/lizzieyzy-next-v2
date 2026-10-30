#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use std::{fs, time::Duration};

fn profile(directory: &Path) -> EngineProfile {
    let profile = EngineProfile {
        program: directory.join("engine"),
        model: directory.join("model.bin"),
        config: directory.join("analysis.cfg"),
        extra_args: Vec::new(),
    };
    for path in [&profile.program, &profile.model, &profile.config] {
        fs::write(path, b"fixture").unwrap();
    }
    profile
}

#[tokio::test]
async fn settings_round_trip_and_missing_file_defaults() {
    let directory = tempfile::tempdir().unwrap();
    assert_eq!(
        Settings::load(directory.path()).await.unwrap(),
        Settings::default()
    );
    let settings = Settings {
        engine: Some(profile(directory.path())),
        pondering: false,
        ..Settings::default()
    };
    let nested = directory.path().join("portable/settings");
    settings.save(&nested).await.unwrap();
    assert_eq!(Settings::load(&nested).await.unwrap(), settings);
    Settings::default().save(&nested).await.unwrap();
    assert_eq!(Settings::load(&nested).await.unwrap(), Settings::default());
}

#[tokio::test]
async fn legacy_settings_default_missing_fields_and_ignore_unknown_fields() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    fs::write(
        &path,
        br#"{"version":0,"removed_option":12,"engine":{"program":"katago","unknown":true}}"#,
    )
    .unwrap();
    let settings = Settings::load(directory.path()).await.unwrap();
    assert_eq!(settings.version, SETTINGS_VERSION);
    assert!(settings.pondering);
    assert_eq!(
        settings.engine.unwrap(),
        EngineProfile {
            program: "katago".into(),
            ..EngineProfile::default()
        }
    );
    fs::write(&path, b"{}").unwrap();
    assert_eq!(
        Settings::load(directory.path()).await.unwrap(),
        Settings::default()
    );
}

#[tokio::test]
async fn future_or_malformed_settings_are_errors_and_failed_save_preserves_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.json");
    let future = br#"{"version":2,"future_field":"keep this"}"#;
    fs::write(&path, future).unwrap();
    assert!(matches!(
        Settings::load(directory.path()).await,
        Err(SettingsError::FutureVersion(2))
    ));
    assert!(matches!(
        Settings {
            version: 2,
            ..Settings::default()
        }
        .save(directory.path())
        .await,
        Err(SettingsError::FutureVersion(2))
    ));
    assert_eq!(fs::read(&path).unwrap(), future);
    fs::write(&path, b"{broken").unwrap();
    assert!(matches!(
        Settings::load(directory.path()).await,
        Err(SettingsError::Json(_))
    ));
    assert_eq!(fs::read(&path).unwrap(), b"{broken");
}

#[tokio::test]
async fn replacement_failure_keeps_existing_destination_and_removes_temporary_file() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("existing");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("original"), b"keep").unwrap();
    assert!(
        crate::storage::write(target.clone(), b"replacement".to_vec())
            .await
            .is_err()
    );
    assert_eq!(fs::read(target.join("original")).unwrap(), b"keep");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[tokio::test]
async fn fingerprints_include_each_path_size_modification_time_and_arguments() {
    let directory = tempfile::tempdir().unwrap();
    let original = profile(directory.path());
    let expected = original.fingerprint().await.unwrap();
    assert_eq!(original.fingerprint().await.unwrap(), expected);

    for slot in 0..3 {
        let mut renamed = original.clone();
        let path = match slot {
            0 => &mut renamed.program,
            1 => &mut renamed.model,
            _ => &mut renamed.config,
        };
        let new_path = directory.path().join(format!("copy-{slot}"));
        fs::copy(&*path, &new_path).unwrap();
        let original_time = fs::metadata(&*path).unwrap().modified().unwrap();
        fs::File::options()
            .write(true)
            .open(&new_path)
            .unwrap()
            .set_modified(original_time)
            .unwrap();
        *path = new_path;
        assert_ne!(renamed.fingerprint().await.unwrap(), expected);
    }

    for path in [&original.program, &original.model, &original.config] {
        let original_time = fs::metadata(path).unwrap().modified().unwrap();
        fs::write(path, b"longer fixture").unwrap();
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(original_time)
            .unwrap();
        assert_ne!(original.fingerprint().await.unwrap(), expected);
        fs::write(path, b"fixture").unwrap();
        let file = fs::File::options().write(true).open(path).unwrap();
        file.set_modified(original_time).unwrap();
        assert_eq!(original.fingerprint().await.unwrap(), expected);
        file.set_modified(original_time + Duration::from_secs(20))
            .unwrap();
        assert_ne!(original.fingerprint().await.unwrap(), expected);
        file.set_modified(original_time).unwrap();
    }
    let mut changed = original;
    changed.extra_args = vec!["--threads".into(), "2".into()];
    assert_ne!(changed.fingerprint().await.unwrap(), expected);
    let changed_fingerprint = changed.fingerprint().await.unwrap();
    changed.extra_args.reverse();
    assert_ne!(changed.fingerprint().await.unwrap(), changed_fingerprint);
}

#[tokio::test]
async fn missing_model_and_non_file_config_have_readable_errors() {
    let directory = tempfile::tempdir().unwrap();
    let mut profile = profile(directory.path());
    fs::remove_file(&profile.model).unwrap();
    let error = profile.fingerprint().await.unwrap_err();
    assert!(error.to_string().contains("engine model"));
    assert!(error.to_string().contains("model.bin"));
    fs::write(&profile.model, b"model").unwrap();
    profile.config = directory.path().to_owned();
    assert!(
        profile
            .fingerprint()
            .await
            .unwrap_err()
            .to_string()
            .contains("not a regular file")
    );
}

#[test]
fn local_transport_forces_black_and_preserves_argument_boundaries() {
    let profile = EngineProfile {
        program: "path with spaces/katago".into(),
        model: "model with spaces.bin".into(),
        config: "custom config.cfg".into(),
        extra_args: vec![
            "-override-config".into(),
            "numSearchThreads=2,reportAnalysisWinratesAs=BLACK".into(),
        ],
    };
    let TransportSpec::LocalProcess {
        program,
        args,
        cwd,
        env,
    } = profile.transport().unwrap();
    assert_eq!(program, profile.program);
    assert!(cwd.is_none());
    assert!(env.is_empty());
    assert_eq!(
        args,
        [
            "analysis",
            "-config",
            "custom config.cfg",
            "-model",
            "model with spaces.bin",
            "-override-config",
            "numSearchThreads=2,numAnalysisThreads=2,reportAnalysisWinratesAs=BLACK"
        ]
        .map(OsString::from)
    );
}

#[test]
fn multiple_override_sources_merge_once_and_keep_other_arguments_in_order() {
    let profile = EngineProfile {
        extra_args: [
            "--first",
            "first value with spaces",
            "-override-config",
            "numSearchThreads=2, reportAnalysisWinratesAs = BLACK ,nnCacheSizePowerOfTwo=20",
            "--middle",
            "-override-config=nnMutexPoolSizePowerOfTwo=16,numSearchThreads=4",
            "-override-config",
            "reportAnalysisWinratesAs=BLACK",
            "--last",
            "last value",
        ]
        .map(str::to_owned)
        .into(),
        ..EngineProfile::default()
    };
    let TransportSpec::LocalProcess { args, .. } = profile.transport().unwrap();
    assert_eq!(
        args.iter().filter(|arg| *arg == "-override-config").count(),
        1
    );
    assert_eq!(
        &args[5..],
        [
            "-override-config",
            "numSearchThreads=2,nnCacheSizePowerOfTwo=20,nnMutexPoolSizePowerOfTwo=16,numSearchThreads=4,numAnalysisThreads=2,reportAnalysisWinratesAs=BLACK",
            "--first",
            "first value with spaces",
            "--middle",
            "--last",
            "last value"
        ]
        .map(OsString::from)
    );
}

#[test]
fn transport_without_user_overrides_still_sets_black_once() {
    let TransportSpec::LocalProcess { args, .. } = EngineProfile::default().transport().unwrap();
    assert_eq!(
        &args[5..],
        [
            "-override-config",
            "numAnalysisThreads=2,reportAnalysisWinratesAs=BLACK"
        ]
        .map(OsString::from)
    );
}

#[test]
fn extra_arguments_cannot_override_profile_paths_or_black_perspective() {
    for args in [
        vec!["-config", "elsewhere"],
        vec!["-model=elsewhere"],
        vec!["-override-config", "reportAnalysisWinratesAs=WHITE"],
        vec!["-override-config=reportAnalysisWinratesAs=SIDETOMOVE"],
        vec![
            "-override-config",
            "numSearchThreads=2, reportAnalysisWinratesAs = WHITE",
        ],
        vec!["-override-config"],
        vec!["-override-config", "badentry"],
        vec!["-override-config", "empty="],
        vec!["-override-config="],
        vec!["-override-config", "=missing_key"],
        vec!["-override-config", "valid=1,"],
        vec![
            "-override-config",
            "valid=1",
            "-override-config",
            "reportAnalysisWinratesAs=WHITE",
        ],
    ] {
        let profile = EngineProfile {
            extra_args: args.into_iter().map(str::to_owned).collect(),
            ..EngineProfile::default()
        };
        assert!(matches!(
            profile.transport(),
            Err(SettingsError::InvalidProfile(_))
        ));
    }
}

#[tokio::test]
async fn fox_history_load_normalizes_identity_duplicates_invalid_uids_and_capacity() {
    let directory = tempfile::tempdir().unwrap();
    let mut history = vec![
        crate::fox::FoxAccount {
            uid: "3".into(),
            nickname: "new name".into(),
        },
        crate::fox::FoxAccount {
            uid: "3".into(),
            nickname: "old name".into(),
        },
        crate::fox::FoxAccount {
            uid: "".into(),
            nickname: "empty".into(),
        },
        crate::fox::FoxAccount {
            uid: "４".into(),
            nickname: "non-ASCII".into(),
        },
    ];
    history.extend((1..=20).map(|uid| crate::fox::FoxAccount {
        uid: uid.to_string(),
        nickname: format!("name-{uid}"),
    }));
    fs::write(
        directory.path().join("settings.json"),
        serde_json::to_vec(&Settings {
            fox_history: history,
            ..Settings::default()
        })
        .unwrap(),
    )
    .unwrap();
    let settings = Settings::load(directory.path()).await.unwrap();
    assert_eq!(settings.fox_history.len(), 10);
    assert_eq!(settings.fox_history[0].uid, "3");
    assert_eq!(settings.fox_history[0].nickname, "new name");
    let uids: Vec<_> = settings
        .fox_history
        .iter()
        .map(|a| a.uid.as_str())
        .collect();
    assert_eq!(uids, ["3", "1", "2", "4", "5", "6", "7", "8", "9", "10"]);
}

#[test]
fn transport_reserves_two_search_slots_even_with_single_thread_override() {
    let profile = EngineProfile {
        extra_args: vec!["-override-config=numAnalysisThreads=1".into()],
        ..EngineProfile::default()
    };
    let TransportSpec::LocalProcess { args, .. } = profile.transport().expect("valid profile");
    assert_eq!(
        args[6],
        "numAnalysisThreads=2,reportAnalysisWinratesAs=BLACK"
    );
}
