use std::{
    io,
    path::{Path, PathBuf},
};

use super::SupportError;

pub(super) async fn resolve(directory: &Path, path: &Path) -> Result<PathBuf, SupportError> {
    if !path.is_absolute() {
        return Err(SupportError::RevealOutsideDataDirectory);
    }
    // Canonicalization resolves both parent components and symbolic links. In
    // particular, a lexical prefix such as `data-neighbour` never authorizes a
    // sibling of `data`, and a symlink under data cannot authorize its target.
    let (directory, path) = tokio::try_join!(
        tokio::fs::canonicalize(directory),
        tokio::fs::canonicalize(path),
    )?;
    if !tokio::fs::metadata(&directory).await?.is_dir() {
        return Err(io::Error::other("application data root must be a directory").into());
    }
    if !path.starts_with(&directory) {
        return Err(SupportError::RevealOutsideDataDirectory);
    }
    let metadata = tokio::fs::metadata(&path).await?;
    if !metadata.is_file() && !metadata.is_dir() {
        return Err(io::Error::other("only regular files or directories may be revealed").into());
    }
    Ok(path)
}
