#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::engine_setup::CatalogModel;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path, time::Duration};
use tempfile::TempDir;

const CONFIG: &str = "numSearchThreadsPerAnalysisThread = 2\nreportAnalysisWinratesAs = BLACK\n";

fn directory() -> TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    std::fs::create_dir_all(&root).unwrap();
    tempfile::tempdir_in(root).unwrap()
}

fn profile(directory: &Path, program: PathBuf) -> EngineProfile {
    let model = directory.join("test.bin.gz");
    let config = directory.join("analysis.cfg");
    std::fs::write(&model, b"tiny fake model").unwrap();
    std::fs::write(&config, CONFIG).unwrap();
    EngineProfile {
        program,
        model,
        config,
        extra_args: Vec::new(),
        canonical_model_name: None,
    }
}

fn status(report: &EngineDiagnostics, kind: DiagnosticKind) -> DiagnosticStatus {
    report
        .checks
        .iter()
        .find(|check| check.kind == kind)
        .unwrap()
        .status
}

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("safety deadline")
}

// fake-katago deliberately has no `version` CLI subcommand. This injected
// version probe still executes the real fake process and its JSON handshake.
async fn fake_version(profile: EngineProfile) -> Result<String, String> {
    let transport = profile.transport().map_err(|error| error.to_string())?;
    let handle = engine_session::spawn(
        transport,
        engine_session::SessionConfig {
            max_restarts: 0,
            ..Default::default()
        },
    );
    let mut status = handle.status();
    let result = loop {
        let current = status.borrow_and_update().clone();
        match current {
            engine_session::EngineStatus::Ready { version } => break Ok(version),
            engine_session::EngineStatus::Failed { reason, .. } => break Err(reason),
            engine_session::EngineStatus::Stopped => break Err("unexpected stop".into()),
            _ => {}
        }
        status.changed().await.map_err(|error| error.to_string())?;
    };
    handle.shutdown().await.map_err(|error| error.to_string())?;
    result
}

#[tokio::test]
async fn all_path_checks_pass_and_version_comes_from_fake_engine() {
    let binary = fake_katago::binary_path().unwrap();
    let directory = directory();
    let profile = profile(directory.path(), binary);
    let probe_profile = profile.clone();
    let report = within(diagnose_with_probe(
        Some(profile.clone()),
        RepairResources::default(),
        move |_| fake_version(probe_profile),
    ))
    .await
    .unwrap();
    assert_eq!(report.profile, Some(profile));
    assert_eq!(report.checks.len(), 7);
    assert!(
        report
            .checks
            .iter()
            .all(|item| item.status == DiagnosticStatus::Ok)
    );
    assert_eq!(report.checks[2].detail.as_deref(), Some("1.16.5-fake"));
    assert!(
        report.checks[6]
            .detail
            .as_deref()
            .unwrap()
            .contains("startup validates")
    );
}

#[tokio::test]
async fn missing_engine_is_empty_and_each_missing_path_has_an_error() {
    assert!(
        diagnose(None, RepairResources::default())
            .await
            .unwrap()
            .checks
            .is_empty()
    );
    let directory = directory();
    let missing = directory.path().join("missing");
    let profile = EngineProfile {
        program: missing.clone(),
        model: missing.clone(),
        config: missing,
        extra_args: Vec::new(),
        canonical_model_name: None,
    };
    let report = diagnose_with_probe(Some(profile), RepairResources::default(), |_| async {
        panic!("missing program must not be started")
    })
    .await
    .unwrap();
    for check in &report.checks {
        assert_eq!(
            check.status,
            if check.kind == DiagnosticKind::ProgramVersion {
                DiagnosticStatus::Skipped
            } else {
                DiagnosticStatus::Error
            }
        );
    }
}

