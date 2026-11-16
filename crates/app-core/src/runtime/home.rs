//! Per-user KataGo state belongs outside the verified engine installation.
use crate::AppError;
use std::{ffi::OsString, path::Path};

/// The profile transport has already validated and merged user overrides.
/// Add defaults only: an explicit empty value is also the user's choice.
pub(crate) async fn apply(args: &mut Vec<OsString>, data_root: &Path) -> Result<(), AppError> {
    let index = args.iter().position(|arg| arg == "-override-config");
    let mut entries = match index {
        Some(index) => args
            .get(index + 1)
            .and_then(|arg| arg.to_str())
            .ok_or_else(|| crate::engines::invalid("invalid engine override argument"))?
            .split(',')
            .filter(|entry| !entry.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>(),
        None => Vec::new(),
    };
    if !contains(&entries, "homeDataDir") {
        let directory = data_root.join("katago-home");
        if let Some(value) = override_path(&directory) {
            tokio::fs::create_dir_all(&directory).await?;
            entries.push(format!("homeDataDir={value}"));
        } else {
            tracing::warn!(
                "KataGo homeDataDir omitted: application data path cannot be represented in an override"
            );
        }
    }
    if !contains(&entries, "logToStderr") {
        entries.push("logToStderr=true".into());
    }
    let value = OsString::from(entries.join(","));
    if let Some(index) = index {
        args[index + 1] = value;
    } else {
        args.extend([OsString::from("-override-config"), value]);
    }
    Ok(())
}

fn contains(entries: &[String], key: &str) -> bool {
    entries.iter().any(|entry| {
        entry
            .split_once('=')
            .is_some_and(|(name, _)| name.trim().eq_ignore_ascii_case(key))
    })
}

fn override_path(path: &Path) -> Option<&str> {
    path.to_str()
        .filter(|value| !value.contains([',', '=', '\r', '\n', '\0']))
}

#[cfg(test)]
mod tests;
