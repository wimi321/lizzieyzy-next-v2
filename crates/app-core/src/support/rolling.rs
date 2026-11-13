use std::{
    io,
    path::{Path, PathBuf},
};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
};

#[derive(Clone, Copy)]
pub(super) struct Limits {
    pub file_bytes: usize,
    pub files: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            file_bytes: 1024 * 1024,
            files: 4,
        }
    }
}

pub(super) struct Logs {
    directory: PathBuf,
    limits: Limits,
    file: Option<fs::File>,
    bytes: usize,
}

impl Logs {
    pub fn new(directory: PathBuf, limits: Limits) -> Self {
        Self {
            directory,
            limits,
            file: None,
            bytes: 0,
        }
    }

    fn path(&self, index: usize) -> PathBuf {
        self.directory.join(if index == 0 {
            "app.log".into()
        } else {
            format!("app.log.{index}")
        })
    }

    async fn open(&mut self) -> io::Result<()> {
        if self.file.is_some() {
            return Ok(());
        }
        directory(&self.directory).await?;
        for index in 0..self.limits.files {
            let path = self.path(index);
            if let Some(bytes) = regular(&path).await? {
                // Old oversized support files must not evade the current cap.
                if bytes > self.limits.file_bytes as u64 {
                    fs::OpenOptions::new()
                        .write(true)
                        .open(path)
                        .await?
                        .set_len(0)
                        .await?;
                }
            }
        }
        let path = self.path(0);
        self.bytes = regular(&path).await?.unwrap_or(0) as usize;
        self.file = Some(
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .await?,
        );
        Ok(())
    }

    pub async fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.open().await?;
        // Truncate oversized records at a UTF-8 boundary before writing. Export
        // applies sanitization again, so truncation cannot expose a partial secret.
        let mut len = bytes.len().min(self.limits.file_bytes);
        while len < bytes.len() && bytes[len] & 0xc0 == 0x80 {
            len -= 1;
        }
        let bytes = &bytes[..len];
        if self.bytes + bytes.len() > self.limits.file_bytes {
            self.rotate().await?;
        }
        if let Some(file) = self.file.as_mut() {
            file.write_all(bytes).await?;
            self.bytes += bytes.len();
        }
        Ok(())
    }

    async fn rotate(&mut self) -> io::Result<()> {
        self.flush().await?;
        directory(&self.directory).await?;
        self.file = None;
        for index in (1..self.limits.files).rev() {
            let destination = self.path(index);
            if regular(&destination).await?.is_some() {
                fs::remove_file(&destination).await?;
            }
            let source = self.path(index - 1);
            if regular(&source).await?.is_some() {
                fs::rename(source, destination).await?;
            }
        }
        // files == 1 still enforces a single-file cap.
        let path = self.path(0);
        regular(&path).await?;
        self.file = Some(
            fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(path)
                .await?,
        );
        self.bytes = 0;
        Ok(())
    }

    pub async fn flush(&mut self) -> io::Result<()> {
        if let Some(file) = self.file.as_mut() {
            file.flush().await?;
        }
        Ok(())
    }

    pub async fn snapshot(&mut self) -> io::Result<Vec<(String, Vec<u8>)>> {
        self.flush().await?;
        let mut entries = Vec::new();
        if fs::symlink_metadata(&self.directory).await.is_ok() {
            directory(&self.directory).await?;
        }
        for index in (0..self.limits.files).rev() {
            let path = self.path(index);
            if regular(&path).await?.is_none() {
                continue;
            }
            let mut data = Vec::new();
            fs::File::open(&path)
                .await?
                .take(self.limits.file_bytes as u64)
                .read_to_end(&mut data)
                .await?;
            let name = if index == 0 {
                "logs/app.log".into()
            } else {
                format!("logs/app.log.{index}")
            };
            entries.push((name, data));
        }
        Ok(entries)
    }
}

async fn regular(path: &Path) -> io::Result<Option<u64>> {
    match fs::symlink_metadata(path).await {
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => Ok(Some(meta.len())),
        Ok(_) => Err(io::Error::other("support log must be a regular file")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

async fn directory(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path).await {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(io::Error::other(
            "support directory must not be a symbolic link",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => fs::create_dir_all(path).await,
        Err(error) => Err(error),
    }
}
