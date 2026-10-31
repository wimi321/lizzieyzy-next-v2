use super::{BoardSyncError, BoardSyncLanguage, BoardSyncOptions};
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command},
    time::{Instant, timeout},
};

const IO_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) struct Process {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    stderr: ChildStderr,
    pending: Vec<u8>,
    stderr_open: bool,
    pub ready: bool,
    startup_deadline: Instant,
}

fn io_error(error: impl std::fmt::Display) -> BoardSyncError {
    BoardSyncError::Process {
        reason: error.to_string(),
    }
}

impl Process {
    pub async fn spawn(
        program: PathBuf,
        options: &BoardSyncOptions,
    ) -> Result<Self, BoardSyncError> {
        if !tokio::fs::metadata(&program)
            .await
            .is_ok_and(|m| m.is_file())
        {
            return Err(BoardSyncError::MissingExecutable { path: program });
        }
        let program = tokio::fs::canonicalize(program).await.map_err(io_error)?;
        let language = match options.language {
            BoardSyncLanguage::Cn => readboard::Language::Chinese,
            BoardSyncLanguage::En => readboard::Language::English,
        };
        let mut command = Command::new(&program);
        command
            .args(readboard::launch_arguments(language))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(directory) = program.parent() {
            command.current_dir(directory);
        }
        let mut child = command.spawn().map_err(io_error)?;
        let input = child.stdin.take();
        let Some(output) = child.stdout.take() else {
            return Err(io_error("missing stdout pipe"));
        };
        let Some(stderr) = child.stderr.take() else {
            return Err(io_error("missing stderr pipe"));
        };
        Ok(Self {
            child,
            input,
            output: BufReader::new(output),
            stderr,
            pending: Vec::new(),
            stderr_open: true,
            ready: false,
            startup_deadline: Instant::now() + Duration::from_secs(15),
        })
    }

    /// Cancellation safe: partially consumed bytes remain in self.pending.
    pub async fn next_line(&mut self) -> Result<Option<String>, BoardSyncError> {
        let mut diagnostics = [0; 2048];
        loop {
            tokio::select! {
                bytes = self.output.fill_buf() => {
                    let bytes = bytes.map_err(io_error)?;
                    if bytes.is_empty() {
                        if self.pending.is_empty() { return Ok(None); }
                        return Err(BoardSyncError::Protocol { reason: "unterminated output line".into() });
                    }
                    let length = bytes.iter().position(|&byte| byte == b'\n').map(|i| i + 1).unwrap_or(bytes.len());
                    if self.pending.len() + length > readboard::MAX_LINE_BYTES + 2 {
                        return Err(BoardSyncError::Protocol { reason: "output line exceeds limit".into() });
                    }
                    self.pending.extend_from_slice(&bytes[..length]);
                    self.output.consume(length);
                    if self.pending.last() == Some(&b'\n') {
                        return String::from_utf8(std::mem::take(&mut self.pending))
                            .map(Some).map_err(|e| BoardSyncError::Protocol { reason: e.to_string() });
                    }
                }
                result = self.stderr.read(&mut diagnostics), if self.stderr_open => {
                    self.stderr_open = result.map_err(io_error)? != 0;
                }
                _ = tokio::time::sleep_until(self.startup_deadline), if !self.ready => {
                    return Err(io_error("readboard did not become ready within 15 seconds"));
                }
            }
        }
    }

    pub async fn send(&mut self, command: readboard::Command) -> Result<(), BoardSyncError> {
        let Some(input) = &mut self.input else {
            return Err(io_error("stdin is closed"));
        };
        timeout(IO_TIMEOUT, input.write_all(command.encode().as_bytes()))
            .await
            .map_err(|_| io_error("stdin write timed out"))?
            .map_err(io_error)
    }

    /// Shutdown and drain pipes concurrently so a full pipe cannot prevent exit.
    pub async fn stop(mut self) -> Result<(), BoardSyncError> {
        let _ = self.send(readboard::Command::Quit).await;
        self.input.take();
        let drain = async {
            let mut out = [0; 2048];
            let mut err = [0; 2048];
            let mut output_open = true;
            loop {
                tokio::select! {
                    result = self.child.wait() => return result,
                    result = self.output.read(&mut out), if output_open => {
                        output_open = result.is_ok_and(|size| size != 0);
                    }
                    result = self.stderr.read(&mut err), if self.stderr_open => {
                        self.stderr_open = result.is_ok_and(|size| size != 0);
                    }
                }
            }
        };
        match timeout(IO_TIMEOUT, drain).await {
            Ok(result) => {
                result.map_err(io_error)?;
            }
            Err(_) => {
                self.child.start_kill().map_err(io_error)?;
                self.child.wait().await.map_err(io_error)?;
            }
        }
        Ok(())
    }
}
