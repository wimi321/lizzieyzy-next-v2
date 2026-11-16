//! App-owned metadata receipts for routine startup checks. Explicit diagnosis
//! and installation still reconstruct and hash every file from pinned archives.
use super::{AssetError, runtime_archive};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};
use tokio::sync::watch;

const VERSION: u32 = 1;
const MAX_RECORD_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Deserialize, Serialize)]
struct Record {
    version: u32,
    executable_sha256: String,
    files: BTreeMap<PathBuf, Metadata>,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
struct Metadata {
    size: u64,
    modified_secs: u64,
    modified_nanos: u32,
    executable_bits: u32,
}

pub(super) fn path(directory: &Path) -> Result<PathBuf, AssetError> {
    let name = directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AssetError::Archive("invalid installation generation".into()))?;
    Ok(directory.with_file_name(format!("{name}.verified.json")))
}

fn inventory(
    directory: &Path,
    cancel: &watch::Receiver<bool>,
) -> Result<BTreeMap<PathBuf, Metadata>, AssetError> {
    let mut files = BTreeMap::new();
    let mut pending = vec![directory.to_path_buf()];
    let mut entries = 0;
    while let Some(parent) = pending.pop() {
        runtime_archive::cancelled(cancel)?;
        for entry in fs::read_dir(parent)? {
            runtime_archive::cancelled(cancel)?;
            let entry = entry?;
            entries += 1;
            if entries > 100_000 {
                return Err(AssetError::Archive("too many installed entries".into()));
            }
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                let relative = entry
                    .path()
                    .strip_prefix(directory)
                    .map_err(|_| AssetError::Verification)?
                    .to_path_buf();
                let modified = metadata
                    .modified()?
                    .duration_since(UNIX_EPOCH)
                    .map_err(|_| AssetError::Verification)?;
                #[cfg(unix)]
                let executable_bits = {
                    use std::os::unix::fs::PermissionsExt;
                    metadata.permissions().mode() & 0o111
                };
                #[cfg(not(unix))]
                let executable_bits = 0;
                files.insert(
                    relative,
                    Metadata {
                        size: metadata.len(),
                        modified_secs: modified.as_secs(),
                        modified_nanos: modified.subsec_nanos(),
                        executable_bits,
                    },
                );
            } else {
                return Err(AssetError::Archive("installed link or special file".into()));
            }
        }
    }
    Ok(files)
}

/// Called only after complete archive/file validation or publication of a
/// validated staged generation. The receipt is outside the installed tree.
pub(super) fn write(directory: &Path, cancel: &watch::Receiver<bool>) -> Result<(), AssetError> {
    let files = inventory(directory, cancel)?;
    let mut executable = File::open(directory.join("bin/katago.exe"))?;
    let executable_sha256 = runtime_archive::hash(&mut executable, cancel)?.sha256;
    let record = Record {
        version: VERSION,
        executable_sha256,
        files,
    };
    let destination = path(directory)?;
    let parent = destination.parent().ok_or(AssetError::Verification)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(temporary.as_file_mut(), &record)
        .map_err(|error| AssetError::Catalog(error.to_string()))?;
    temporary.flush()?;
    if temporary.as_file().metadata()?.len() > MAX_RECORD_BYTES {
        return Err(AssetError::Archive(
            "runtime receipt exceeds size limit".into(),
        ));
    }
    temporary.as_file().sync_all()?;
    runtime_archive::cancelled(cancel)?;
    temporary
        .persist(destination)
        .map_err(|error| AssetError::Io(error.error))?;
    Ok(())
}

fn read(directory: &Path, pin: &str) -> Option<Record> {
    let path = path(directory).ok()?;
    let metadata = fs::symlink_metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_RECORD_BYTES {
        return None;
    }
    let record: Record =
        serde_json::from_reader(File::open(path).ok()?.take(MAX_RECORD_BYTES)).ok()?;
    if record.version != VERSION
        || !record.executable_sha256.eq_ignore_ascii_case(pin)
        || !record.files.contains_key(Path::new("bin/katago.exe"))
        || record.files.len() > 100_000
        || record
            .files
            .values()
            .any(|metadata| metadata.modified_nanos >= 1_000_000_000)
        || record.files.keys().any(|path| {
            path.as_os_str().is_empty()
                || path
                    .components()
                    .any(|part| !matches!(part, std::path::Component::Normal(_)))
        })
    {
        return None;
    }
    Some(record)
}

/// None means the receipt needs a one-time full validation. Some(missing)
/// means a valid receipt was checked; changed installations never self-heal it.
pub(super) fn check(
    directory: &Path,
    executable_pin: &str,
    cancel: &watch::Receiver<bool>,
) -> Result<Option<Vec<String>>, AssetError> {
    let Some(record) = read(directory, executable_pin) else {
        return Ok(None);
    };
    let mut actual = match inventory(directory, cancel) {
        Ok(actual) => actual,
        Err(AssetError::Cancelled) => return Err(AssetError::Cancelled),
        Err(error) => return Ok(Some(vec![error.to_string()])),
    };
    let mut missing = Vec::new();
    for (relative, metadata) in record.files {
        if actual.remove(&relative).as_ref() != Some(&metadata) {
            missing.push(format!("changed or missing: {}", relative.display()));
        }
    }
    missing.extend(
        actual
            .keys()
            .map(|path| format!("unexpected: {}", path.display())),
    );
    if missing.is_empty() {
        let mut executable = File::open(directory.join("bin/katago.exe"))?;
        if !runtime_archive::hash(&mut executable, cancel)?
            .sha256
            .eq_ignore_ascii_case(executable_pin)
        {
            missing.push("changed: bin/katago.exe SHA-256".into());
        }
    }
    Ok(Some(missing))
}

#[cfg(test)]
mod tests;
