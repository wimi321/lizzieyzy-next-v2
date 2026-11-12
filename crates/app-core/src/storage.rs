use std::{io, io::Write, path::PathBuf};

/// Replace a file only after a complete, flushed write in the same directory.
/// A failed write or rename drops the temporary file and leaves the target alone.
pub(crate) async fn write(path: PathBuf, bytes: Vec<u8>) -> io::Result<()> {
    persist(path, bytes, false).await
}

/// Export only to a new target; never overwrite a file or an existing link.
pub(crate) async fn write_new(path: PathBuf, bytes: Vec<u8>) -> io::Result<()> {
    persist(path, bytes, true).await
}

async fn persist(path: PathBuf, bytes: Vec<u8>, no_clobber: bool) -> io::Result<()> {
    tokio::task::spawn_blocking(move || {
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."));
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(&bytes)?;
        temporary.flush()?;
        temporary.as_file().sync_all()?;
        if no_clobber {
            temporary
                .persist_noclobber(&path)
                .map_err(|error| error.error)?;
        } else {
            temporary.persist(&path).map_err(|error| error.error)?;
        }
        Ok(())
    })
    .await
    .map_err(io::Error::other)?
}
