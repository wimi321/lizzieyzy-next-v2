#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{
    AppConfig, AppCore, DiagnosticKind, DiagnosticRepair, DiagnosticStatus, EngineDiagnostics,
    EngineProfile, RepairResources, Settings,
};
use std::{
    future::Future,
    path::{Path, PathBuf},
    time::Duration,
};

const CONFIG: &str = "numSearchThreadsPerAnalysisThread = 2\nreportAnalysisWinratesAs = BLACK\n";

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("safety deadline")
}

fn directory() -> tempfile::TempDir {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    std::fs::create_dir_all(&path).unwrap();
    tempfile::tempdir_in(path).unwrap()
}

async fn core(directory: &Path, binary: PathBuf) -> (AppCore, EngineProfile) {
    let model = directory.join("model.bin.gz");
    let config_path = directory.join("original.cfg");
    std::fs::write(&model, b"tiny local fake weights").unwrap();
    std::fs::write(&config_path, CONFIG).unwrap();
    let profile = EngineProfile {
        program: binary,
        model,
        config: config_path,
        extra_args: vec!["--visits-per-sec".into(), "100000".into()],
        canonical_model_name: None,
    };
    let mut config = AppConfig::default();
    config.analysis.live.max_visits = 1;
    config.session.write_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(60);
    let (core, _output) = within(crate::spawn_with_config(directory, config))
        .await
        .unwrap();
    let settings = within(core.add_engine("Main".into(), profile.clone()))
        .await
        .unwrap();
    within(core.select_main_engine(settings.engines[0].id.clone()))
        .await
        .unwrap();
    (core, profile)
}

fn check(report: &EngineDiagnostics, kind: DiagnosticKind) -> DiagnosticStatus {
    report
        .checks
        .iter()
        .find(|item| item.kind == kind)
        .unwrap()
        .status
}

#[tokio::test]
async fn public_repair_commands_commit_candidates_and_return_fresh_diagnostics() {
    let binary = fake_katago::binary_path().unwrap();
    let directory = directory();
    let alternate = directory.path().join("bundled-katago.exe");
    // A hard link exercises a different selected program path without copying
    // or downloading an engine executable on disk-constrained test hosts.
    std::fs::hard_link(&binary, &alternate).unwrap();
    let template = directory.path().join("template.cfg");
    std::fs::write(&template, format!("{CONFIG}maxVisits = 100\n")).unwrap();
    let (core, original) = core(directory.path(), binary).await;
    let resources = RepairResources {
        program: Some(alternate.clone()),
        config_template: Some(template),
        ..Default::default()
    };
    let report = within(core.repair_engine(DiagnosticRepair::RegenerateConfig, resources.clone()))
        .await
        .unwrap();
    let repaired = report.profile.as_ref().unwrap();
    assert_ne!(repaired.config, original.config);
    assert_eq!(
        check(&report, DiagnosticKind::ConfigValid),
        DiagnosticStatus::Ok
    );
    assert_eq!(std::fs::read_to_string(&original.config).unwrap(), CONFIG);
    assert_eq!(
        within(core.settings()).await.unwrap().engine.as_ref(),
        Some(repaired)
    );
    let report = within(core.repair_engine(DiagnosticRepair::UseBundledProgram, resources.clone()))
        .await
        .unwrap();
    assert_eq!(report.profile.as_ref().unwrap().program, alternate);
    assert_eq!(
        check(&report, DiagnosticKind::ProgramExecutable),
        DiagnosticStatus::Ok
    );
    assert_eq!(
        check(&report, DiagnosticKind::ModelReadable),
        DiagnosticStatus::Ok
    );
    assert_eq!(
        check(&report, DiagnosticKind::ConfigValid),
        DiagnosticStatus::Ok
    );
    // fake-katago's version comes through JSON, not the optional CLI subcommand.
    assert_eq!(
        check(&report, DiagnosticKind::ProgramVersion),
        DiagnosticStatus::Error
    );
    let settings = within(core.settings()).await.unwrap();
    assert_eq!(Settings::load(directory.path()).await.unwrap(), settings);
    within(core.close_engine()).await.unwrap();
    let closed = within(core.diagnose_engine(resources)).await.unwrap();
    assert_eq!(closed.profile, settings.engine);
    assert!(!closed.checks.is_empty());
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn failed_repair_trial_keeps_main_engine_and_saved_settings() {
    let binary = fake_katago::binary_path().unwrap();
    let directory = directory();
    let (core, original) = core(directory.path(), binary).await;
    let bad_program = directory.path().join("invalid.exe");
    std::fs::write(&bad_program, b"invalid executable").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bad_program, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let before = within(core.settings()).await.unwrap();
    let on_disk = std::fs::read(directory.path().join("settings.json")).unwrap();
    let result = within(core.repair_engine(
        DiagnosticRepair::UseBundledProgram,
        RepairResources {
            program: Some(bad_program),
            ..Default::default()
        },
    ))
    .await;
    assert!(result.is_err());
    assert_eq!(within(core.settings()).await.unwrap(), before);
    assert_eq!(
        std::fs::read(directory.path().join("settings.json")).unwrap(),
        on_disk
    );
    assert_eq!(
        within(core.diagnose_engine(RepairResources::default()))
            .await
            .unwrap()
            .profile,
        Some(original)
    );
    // The retained fake session remains usable after candidate failure.
    within(core.set_pondering(false)).await.unwrap();
    within(core.set_pondering(true)).await.unwrap();
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn public_repairs_reject_no_main_missing_resources_and_unpinned_model() {
    let binary = fake_katago::binary_path().unwrap();
    let directory = directory();
    let (empty, _output) = within(crate::spawn(directory.path().join("empty")))
        .await
        .unwrap();
    assert!(
        within(empty.diagnose_engine(RepairResources::default()))
            .await
            .unwrap()
            .checks
            .is_empty()
    );
    assert!(
        within(empty.repair_engine(
            DiagnosticRepair::RegenerateConfig,
            RepairResources::default()
        ))
        .await
        .is_err()
    );
    within(empty.shutdown()).await.unwrap();
    let (core, profile) = core(directory.path(), binary).await;
    let before = within(core.settings()).await.unwrap();
    for repair in [
        DiagnosticRepair::RegenerateConfig,
        DiagnosticRepair::UseBundledProgram,
        DiagnosticRepair::UseBundledModel,
    ] {
        assert!(
            within(core.repair_engine(repair, RepairResources::default()))
                .await
                .is_err()
        );
    }
    assert!(
        within(core.repair_engine(
            DiagnosticRepair::UseBundledModel,
            RepairResources {
                model: Some(profile.model),
                ..Default::default()
            }
        ))
        .await
        .is_err()
    );
    assert_eq!(within(core.settings()).await.unwrap(), before);
    within(core.shutdown()).await.unwrap();
}
