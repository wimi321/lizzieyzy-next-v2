use crate::AppError;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const LIMIT: u64 = 1024 * 1024;
pub(super) const NEEDS_ENGINE_VALIDATION: &str =
    "configuration includes require engine validation; included files were not inspected";

pub(super) fn check(path: &Path) -> Result<(), String> {
    read(path).map(|_| ())
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    super::regular_file(path)?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(LIMIT + 1).read_to_end(&mut bytes))
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > LIMIT {
        return Err("analysis configuration exceeds 1 MiB".into());
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| "analysis configuration is not UTF-8")?;
    validate(text)?;
    Ok(bytes)
}

fn validate(text: &str) -> Result<(), String> {
    let mut values = BTreeMap::new();
    let mut includes = false;
    for (index, line) in text.trim_start_matches('\u{feff}').lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.contains('\0') {
            return Err("configuration contains NUL".into());
        }
        if line.starts_with('@') {
            let directive = line.split('#').next().unwrap_or_default().trim();
            let separator =
                directive.find(|character: char| character.is_whitespace() || character == '=');
            if let Some(separator) = separator
                && &directive[..separator] == "@include"
                && !directive[separator..]
                    .trim_matches(|character: char| {
                        character.is_whitespace() || matches!(character, '=' | '\'' | '"')
                    })
                    .is_empty()
            {
                includes = true;
                continue;
            }
            return Err(format!("line {}: invalid include directive", index + 1));
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("line {}: expected key = value", index + 1))?;
        let key = key.trim();
        let value =
            parse_value(value.trim()).map_err(|reason| format!("line {}: {reason}", index + 1))?;
        if key.is_empty()
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
            || value.is_empty()
        {
            return Err(format!("line {}: invalid key or empty value", index + 1));
        }
        if values.insert(key, value).is_some() {
            return Err(format!("duplicate configuration key: {key}"));
        }
    }
    if includes {
        return Err(NEEDS_ENGINE_VALIDATION.into());
    }
    let search = values
        .get("numSearchThreadsPerAnalysisThread")
        .or_else(|| values.get("numSearchThreads"));
    if search.is_none() {
        return Err(
            "analysis configuration needs numSearchThreadsPerAnalysisThread or numSearchThreads"
                .into(),
        );
    }
    if values.contains_key("numSearchThreadsPerAnalysisThread")
        && values.contains_key("numSearchThreads")
    {
        return Err(
            "numSearchThreadsPerAnalysisThread and numSearchThreads are aliases; specify only one"
                .into(),
        );
    }
    for key in [
        "numSearchThreadsPerAnalysisThread",
        "numSearchThreads",
        "numAnalysisThreads",
        "nnMaxBatchSize",
        "numNNServerThreadsPerModel",
    ] {
        if let Some(value) = values.get(key)
            && !value.parse::<u32>().is_ok_and(|number| number > 0)
        {
            return Err(format!("{key} must be a positive integer"));
        }
    }
    if let Some(value) = values.get("reportAnalysisWinratesAs")
        && !["black", "white", "b", "w", "sidetomove"]
            .contains(&value.to_ascii_lowercase().as_str())
    {
        return Err("reportAnalysisWinratesAs must be BLACK, WHITE or SIDETOMOVE".into());
    }
    Ok(())
}

// Mirrors the upstream config parser's quoted values, escaping and comments.
// https://github.com/lightvector/KataGo/blob/master/cpp/core/config_parser.cpp
fn parse_value(value: &str) -> Result<String, &'static str> {
    let Some(quoted) = value.strip_prefix('"') else {
        return Ok(value
            .split('#')
            .next()
            .unwrap_or_default()
            .trim()
            .to_owned());
    };
    let mut result = String::new();
    let mut escaped = false;
    for (index, character) in quoted.char_indices() {
        if escaped {
            escaped = false;
            result.push(character);
            continue;
        }
        match character {
            '\\' => escaped = true,
            '"' => {
                let remainder = quoted[index + 1..].trim();
                if !remainder.is_empty() && !remainder.starts_with('#') {
                    return Err("unexpected content after quoted value");
                }
                return Ok(result);
            }
            _ => result.push(character),
        }
    }
    Err("unterminated quoted value")
}

pub(super) fn regenerate(source: &Path, directory: &Path) -> Result<PathBuf, AppError> {
    let bytes = read(source).map_err(|reason| crate::engines::invalid(&reason))?;
    let directory = directory.join("engine-configs");
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(format!("analysis-{:x}.cfg", Sha256::digest(&bytes)));
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_file() => {
            let existing = read(&path).map_err(|reason| crate::engines::invalid(&reason))?;
            if existing == bytes {
                return Ok(path);
            }
            return Err(crate::engines::invalid(
                "generated configuration was modified; it will not be overwritten",
            )
            .into());
        }
        Ok(_) => {
            return Err(crate::engines::invalid(
                "generated configuration target is not a regular file",
            )
            .into());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    temporary.write_all(&bytes)?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary
        .persist_noclobber(&path)
        .map_err(|error| error.error)?;
    Ok(path)
}
