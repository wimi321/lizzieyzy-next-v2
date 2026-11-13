use std::{
    io::{self, Write},
    path::PathBuf,
};

use serde_json::json;
use zip::{ZipWriter, write::SimpleFileOptions};

use super::{BundleInput, SupportError, sanitize};

pub(super) fn write(
    directory: PathBuf,
    input: BundleInput,
    logs: Vec<(String, Vec<u8>)>,
) -> Result<PathBuf, SupportError> {
    match std::fs::symlink_metadata(&directory) {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {}
        Ok(_) => {
            return Err(
                io::Error::other("diagnostics directory must not be a symbolic link").into(),
            );
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            std::fs::create_dir_all(&directory)?
        }
        Err(error) => return Err(error.into()),
    }
    let mut file = tempfile::Builder::new()
        .prefix(".diagnostics-")
        .suffix(".tmp")
        .tempfile_in(&directory)?;
    let stem = file
        .path()
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::other("invalid temporary diagnostic name"))?;
    let destination = directory.join(format!("{}.zip", stem.trim_start_matches('.')));
    {
        let mut zip = ZipWriter::new(file.as_file_mut());
        let settings = sanitize::json(serde_json::to_value(input.settings)?);
        let diagnostics = sanitize::json(serde_json::to_value(input.diagnostics)?);
        let system = json!({
            "version": sanitize::text(&input.version),
            "os": std::env::consts::OS,
            "architecture": std::env::consts::ARCH,
            "family": std::env::consts::FAMILY,
            "redaction": "java-privacy-categories-conservative-v1",
        });
        for (name, value) in [
            ("settings.json", settings),
            ("engine.json", diagnostics),
            ("system.json", system),
        ] {
            zip.start_file(name, SimpleFileOptions::default())?;
            zip.write_all(&serde_json::to_vec_pretty(&value)?)?;
        }
        for (name, data) in logs {
            zip.start_file(name, SimpleFileOptions::default())?;
            zip.write_all(sanitize::log_file(&String::from_utf8_lossy(&data)).as_bytes())?;
        }
        zip.finish()?;
    }
    file.as_file_mut().flush()?;
    file.as_file().sync_all()?;
    file.persist_noclobber(&destination)
        .map_err(|error| error.error)?;
    Ok(destination)
}
