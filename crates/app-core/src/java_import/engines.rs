use super::{JavaImportedEngine, invalid};
use crate::{AppError, EngineEntry, EngineProfile};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub(super) async fn read(
    directory: &Path,
    leelaz: &Value,
    issues: &mut Vec<String>,
) -> Result<Vec<JavaImportedEngine>, AppError> {
    let entries = if let Some(entries) = leelaz.get("engine-settings-list") {
        entries
            .as_array()
            .ok_or_else(|| invalid("engine-settings-list must be an array"))?
            .clone()
    } else {
        let mut commands = Vec::new();
        if let Some(command) = leelaz.get("engine-command") {
            commands.push(command.clone());
        }
        if let Some(list) = leelaz.get("engine-command-list") {
            commands.extend(
                list.as_array()
                    .ok_or_else(|| invalid("engine-command-list must be an array"))?
                    .clone(),
            );
        }
        commands.into_iter().enumerate().map(|(index, command)| {
            json!({"command": command, "name": leelaz["engine-name-list"][index]})
        }).collect()
    };
    if entries.len() > 256 {
        return Err(invalid("more than 256 engines"));
    }
    let mut imported = Vec::new();
    for (index, value) in entries.iter().enumerate() {
        if !value.is_object() {
            return Err(invalid(format!("engine {index} must be an object")));
        }
        let command = value["command"]
            .as_str()
            .ok_or_else(|| invalid(format!("engine {index} command must be text")))?;
        let mut entry_issues = Vec::new();
        let remote = value["useJavaSSH"].as_bool().unwrap_or(false);
        let profile = parse_profile(directory, command, remote, &mut entry_issues)?;
        for (kind, path) in [
            ("program", &profile.program),
            ("model", &profile.model),
            ("configuration", &profile.config),
        ] {
            if !tokio::fs::metadata(path)
                .await
                .is_ok_and(|meta| meta.is_file())
            {
                entry_issues.push(format!(
                    "{kind} is missing or not a regular file: {}",
                    path.display()
                ));
            }
        }
        let name = value["name"]
            .as_str()
            .filter(|name| !name.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Java engine {}", index + 1));
        let identity =
            serde_json::to_vec(&(directory, index)).map_err(crate::SettingsError::from)?;
        let id = format!("java-{}", blake3::hash(&identity).to_hex());
        imported.push(JavaImportedEngine {
            source_index: index as u32,
            entry: EngineEntry { id, name, profile },
            issues: entry_issues,
        });
    }
    if imported.is_empty() {
        issues.push("no Java engine entries were found".into());
    }
    Ok(imported)
}

pub(super) fn selected_index(ui: &Value, leelaz: &Value) -> Option<u32> {
    let index = |key| ui[key].as_u64().and_then(|value| value.try_into().ok());
    let marked = leelaz["engine-settings-list"]
        .as_array()
        .and_then(|entries| entries.iter().position(|entry| entry["isDefault"] == true))
        .map(|index| index as u32);
    if ui["autoload-default"] == true {
        marked.or_else(|| index("default-engine"))
    } else if ui["autoload-last"] == true {
        index("last-engine")
    } else if ui["autoload-empty"] == true {
        None
    } else {
        index("last-engine")
            .or(marked)
            .or_else(|| index("default-engine"))
    }
}

fn resolve(directory: &Path, text: &str) -> PathBuf {
    if text.is_empty() {
        return PathBuf::new();
    }
    let path = PathBuf::from(text);
    // Do not turn Windows absolute paths into misleading Unix relative paths
    // when importing a configuration copied from another machine.
    if path.is_absolute() || text.as_bytes().get(1) == Some(&b':') || text.starts_with("\\\\") {
        path
    } else {
        directory.join(path)
    }
}

fn parse_profile(
    directory: &Path,
    command: &str,
    remote: bool,
    issues: &mut Vec<String>,
) -> Result<EngineProfile, AppError> {
    let tokens = split_command(command)?;
    let mut profile = EngineProfile::default();
    if let Some(program) = tokens.first() {
        profile.program = resolve(directory, program);
    }
    if remote {
        issues.push(
            "remote SSH engine is unsupported; credentials and remote arguments were not imported"
                .into(),
        );
        // Never copy Java password/key material or turn SSH into a local launch.
        profile.program = PathBuf::new();
        return Ok(profile);
    }
    let mode = tokens.get(1).map(String::as_str);
    if !matches!(mode, Some("analysis" | "gtp")) {
        issues.push(
            "only local KataGo analysis/gtp commands can be converted; arguments were not imported"
                .into(),
        );
        return Ok(profile);
    }
    if mode == Some("gtp") {
        issues.push("GTP command will use the analysis protocol; its original configuration is retained and may need an analysis configuration".into());
    }
    let mut arguments = tokens.iter().skip(2);
    while let Some(argument) = arguments.next() {
        let (flag, inline) = argument
            .split_once('=')
            .map_or((argument.as_str(), None), |(key, value)| (key, Some(value)));
        if matches!(flag, "-model" | "-config") {
            let text = inline
                .or_else(|| arguments.next().map(String::as_str))
                .ok_or_else(|| invalid(format!("{flag} requires a path")))?;
            let target = if flag == "-model" {
                &mut profile.model
            } else {
                &mut profile.config
            };
            if !target.as_os_str().is_empty() {
                return Err(invalid(format!("duplicate {flag} path")));
            }
            *target = resolve(directory, text);
        } else {
            profile.extra_args.push(argument.clone());
        }
    }
    if let Err(error) = profile.transport() {
        issues.push(format!(
            "profile requires correction before launch: {error}"
        ));
    }
    Ok(profile)
}

/// Java Utils.splitCommand quote boundaries, with unmatched quotes rejected.
/// Backslashes are literal (including Windows paths); there is no shell expansion.
pub(super) fn split_command(command: &str) -> Result<Vec<String>, AppError> {
    let mut result = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut started = false;
    for character in command.chars() {
        if let Some(delimiter) = quote {
            if character == delimiter {
                quote = None;
            } else {
                word.push(character);
            }
        } else if character == '\'' || character == '"' {
            quote = Some(character);
            started = true;
        } else if character == ' ' {
            if started {
                result.push(std::mem::take(&mut word));
                started = false;
            }
        } else {
            word.push(character);
            started = true;
        }
    }
    if quote.is_some() {
        return Err(invalid("unclosed command quote"));
    }
    if started {
        result.push(word);
    }
    Ok(result)
}
