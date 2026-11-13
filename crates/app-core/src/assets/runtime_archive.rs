//! Synchronous ZIP work runs only in spawn_blocking. Installed-file identities
//! are reconstructed from reverified pinned archives, never a local manifest.
use super::{AssetCatalog, AssetError, catalog::RuntimeProfileSpec};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
use tokio::sync::watch;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Identity {
    pub size: u64,
    pub sha256: String,
    pub executable_bits: u32,
}
pub(super) type Inventory = BTreeMap<PathBuf, Identity>;
const MAX_EXPANDED_BYTES: u64 = 16 * 1024 * 1024 * 1024;

pub(super) fn inventory(
    root: &Path,
    catalog: &AssetCatalog,
    profile: &RuntimeProfileSpec,
    destination: Option<&Path>,
    cancel: &watch::Receiver<bool>,
) -> Result<Inventory, AssetError> {
    inventory_with_progress(root, catalog, profile, destination, cancel, &mut || {})
}

pub(super) fn inventory_with_progress(
    root: &Path,
    catalog: &AssetCatalog,
    profile: &RuntimeProfileSpec,
    destination: Option<&Path>,
    cancel: &watch::Receiver<bool>,
    progress: &mut impl FnMut(),
) -> Result<Inventory, AssetError> {
    let mut inventory = BTreeMap::new();
    let mut total = 0u64;
    for id in &profile.assets {
        cancelled(cancel)?;
        let spec = catalog.get(id)?;
        let path = root.join(spec.relative_path());
        let mut file = File::open(&path)?;
        if file.metadata()?.len() != spec.size {
            return Err(AssetError::Verification);
        }
        let identity = hash(&mut (&mut file).take(spec.size.saturating_add(1)), cancel)?;
        if identity.size != spec.size || !identity.sha256.eq_ignore_ascii_case(&spec.sha256) {
            return Err(AssetError::Verification);
        }
        file.seek(SeekFrom::Start(0))?;
        let mut archive = zip::ZipArchive::new(file).map_err(zip_error)?;
        if archive.len() > 100_000 {
            return Err(AssetError::Archive("too many entries".into()));
        }
        let mut names = std::collections::BTreeSet::new();
        for index in 0..archive.len() {
            cancelled(cancel)?;
            let mut entry = archive.by_index(index).map_err(zip_error)?;
            let components = safe_name(&entry.name().map_err(zip_error)?)?;
            if entry.is_symlink()
                || entry.unix_mode().is_some_and(|mode| {
                    let kind = mode & 0o170000;
                    kind != 0 && kind != 0o100000 && kind != 0o040000
                })
            {
                return Err(AssetError::Archive(
                    "links or special files are forbidden".into(),
                ));
            }
            let key = components.join("/").to_ascii_lowercase();
            if !names.insert(key) {
                return Err(AssetError::Archive("duplicate archive entry".into()));
            }
            if entry.is_dir() {
                continue;
            }
            total = total
                .checked_add(entry.size())
                .ok_or_else(|| AssetError::Archive("expanded size overflow".into()))?;
            if total > MAX_EXPANDED_BYTES {
                return Err(AssetError::Archive("expanded size exceeds limit".into()));
            }
            let Some(relative) = mapped_path(id, &profile.engine_asset, &components, profile)
            else {
                continue;
            };
            let mut writer = if let Some(destination) = destination {
                let target = destination.join(&relative);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                // Duplicates across packages are checked by content below.
                Some(tempfile::NamedTempFile::new_in(
                    target
                        .parent()
                        .ok_or_else(|| AssetError::Archive("missing parent".into()))?,
                )?)
            } else {
                None
            };
            let mut digest = Sha256::new();
            let mut bytes = 0u64;
            let mut buffer = [0; 64 * 1024];
            loop {
                cancelled(cancel)?;
                let count = entry.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                bytes = bytes
                    .checked_add(count as u64)
                    .ok_or(AssetError::Verification)?;
                if bytes > entry.size() {
                    return Err(AssetError::Verification);
                }
                digest.update(&buffer[..count]);
                if let Some(writer) = &mut writer {
                    writer.write_all(&buffer[..count])?;
                }
            }
            let identity = Identity {
                size: bytes,
                sha256: format!("{:x}", digest.finalize()),
                executable_bits: if cfg!(unix) && relative == Path::new("bin/katago.exe") {
                    entry.unix_mode().unwrap_or(0) & 0o111
                } else {
                    0
                },
            };
            if bytes != entry.size() {
                return Err(AssetError::Verification);
            }
            if let Some(previous) = inventory.get(&relative) {
                if previous != &identity {
                    return Err(AssetError::Archive(format!(
                        "conflicting file: {}",
                        relative.display()
                    )));
                }
                continue;
            }
            if let (Some(mut writer), Some(destination)) = (writer, destination) {
                writer.flush()?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    writer
                        .as_file()
                        .set_permissions(fs::Permissions::from_mode(
                            0o644 | identity.executable_bits,
                        ))?;
                }
                writer.as_file().sync_all()?;
                writer
                    .persist_noclobber(destination.join(&relative))
                    .map_err(|error| AssetError::Io(error.error))?;
            }
            inventory.insert(relative, identity);
            progress();
        }
    }
    for required in &profile.required_files {
        if !inventory.contains_key(&Path::new("bin").join(required.to_ascii_lowercase())) {
            return Err(AssetError::Archive(format!(
                "required runtime file missing: {required}"
            )));
        }
    }
    let engine = catalog.get(&profile.engine_asset)?;
    let expected = engine
        .executable_sha256
        .as_deref()
        .ok_or_else(|| AssetError::Catalog("missing executable pin".into()))?;
    if inventory
        .get(Path::new("bin/katago.exe"))
        .is_none_or(|identity| !identity.sha256.eq_ignore_ascii_case(expected))
    {
        return Err(AssetError::Verification);
    }
    Ok(inventory)
}

