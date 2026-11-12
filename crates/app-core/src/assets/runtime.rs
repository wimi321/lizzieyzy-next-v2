use super::{
    AssetCatalog, AssetError, RuntimeInstallation,
    runtime_archive::{self, Inventory},
};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};
use tokio::sync::watch;

pub(super) fn version(catalog: &AssetCatalog, profile_id: &str) -> Result<String, AssetError> {
    let profile = catalog.profile(profile_id)?;
    let mut hash = Sha256::new();
    hash.update(b"LizzieYzy-runtime-v1\0");
    for id in &profile.assets {
        hash.update(id.as_bytes());
        hash.update([0]);
        hash.update(catalog.get(id)?.sha256.as_bytes());
        hash.update([0]);
    }
    for required in &profile.required_files {
        hash.update(required.as_bytes());
        hash.update([0]);
    }
    Ok(format!("{:x}", hash.finalize())[..24].to_owned())
}

pub(super) fn inspect(
    root: &Path,
    catalog: &AssetCatalog,
    profile_id: &str,
) -> Result<RuntimeInstallation, AssetError> {
    let (_cancel, cancellation) = watch::channel(false);
    inspect_cancellable(root, catalog, profile_id, &cancellation)
}

pub(super) fn inspect_cancellable(
    root: &Path,
    catalog: &AssetCatalog,
    profile_id: &str,
    cancellation: &watch::Receiver<bool>,
) -> Result<RuntimeInstallation, AssetError> {
    inspect_generation(root, catalog, profile_id, cancellation, None)
}

fn inspect_generation(
    root: &Path,
    catalog: &AssetCatalog,
    profile_id: &str,
    cancellation: &watch::Receiver<bool>,
    generation: Option<&Path>,
) -> Result<RuntimeInstallation, AssetError> {
    let profile = catalog.profile(profile_id)?;
    let mut status = RuntimeInstallation {
        profile_id: profile_id.into(),
        missing: profile.required_files.clone(),
        ..Default::default()
    };
    let parent = root.join("runtimes").join(profile_id);
    if fs::symlink_metadata(root.join("runtimes")).is_ok_and(|metadata| !metadata.is_dir()) {
        status.error = Some("runtime root is not an ordinary directory".into());
        return Ok(status);
    }
    let metadata = match fs::symlink_metadata(&parent) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(status),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_dir() {
        status.error = Some("runtime directory is not an ordinary directory".into());
        return Ok(status);
    }
    let version = version(catalog, profile_id)?;
    let mut candidates = Vec::new();
    for entry in fs::read_dir(&parent)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with(&format!("{version}-"))
            && entry.file_type()?.is_dir()
            && generation.is_none_or(|generation| generation == entry.path())
        {
            candidates.push(entry.path());
        }
    }
    if candidates.is_empty() {
        return Ok(status);
    }
    let expected = match runtime_archive::inventory(root, catalog, profile, None, cancellation) {
        Ok(expected) => expected,
        Err(AssetError::Cancelled) => return Err(AssetError::Cancelled),
        Err(error) => {
            status.error = Some(error.to_string());
            return Ok(status);
        }
    };
    candidates.sort();
    for candidate in candidates.into_iter().rev() {
        let missing = validate(&candidate, &expected, cancellation)?;
        status.directory = Some(candidate.join("bin"));
        status.program = Some(candidate.join("bin/katago.exe"));
        status.missing = missing;
        if status.missing.is_empty() {
            status.ready = true;
            return Ok(status);
        }
    }
    Ok(status)
}

pub(super) fn validate(
    directory: &Path,
    expected: &Inventory,
    cancel: &watch::Receiver<bool>,
) -> Result<Vec<String>, AssetError> {
    let mut actual = std::collections::BTreeSet::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(path) = pending.pop() {
        runtime_archive::cancelled(cancel)?;
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            let path = entry.path();
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() {
                if let Ok(relative) = path.strip_prefix(directory) {
                    actual.insert(relative.to_path_buf());
                }
            } else {
                return Ok(vec![format!(
                    "unexpected link or special file: {}",
                    path.display()
                )]);
            }
            if actual.len() + pending.len() > 100_000 {
                return Err(AssetError::Archive("too many installed files".into()));
            }
        }
    }
    let mut missing = Vec::new();
    for (relative, expected) in expected {
        if !actual.remove(relative) {
            missing.push(relative.to_string_lossy().into_owned());
            continue;
        }
        let mut file = File::open(directory.join(relative))?;
        if file.metadata()?.len() != expected.size {
            missing.push(format!("changed: {}", relative.display()));
            continue;
        }
        let identity = runtime_archive::hash(
            &mut std::io::Read::take(&mut file, expected.size.saturating_add(1)),
            cancel,
        )?;
        #[cfg(unix)]
        let identity = {
            use std::os::unix::fs::PermissionsExt;
            let mut identity = identity;
            identity.executable_bits = file.metadata()?.permissions().mode() & 0o111;
            identity
        };
        if &identity != expected {
            missing.push(format!("changed: {}", relative.display()));
        }
    }
    missing.extend(
        actual
            .into_iter()
            .map(|path| format!("unexpected: {}", path.display())),
    );
    Ok(missing)
}