#[tokio::test]
async fn fake_startup_failure_is_a_version_error_and_other_checks_remain_available() {
    let binary = fake_katago::binary_path().unwrap();
    let directory = directory();
    let mut profile = profile(directory.path(), binary);
    profile.extra_args.push("--fail-startup".into());
    let probe_profile = profile.clone();
    let report = within(diagnose_with_probe(
        Some(profile),
        RepairResources::default(),
        move |_| fake_version(probe_profile),
    ))
    .await
    .unwrap();
    assert_eq!(
        status(&report, DiagnosticKind::ProgramVersion),
        DiagnosticStatus::Error
    );
    assert_eq!(
        status(&report, DiagnosticKind::ProgramExecutable),
        DiagnosticStatus::Ok
    );
    assert_eq!(
        status(&report, DiagnosticKind::ModelReadable),
        DiagnosticStatus::Ok
    );
    assert_eq!(
        status(&report, DiagnosticKind::ConfigValid),
        DiagnosticStatus::Ok
    );
}

#[tokio::test]
async fn production_probe_reports_unrecognized_version_from_fake_cli() {
    let binary = fake_katago::binary_path().unwrap();
    let directory = directory();
    let profile = profile(directory.path(), binary);
    let report = within(diagnose(Some(profile), RepairResources::default()))
        .await
        .unwrap();
    assert_eq!(
        status(&report, DiagnosticKind::ProgramVersion),
        DiagnosticStatus::Error
    );
    assert!(
        report.checks[2]
            .detail
            .as_deref()
            .unwrap()
            .contains("version check")
    );
}

#[tokio::test]
async fn missing_program_loader_is_an_execution_error_even_with_executable_permission() {
    let directory = directory();
    let program = directory.path().join("invalid.exe");
    // Plain text can be executed by the platform's shell fallback after
    // ENOEXEC. A shebang with an absent interpreter fails at process creation
    // on Unix; Windows rejects the same file as an invalid executable image.
    std::fs::write(
        &program,
        format!(
            "#!{}\n",
            directory.path().join("missing-interpreter").display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let report = within(diagnose(
        Some(profile(directory.path(), program)),
        RepairResources::default(),
    ))
    .await
    .unwrap();
    assert_eq!(
        status(&report, DiagnosticKind::ProgramExists),
        DiagnosticStatus::Ok
    );
    assert_eq!(
        status(&report, DiagnosticKind::ProgramExecutable),
        DiagnosticStatus::Error
    );
    assert_eq!(
        status(&report, DiagnosticKind::ProgramVersion),
        DiagnosticStatus::Error
    );
}

#[test]
fn directories_and_empty_model_are_not_readable_files() {
    let directory = directory();
    let mut profile = profile(directory.path(), directory.path().into());
    profile.config = directory.path().into();
    std::fs::write(&profile.model, b"").unwrap();
    let report = file_checks(
        profile,
        RepairResources::default(),
        &ModelCatalog::default(),
    );
    assert_eq!(
        status(&report, DiagnosticKind::ProgramExists),
        DiagnosticStatus::Error
    );
    assert_eq!(
        status(&report, DiagnosticKind::ProgramExecutable),
        DiagnosticStatus::Error
    );
    assert_eq!(
        status(&report, DiagnosticKind::ModelExists),
        DiagnosticStatus::Ok
    );
    assert_eq!(
        status(&report, DiagnosticKind::ModelReadable),
        DiagnosticStatus::Error
    );
    assert_eq!(
        status(&report, DiagnosticKind::ConfigExists),
        DiagnosticStatus::Error
    );
    assert_eq!(
        status(&report, DiagnosticKind::ConfigValid),
        DiagnosticStatus::Error
    );
}

#[cfg(unix)]
#[test]
fn program_without_execution_permission_is_not_started() {
    use std::os::unix::fs::PermissionsExt;
    let directory = directory();
    let program = directory.path().join("not-executable");
    std::fs::write(&program, b"not a program").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o600)).unwrap();
    let profile = profile(directory.path(), program);
    let report = file_checks(
        profile,
        RepairResources::default(),
        &ModelCatalog::default(),
    );
    assert_eq!(
        status(&report, DiagnosticKind::ProgramExists),
        DiagnosticStatus::Ok
    );
    assert_eq!(
        status(&report, DiagnosticKind::ProgramExecutable),
        DiagnosticStatus::Error
    );
    assert_eq!(
        status(&report, DiagnosticKind::ProgramVersion),
        DiagnosticStatus::Skipped
    );
}