fn mapped_path(
    id: &str,
    engine: &str,
    components: &[String],
    profile: &RuntimeProfileSpec,
) -> Option<PathBuf> {
    let name = components.last()?.to_ascii_lowercase();
    let relative: PathBuf = components.iter().collect();
    if id == engine && name == "katago.exe" {
        return Some(Path::new("bin").join(name));
    }
    if name.ends_with(".dll") {
        let msvc = name.starts_with("msvcp")
            || name.starts_with("vcruntime")
            || name.starts_with("concrt")
            || name == "vcomp140.dll";
        if id == "runtime-msvc-runtime" && !msvc {
            return None;
        }
        if id == engine && msvc && profile.assets.iter().any(|id| id == "runtime-msvc-runtime") {
            return None;
        }
        if id == "runtime-tensorrt"
            && !["nvinfer", "nvonnxparser", "onnx_proto", "myelin"]
                .iter()
                .any(|prefix| name.starts_with(prefix))
        {
            return None;
        }
        return Some(Path::new("bin").join(name));
    }
    if components.iter().any(|part| {
        let part = part.to_ascii_lowercase();
        part.contains("license")
            || part.contains("copyright")
            || part == "notice"
            || part == "notice.txt"
    }) {
        return Some(Path::new("licenses").join(id).join(relative));
    }
    if id == engine {
        return Some(Path::new("engine").join(relative));
    }
    None
}

pub(super) fn safe_name(name: &str) -> Result<Vec<String>, AssetError> {
    if name.is_empty() || name.starts_with(['/', '\\']) || name.contains(['\\', ':', '\0']) {
        return Err(AssetError::Archive(
            "absolute or nonportable archive path".into(),
        ));
    }
    let mut parts = Vec::new();
    for part in name.trim_end_matches('/').split('/') {
        let base = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        let reserved = matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (base.len() == 4
                && (base.starts_with("COM") || base.starts_with("LPT"))
                && base.as_bytes()[3].is_ascii_digit());
        if part.is_empty() || part == "." || part == ".." || part.ends_with(['.', ' ']) || reserved
        {
            return Err(AssetError::Archive("unsafe archive path component".into()));
        }
        parts.push(part.to_owned());
    }
    Ok(parts)
}

pub(super) fn hash(
    reader: &mut impl Read,
    cancel: &watch::Receiver<bool>,
) -> Result<Identity, AssetError> {
    let mut digest = Sha256::new();
    let mut size = 0;
    let mut buffer = [0; 64 * 1024];
    loop {
        cancelled(cancel)?;
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        size += count as u64;
        digest.update(&buffer[..count]);
    }
    Ok(Identity {
        size,
        sha256: format!("{:x}", digest.finalize()),
        executable_bits: 0,
    })
}
pub(super) fn cancelled(cancel: &watch::Receiver<bool>) -> Result<(), AssetError> {
    if *cancel.borrow() {
        Err(AssetError::Cancelled)
    } else {
        Ok(())
    }
}
fn zip_error(error: zip::result::ZipError) -> AssetError {
    AssetError::Archive(error.to_string())
}