pub(super) fn for_program(
    root: &Path,
    catalog: &AssetCatalog,
    program: &Path,
) -> Result<Option<RuntimeInstallation>, AssetError> {
    let (_cancel, cancellation) = watch::channel(false);
    let relative = program.strip_prefix(root.join("runtimes")).ok();
    let managed = relative
        .and_then(|path| path.components().next())
        .and_then(|part| part.as_os_str().to_str())
        .filter(|id| catalog.profiles.contains_key(*id))
        .map(str::to_owned);
    let actual = (|| {
        if !fs::metadata(program)?.is_file() {
            return Err(AssetError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "selected program is not a regular file",
            )));
        }
        let mut file = File::open(program)?;
        Ok(runtime_archive::hash(&mut file, &cancellation)?.sha256)
    })();
    let identified = actual.as_ref().ok().and_then(|sha| {
        catalog
            .specs
            .iter()
            .find(|spec| {
                spec.executable_sha256
                    .as_deref()
                    .is_some_and(|pin| pin.eq_ignore_ascii_case(sha))
            })
            .and_then(|spec| spec.runtime_profile.clone())
    });
    let Some(profile_id) = managed.clone().or(identified.clone()) else {
        // This path has no managed runtime identity. Basic path diagnostics
        // report missing/unreadable/non-file programs without losing the report.
        return Ok(None);
    };
    let mut status = RuntimeInstallation {
        profile_id: profile_id.clone(),
        program: Some(program.to_path_buf()),
        directory: program.parent().map(Path::to_path_buf),
        missing: catalog.profile(&profile_id)?.required_files.clone(),
        ..Default::default()
    };
    if let Err(error) = actual {
        status.error = Some(error.to_string());
        return Ok(Some(status));
    }
    if managed.is_none() {
        status.error = Some("external executable requires installation and switching to the verified managed executable".into());
        return Ok(Some(status));
    }
    if identified.as_deref() != Some(profile_id.as_str()) {
        status.error = Some(
            "selected managed executable is changed or belongs to another runtime profile".into(),
        );
        return Ok(Some(status));
    }
    let parts = relative
        .map(|path| path.components().collect::<Vec<_>>())
        .unwrap_or_default();
    if parts.len() != 4
        || parts
            .iter()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
        || parts[2].as_os_str() != "bin"
        || parts[3].as_os_str() != "katago.exe"
    {
        status.error =
            Some("selected program is not the executable of a managed installation".into());
        return Ok(Some(status));
    }
    let generation = root
        .join("runtimes")
        .join(&profile_id)
        .join(parts[1].as_os_str());
    match inspect_generation(root, catalog, &profile_id, &cancellation, Some(&generation)) {
        Ok(selected) => {
            status.ready = selected.ready;
            status.missing = selected.missing;
            status.error = selected.error;
        }
        Err(error) => status.error = Some(error.to_string()),
    }
    // Windows searches beside the selected executable before PATH. A verified
    // alternative generation cannot make a damaged selected generation safe.
    Ok(Some(status))
}

pub(super) fn stage(
    root: &Path,
    catalog: &AssetCatalog,
    profile_id: &str,
    cancel: &watch::Receiver<bool>,
) -> Result<(tempfile::TempDir, PathBuf), AssetError> {
    stage_with_progress(root, catalog, profile_id, cancel, &mut || {})
}

pub(super) fn stage_with_progress(
    root: &Path,
    catalog: &AssetCatalog,
    profile_id: &str,
    cancel: &watch::Receiver<bool>,
    progress: &mut impl FnMut(),
) -> Result<(tempfile::TempDir, PathBuf), AssetError> {
    let profile = catalog.profile(profile_id)?;
    let parent = root.join("runtimes").join(profile_id);
    fs::create_dir_all(root)?;
    ensure_directory(&root.join("runtimes"))?;
    ensure_directory(&parent)?;
    let temporary = tempfile::Builder::new()
        .prefix(".install-")
        .tempdir_in(&parent)?;
    let prepared = (|| {
        let expected = runtime_archive::inventory_with_progress(
            root,
            catalog,
            profile,
            Some(temporary.path()),
            cancel,
            progress,
        )?;
        let missing = validate(temporary.path(), &expected, cancel)?;
        if !missing.is_empty() {
            return Err(AssetError::Archive(missing.join(", ")));
        }
        runtime_archive::cancelled(cancel)?;
        let suffix = temporary
            .path()
            .file_name()
            .ok_or_else(|| AssetError::Archive("missing temporary name".into()))?
            .to_string_lossy();
        Ok(parent.join(format!(
            "{}-{}",
            version(catalog, profile_id)?,
            suffix.trim_start_matches('.')
        )))
    })();
    match prepared {
        Ok(target) => Ok((temporary, target)),
        Err(error) => {
            temporary.close()?;
            Err(error)
        }
    }
}

pub(super) fn publish(
    temporary: tempfile::TempDir,
    target: PathBuf,
) -> Result<PathBuf, AssetError> {
    // Each complete generation has a fresh name: Windows never has to replace
    // a nonempty directory or a DLL held by the currently running engine.
    if target.try_exists()? {
        return Err(AssetError::Archive(
            "installation generation already exists".into(),
        ));
    }
    fs::rename(temporary.path(), &target)?;
    drop(temporary);
    Ok(target.join("bin"))
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;

fn ensure_directory(path: &Path) -> Result<(), AssetError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(AssetError::Archive(
            "runtime parent is a link or file".into(),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(AssetError::from)
        }
        Err(error) => Err(error.into()),
    }
}