#[test]
fn configuration_checks_reject_corruption_and_accept_bundled_template() {
    let directory = directory();
    let path = directory.path().join("analysis.cfg");
    for content in [
        "",
        "broken",
        "numSearchThreads = 0",
        "numSearchThreads = NaN",
        "numSearchThreads = 1\nnumAnalysisThreads = -1",
        "numSearchThreads = 1\nnumSearchThreads = 2",
        "numSearchThreads = 1\nnumSearchThreadsPerAnalysisThread = 2",
        "numSearchThreads = 1\nreportAnalysisWinratesAs = INVALID",
        "numSearchThreads = 1\npath = \"unterminated",
        "numSearchThreads = 1\ninvalid.key = 1",
        "numSearchThreads = 1\npath = \"\"",
        "numSearchThreads = 1\npath = \"value\" unexpected",
        "numSearchThreads = 1\npath = \0",
    ] {
        std::fs::write(&path, content).unwrap();
        assert!(config::check(&path).is_err(), "{content:?}");
    }
    std::fs::write(&path, [0xff]).unwrap();
    assert!(config::check(&path).unwrap_err().contains("UTF-8"));
    std::fs::write(&path, vec![b' '; 1024 * 1024 + 1]).unwrap();
    assert!(config::check(&path).unwrap_err().contains("1 MiB"));
    std::fs::write(
        &path,
        format!("{CONFIG}avoidSgfPatternDirs = \"path#with\\\"quote\" # comment\n"),
    )
    .unwrap();
    assert!(config::check(&path).is_ok());
    let bundled = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../resources/katago/analysis.cfg");
    assert!(config::check(&bundled).is_ok());
    std::fs::write(&path, "numSearchThreads = \"2\"\ncustom-key = value\n").unwrap();
    assert!(config::check(&path).is_ok());
    std::fs::write(
        &path,
        "numSearchThreads = 2\nreportAnalysisWinratesAs = b\n",
    )
    .unwrap();
    assert!(config::check(&path).is_ok());
}

#[test]
fn included_configuration_is_reported_as_unverified_instead_of_invalid() {
    let directory = directory();
    let profile = profile(directory.path(), directory.path().join("unused"));
    std::fs::write(&profile.config, "@include \"backend.cfg\"\n").unwrap();
    let report = file_checks(
        profile,
        RepairResources::default(),
        &ModelCatalog::default(),
    );
    assert_eq!(
        status(&report, DiagnosticKind::ConfigExists),
        DiagnosticStatus::Ok
    );
    assert_eq!(
        status(&report, DiagnosticKind::ConfigValid),
        DiagnosticStatus::Skipped
    );
    assert_eq!(
        report.checks[6].detail.as_deref(),
        Some(config::NEEDS_ENGINE_VALIDATION)
    );
}

fn tiny_catalog(bytes: &[u8]) -> ModelCatalog {
    ModelCatalog {
        default_model_id: "test".into(),
        models: BTreeMap::from([(
            "test".into(),
            CatalogModel {
                file_name: "canonical.bin.gz".into(),
                size_bytes: bytes.len() as u64,
                sha256: format!("{:x}", Sha256::digest(bytes)),
                tier: "strongest".into(),
                published_at: None,
                minimum_katago_version: "1.16.5".into(),
            },
        )]),
    }
}

