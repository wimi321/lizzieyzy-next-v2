//! Read-only checks and immutable repair candidates. The caller must apply a
//! candidate with the existing P2-11 engine transaction before reporting success.

use crate::{AppError, EngineProfile, engine_setup::catalog::ModelCatalog};
use serde::{Deserialize, Serialize};
use std::{future::Future, io::Read, path::PathBuf};
use ts_rs::TS;

mod config;

/// Shell-owned resource locations, never supplied by an IPC caller.
#[derive(Clone, Debug, Default)]
pub struct RepairResources {
    pub program: Option<PathBuf>,
    pub model: Option<PathBuf>,
    pub config_template: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum DiagnosticRepair {
    InstallRuntime,
    RegenerateConfig,
    UseBundledProgram,
    UseBundledModel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum DiagnosticKind {
    RuntimeComponents,
    ProgramExists,
    ProgramExecutable,
    ProgramVersion,
    ModelExists,
    ModelReadable,
    ConfigExists,
    ConfigValid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum DiagnosticStatus {
    Ok,
    Error,
    Skipped,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DiagnosticCheck {
    #[ts(as = "Option<DiagnosticKind>", optional)]
    pub kind: DiagnosticKind,
    #[ts(as = "Option<DiagnosticStatus>", optional)]
    pub status: DiagnosticStatus,
    #[ts(optional = nullable)]
    pub path: Option<PathBuf>,
    #[ts(optional = nullable)]
    pub detail: Option<String>,
    #[ts(as = "Option<Vec<DiagnosticRepair>>", optional)]
    pub repairs: Vec<DiagnosticRepair>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct EngineDiagnostics {
    #[ts(optional = nullable)]
    pub profile: Option<EngineProfile>,
    #[ts(as = "Option<Vec<DiagnosticCheck>>", optional)]
    pub checks: Vec<DiagnosticCheck>,
}

pub(crate) async fn diagnose(
    profile: Option<EngineProfile>,
    resources: RepairResources,
) -> Result<EngineDiagnostics, AppError> {
    diagnose_with_runtime(profile, resources, None).await
}

pub(crate) async fn diagnose_with_runtime(
    profile: Option<EngineProfile>,
    resources: RepairResources,
    runtime: Option<crate::assets::RuntimeInstallation>,
) -> Result<EngineDiagnostics, AppError> {
    // Even a `version` process loads DLLs. Never execute an identified GPU
    // engine against missing, damaged or unverified adjacent components.
    let environment = runtime
        .as_ref()
        .map(crate::runtime::environment)
        .transpose()
        .map_err(|error| error.to_string());
    let mut report = diagnose_with_probe(profile, resources, |program| async move {
        let environment = environment?.unwrap_or_default();
        crate::auto_setup::version::probe_with_environment(
            &program,
            environment.into_iter().collect(),
        )
        .await
    })
    .await?;
    if report.profile.is_some() {
        report
            .checks
            .push(crate::runtime::diagnostic(runtime.as_ref()));
    }
    Ok(report)
}

async fn diagnose_with_probe<F, Fut>(
    profile: Option<EngineProfile>,
    resources: RepairResources,
    probe: F,
) -> Result<EngineDiagnostics, AppError>
where
    F: FnOnce(PathBuf) -> Fut,
    Fut: Future<Output = Result<String, String>>,
{
    let Some(current) = profile.clone() else {
        return Ok(EngineDiagnostics {
            profile,
            checks: Vec::new(),
        });
    };
    let mut report = tokio::task::spawn_blocking(move || {
        file_checks(current, resources, &ModelCatalog::bundled())
    })
    .await
    .map_err(std::io::Error::other)?;
    if report.checks[1].status == DiagnosticStatus::Ok {
        let version = probe(
            report
                .profile
                .as_ref()
                .map(|p| p.program.clone())
                .unwrap_or_default(),
        )
        .await;
        if let Err(reason) = &version
            && reason.starts_with("cannot start version check:")
        {
            report.checks[1].status = DiagnosticStatus::Error;
            report.checks[1].detail = Some(reason.clone());
        }
        report.checks[2].status = if version.is_ok() {
            DiagnosticStatus::Ok
        } else {
            DiagnosticStatus::Error
        };
        report.checks[2].detail = Some(match version {
            Ok(version) => version,
            Err(error) => error,
        });
    }
    Ok(report)
}

fn file_checks(
    profile: EngineProfile,
    resources: RepairResources,
    catalog: &ModelCatalog,
) -> EngineDiagnostics {
    let program_repair = resources
        .program
        .filter(|path| executable(path).is_ok())
        .map(|_| DiagnosticRepair::UseBundledProgram)
        .into_iter()
        .collect::<Vec<_>>();
    let model_repair = resources
        .model
        .filter(|path| catalog.identify(path).is_some())
        .map(|_| DiagnosticRepair::UseBundledModel)
        .into_iter()
        .collect::<Vec<_>>();
    let config_repair = resources
        .config_template
        .filter(|path| config::check(path).is_ok())
        .map(|_| DiagnosticRepair::RegenerateConfig)
        .into_iter()
        .collect::<Vec<_>>();
    let mut checks = vec![
        checked(
            DiagnosticKind::ProgramExists,
            &profile.program,
            regular_file(&profile.program),
            &program_repair,
        ),
        checked(
            DiagnosticKind::ProgramExecutable,
            &profile.program,
            executable(&profile.program),
            &program_repair,
        ),
        DiagnosticCheck {
            kind: DiagnosticKind::ProgramVersion,
            status: DiagnosticStatus::Skipped,
            path: Some(profile.program.clone()),
            detail: Some("program cannot be executed".into()),
            repairs: program_repair,
        },
        checked(
            DiagnosticKind::ModelExists,
            &profile.model,
            regular_file(&profile.model),
            &model_repair,
        ),
        checked(
            DiagnosticKind::ModelReadable,
            &profile.model,
            readable(&profile.model),
            &model_repair,
        ),
        checked(
            DiagnosticKind::ConfigExists,
            &profile.config,
            regular_file(&profile.config),
            &config_repair,
        ),
        checked(
            DiagnosticKind::ConfigValid,
            &profile.config,
            config::check(&profile.config),
            &config_repair,
        ),
    ];
    // Report what was actually checked, rather than claiming that static checks
    // prove every backend-specific key or that the model can be loaded on a GPU.
    if checks[6].detail.as_deref() == Some(config::NEEDS_ENGINE_VALIDATION) {
        checks[6].status = DiagnosticStatus::Skipped;
    } else if checks[6].status == DiagnosticStatus::Ok {
        checks[6].detail = Some("syntax and required analysis settings checked; engine startup validates backend-specific options".into());
    }
    EngineDiagnostics {
        profile: Some(profile),
        checks,
    }
}

fn checked(
    kind: DiagnosticKind,
    path: &std::path::Path,
    result: Result<(), String>,
    repairs: &[DiagnosticRepair],
) -> DiagnosticCheck {
    DiagnosticCheck {
        kind,
        status: if result.is_ok() {
            DiagnosticStatus::Ok
        } else {
            DiagnosticStatus::Error
        },
        path: Some(path.to_owned()),
        detail: result.err(),
        repairs: repairs.to_vec(),
    }
}

fn regular_file(path: &std::path::Path) -> Result<(), String> {
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.is_file() {
        Ok(())
    } else {
        Err("path is not a regular file".into())
    }
}

fn executable(path: &std::path::Path) -> Result<(), String> {
    regular_file(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::metadata(path)
            .map_err(|error| error.to_string())?
            .permissions()
            .mode()
            & 0o111
            == 0
        {
            return Err("program has no executable permission".into());
        }
    }
    Ok(())
}

fn readable(path: &std::path::Path) -> Result<(), String> {
    regular_file(path)?;
    let mut byte = [0];
    match std::fs::File::open(path).and_then(|mut file| file.read(&mut byte)) {
        Ok(0) => Err("model is empty".into()),
        Ok(_) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

/// This only prepares a candidate: it never changes saved settings or stops an
/// engine. A failed trial may retain the immutable generated configuration.
pub(crate) async fn prepare_repair(
    profile: EngineProfile,
    repair: DiagnosticRepair,
    resources: RepairResources,
    directory: PathBuf,
) -> Result<EngineProfile, AppError> {
    tokio::task::spawn_blocking(move || {
        repair_with_catalog(
            profile,
            repair,
            resources,
            directory,
            &ModelCatalog::bundled(),
        )
    })
    .await
    .map_err(std::io::Error::other)?
}

fn repair_with_catalog(
    mut profile: EngineProfile,
    repair: DiagnosticRepair,
    resources: RepairResources,
    directory: PathBuf,
    catalog: &ModelCatalog,
) -> Result<EngineProfile, AppError> {
    let unavailable =
        || crate::engines::invalid("requested bundled repair resource is unavailable");
    match repair {
        DiagnosticRepair::InstallRuntime => {
            return Err(
                crate::engines::invalid("runtime installation requires the asset actor").into(),
            );
        }
        DiagnosticRepair::RegenerateConfig => {
            let source = resources.config_template.ok_or_else(unavailable)?;
            profile.config = config::regenerate(&source, &directory)?;
        }
        DiagnosticRepair::UseBundledProgram => {
            let program = resources.program.ok_or_else(unavailable)?;
            executable(&program).map_err(|reason| crate::engines::invalid(&reason))?;
            profile.program = program;
        }
        DiagnosticRepair::UseBundledModel => {
            let path = resources.model.ok_or_else(unavailable)?;
            let model = catalog.identify(&path).ok_or_else(|| {
                crate::engines::invalid(
                    "bundled model does not match the pinned catalog SHA-256 and size",
                )
            })?;
            profile.model = path;
            profile.canonical_model_name = Some(model.file_name);
        }
    }
    Ok(profile)
}

#[cfg(test)]
mod tests;
