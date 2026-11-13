//! DLL discovery changes only the selected child's environment. Installed
//! files are verified by the asset service before a directory is used.
use crate::assets::RuntimeInstallation;
use crate::{AppError, DiagnosticCheck, DiagnosticKind, DiagnosticRepair, DiagnosticStatus};
use std::{collections::BTreeMap, ffi::OsString};

pub(crate) fn supported() -> bool {
    cfg!(all(target_os = "windows", target_arch = "x86_64"))
}

impl crate::AppCore {
    pub(crate) fn runtime_supported(&self) -> bool {
        #[cfg(test)]
        {
            self.runtime_platform
        }
        #[cfg(not(test))]
        {
            supported()
        }
    }
}

pub(crate) fn enabled(config: &crate::AppConfig) -> bool {
    #[cfg(test)]
    {
        config.runtime_platform
    }
    #[cfg(not(test))]
    {
        let _ = config;
        supported()
    }
}

pub(crate) fn environment(
    installation: &RuntimeInstallation,
) -> Result<BTreeMap<OsString, OsString>, AppError> {
    if !installation.ready {
        return Err(crate::engines::invalid(&format!(
            "runtime {} is missing or incomplete: {}",
            installation.profile_id,
            installation
                .error
                .as_deref()
                .unwrap_or("download and install the runtime components"),
        ))
        .into());
    }
    let directory = installation
        .directory
        .as_ref()
        .ok_or_else(|| crate::engines::invalid("verified runtime has no directory"))?;
    let mut entries = vec![directory.clone()];
    if let Some(inherited) = std::env::var_os("PATH") {
        for entry in std::env::split_paths(&inherited) {
            if !entry.as_os_str().is_empty() && !entries.contains(&entry) {
                entries.push(entry);
            }
        }
    }
    let path = std::env::join_paths(entries)
        .map_err(|error| crate::engines::invalid(&error.to_string()))?;
    Ok(BTreeMap::from([(OsString::from("PATH"), path)]))
}

pub(crate) fn diagnostic(installation: Option<&RuntimeInstallation>) -> DiagnosticCheck {
    match installation {
        Some(value) => DiagnosticCheck {
            kind: DiagnosticKind::RuntimeComponents,
            status: if value.ready {
                DiagnosticStatus::Ok
            } else {
                DiagnosticStatus::Error
            },
            path: value.directory.clone(),
            detail: Some(if value.ready {
                format!("verified runtime {}", value.profile_id)
            } else {
                format!(
                    "{}: {}; missing: {}",
                    value.profile_id,
                    value.error.as_deref().unwrap_or("runtime is not installed"),
                    value.missing.join(", ")
                )
            }),
            repairs: if value.ready {
                Vec::new()
            } else {
                vec![DiagnosticRepair::InstallRuntime]
            },
        },
        None => DiagnosticCheck {
            kind: DiagnosticKind::RuntimeComponents,
            status: DiagnosticStatus::Skipped,
            path: None,
            detail: Some(
                "managed NVIDIA runtime is not required for this platform or engine".into(),
            ),
            repairs: Vec::new(),
        },
    }
}

#[cfg(test)]
mod tests;