#[tokio::test]
async fn each_repair_prepares_candidate_without_overwriting_original_files() {
    let binary = fake_katago::binary_path().unwrap();
    let directory = directory();
    let profile = profile(directory.path(), directory.path().join("missing-program"));
    let original = std::fs::read(&profile.config).unwrap();
    let template = directory.path().join("template.cfg");
    std::fs::write(&template, format!("{CONFIG}maxVisits = 100\n")).unwrap();
    let model = directory.path().join("bundled.bin.gz");
    let bytes = b"pinned tiny weights";
    std::fs::write(&model, bytes).unwrap();
    let catalog = tiny_catalog(bytes);
    let resources = RepairResources {
        program: Some(binary.clone()),
        model: Some(model.clone()),
        config_template: Some(template.clone()),
    };
    let checked = file_checks(profile.clone(), resources.clone(), &catalog);
    assert!(
        checked.checks[0]
            .repairs
            .contains(&DiagnosticRepair::UseBundledProgram)
    );
    assert!(
        checked.checks[3]
            .repairs
            .contains(&DiagnosticRepair::UseBundledModel)
    );
    assert!(
        checked.checks[5]
            .repairs
            .contains(&DiagnosticRepair::RegenerateConfig)
    );
    let repaired = prepare_repair(
        profile.clone(),
        DiagnosticRepair::RegenerateConfig,
        resources.clone(),
        directory.path().into(),
    )
    .await
    .unwrap();
    assert_ne!(repaired.config, profile.config);
    assert_eq!(std::fs::read(&profile.config).unwrap(), original);
    assert_eq!(
        std::fs::read(&repaired.config).unwrap(),
        std::fs::read(&template).unwrap()
    );
    let repeated = prepare_repair(
        profile.clone(),
        DiagnosticRepair::RegenerateConfig,
        resources.clone(),
        directory.path().into(),
    )
    .await
    .unwrap();
    assert_eq!(repeated.config, repaired.config);
    let program = prepare_repair(
        profile.clone(),
        DiagnosticRepair::UseBundledProgram,
        resources.clone(),
        directory.path().into(),
    )
    .await
    .unwrap();
    assert_eq!(program.program, binary);
    assert_eq!(program.model, profile.model);
    assert_eq!(within(fake_version(program)).await.unwrap(), "1.16.5-fake");
    let model = repair_with_catalog(
        profile.clone(),
        DiagnosticRepair::UseBundledModel,
        resources,
        directory.path().into(),
        &catalog,
    )
    .unwrap();
    assert_eq!(model.model, directory.path().join("bundled.bin.gz"));
    assert_eq!(
        model.canonical_model_name.as_deref(),
        Some("canonical.bin.gz")
    );
    assert_eq!(model.program, profile.program);
}

#[tokio::test]
async fn repair_rejects_missing_corrupt_resources_and_modified_managed_config() {
    let directory = directory();
    let profile = profile(directory.path(), directory.path().join("missing"));
    for repair in [
        DiagnosticRepair::RegenerateConfig,
        DiagnosticRepair::UseBundledProgram,
        DiagnosticRepair::UseBundledModel,
    ] {
        assert!(
            prepare_repair(
                profile.clone(),
                repair,
                RepairResources::default(),
                directory.path().into()
            )
            .await
            .is_err()
        );
    }
    let bytes = b"pinned data";
    let model = directory.path().join("bundled.bin.gz");
    std::fs::write(&model, b"broken data").unwrap();
    let resources = RepairResources {
        model: Some(model),
        config_template: Some(profile.config.clone()),
        ..Default::default()
    };
    assert!(
        repair_with_catalog(
            profile.clone(),
            DiagnosticRepair::UseBundledModel,
            resources.clone(),
            directory.path().into(),
            &tiny_catalog(bytes)
        )
        .is_err()
    );
    let prepared = prepare_repair(
        profile.clone(),
        DiagnosticRepair::RegenerateConfig,
        resources.clone(),
        directory.path().into(),
    )
    .await
    .unwrap();
    std::fs::write(&prepared.config, format!("{CONFIG}maxVisits = 900\n")).unwrap();
    assert!(
        prepare_repair(
            profile.clone(),
            DiagnosticRepair::RegenerateConfig,
            resources,
            directory.path().into()
        )
        .await
        .is_err()
    );
    assert!(
        std::fs::read_to_string(prepared.config)
            .unwrap()
            .contains("900")
    );
    assert_eq!(std::fs::read_to_string(profile.config).unwrap(), CONFIG);
}
