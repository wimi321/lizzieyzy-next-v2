//! The current engine-session API exposes stderr only after failure. A private
//! KataGo logFile observes startup without introducing another process owner.
use super::output::{OutputTail, reported_backend};
use engine_session::TransportSpec;
use std::{ffi::OsString, io, path::Path, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt},
    time::Instant,
};

const READ_BYTES: u64 = 64 * 1024;
const LINE_BYTES: usize = 4096;

pub(crate) struct Observation {
    directory: tempfile::TempDir,
    tail: OutputTail,
    offset: u64,
    partial: Vec<u8>,
    pub(crate) next_poll: Instant,
    pub(crate) backend_line: Option<String>,
}

impl Observation {
    pub(crate) async fn new() -> io::Result<Self> {
        let directory = tokio::task::spawn_blocking(|| {
            tempfile::Builder::new()
                .prefix("lizzie-acceleration-")
                .tempdir()
        })
        .await
        .map_err(io::Error::other)??;
        validate_path(&directory.path().join("startup.log"))?;
        Ok(Self {
            directory,
            tail: OutputTail::default(),
            offset: 0,
            partial: Vec::new(),
            next_poll: Instant::now(),
            backend_line: None,
        })
    }

    /// Only the transient transport is changed. Stored model/config/arguments
    /// remain untouched, and all override assignments stay in one CLI argument.
    pub(crate) fn apply(&self, transport: &mut TransportSpec) -> io::Result<()> {
        let TransportSpec::LocalProcess { args, .. } = transport else {
            return Err(io::Error::other("startup logging requires a local engine"));
        };
        let path = self.directory.path().join("startup.log");
        let path = validate_path(&path)?;
        let mut values = Vec::new();
        let index = args.iter().position(|value| value == "-override-config");
        if let Some(index) = index {
            let value = args
                .get(index + 1)
                .and_then(|value| value.to_str())
                .ok_or_else(|| io::Error::other("invalid engine override argument"))?;
            values.extend(
                value
                    .split(',')
                    .filter(|assignment| {
                        let key = assignment.split('=').next().unwrap_or("").trim();
                        ![
                            "logFile",
                            "logDir",
                            "logDirDated",
                            "logToStderr",
                            "logAllRequests",
                            "logAllResponses",
                            "logSearchInfo",
                        ]
                        .contains(&key)
                    })
                    .filter(|assignment| !assignment.trim().is_empty())
                    .map(str::to_owned),
            );
        }
        values.extend([
            format!("logFile={path}"),
            "logDir=".into(),
            "logDirDated=".into(),
            "logToStderr=true".into(),
            "logAllRequests=false".into(),
            "logAllResponses=false".into(),
            "logSearchInfo=false".into(),
        ]);
        let value = OsString::from(values.join(","));
        if let Some(index) = index {
            args[index + 1] = value;
        } else {
            args.extend([OsString::from("-override-config"), value]);
        }
        Ok(())
    }

    pub(crate) async fn read(&mut self) -> io::Result<Vec<String>> {
        self.next_poll = Instant::now() + Duration::from_millis(100);
        let path = self.directory.path().join("startup.log");
        let mut file = match tokio::fs::File::open(path).await {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let size = file.metadata().await?.len();
        if size < self.offset {
            self.offset = 0;
            self.partial.clear();
        }
        let skip_first = size.saturating_sub(self.offset) > READ_BYTES;
        if skip_first {
            self.offset = size - READ_BYTES;
            self.partial.clear();
        }
        file.seek(std::io::SeekFrom::Start(self.offset)).await?;
        let mut bytes = Vec::with_capacity(READ_BYTES as usize);
        file.take(READ_BYTES).read_to_end(&mut bytes).await?;
        self.offset += bytes.len() as u64;
        let mut skipping = skip_first;
        let mut lines = Vec::new();
        for byte in bytes {
            if skipping {
                if byte == b'\n' {
                    skipping = false;
                }
            } else if byte == b'\n' || byte == b'\r' {
                if let Some(line) = self.finish_line() {
                    lines.push(line);
                }
            } else if self.partial.len() < LINE_BYTES {
                self.partial.push(byte);
            }
        }
        Ok(lines)
    }

    fn finish_line(&mut self) -> Option<String> {
        if self.partial.is_empty() {
            return None;
        }
        let mut line = String::from_utf8_lossy(&std::mem::take(&mut self.partial)).into_owned();
        if line.len() > LINE_BYTES {
            let mut end = LINE_BYTES;
            while !line.is_char_boundary(end) {
                end -= 1;
            }
            line.truncate(end);
        }
        self.tail.push(&line);
        if reported_backend(&line).is_some() {
            self.backend_line = Some(line.clone());
        }
        Some(line)
    }

    pub(crate) fn finish(&mut self) {
        self.finish_line();
    }

    pub(crate) fn lines(&self) -> Vec<String> {
        self.tail.lines()
    }
}

fn validate_path(path: &Path) -> io::Result<&str> {
    path.to_str()
        .filter(|value| !value.contains([',', '=', '\r', '\n', '\0']))
        .ok_or_else(|| {
            io::Error::other("temporary log path cannot be represented in KataGo overrides")
        })
}

#[cfg(test)]
mod tests;
