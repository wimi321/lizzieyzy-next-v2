//! A deterministic test process for the KataGo analysis protocol.
//!
//! This crate belongs in consumers' `dev-dependencies` only.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use serde_json::Value;
use thiserror::Error;

static BINARY_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Failures locating or building the test executable.
#[derive(Debug, Error)]
pub enum BinaryPathError {
    #[error("could not invoke cargo to build fake-katago: {0}")]
    Io(#[from] std::io::Error),
    #[error("cargo build -p fake-katago failed (exit code {code:?}): {stderr}")]
    BuildFailed { code: Option<i32>, stderr: String },
    #[error("cargo did not report an existing fake-katago executable")]
    MissingExecutable,
}

/// Build the binary once per caller process and return Cargo's executable path.
///
/// Cargo's JSON output respects `CARGO_TARGET_DIR`, configured target triples and
/// platform executable suffixes; no `target/debug` layout is assumed. The build
/// checks freshness even if an older executable exists. Subsequent calls reuse
/// the path while it still exists. Call this at test runtime, not in build.rs.
pub fn binary_path() -> Result<PathBuf, BinaryPathError> {
    if let Some(path) = BINARY_PATH.get().filter(|path| path.is_file()) {
        return Ok(path.clone());
    }
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "build",
            "--locked",
            "-p",
            "fake-katago",
            "--bin",
            "fake-katago",
            "--message-format=json",
        ])
        .output()?;
    if !output.status.success() {
        return Err(BinaryPathError::BuildFailed {
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    let path = artifact_path(&String::from_utf8_lossy(&output.stdout))
        .filter(|path| path.is_file())
        .ok_or(BinaryPathError::MissingExecutable)?;
    let _ = BINARY_PATH.set(path.clone());
    Ok(path)
}

fn artifact_path(messages: &str) -> Option<PathBuf> {
    messages.lines().find_map(|line| {
        let value: Value = serde_json::from_str(line).ok()?;
        if value["reason"] != "compiler-artifact"
            || value["target"]["name"] != "fake-katago"
            || !value["target"]["kind"]
                .as_array()?
                .iter()
                .any(|kind| kind == "bin")
        {
            return None;
        }
        value["executable"].as_str().map(PathBuf::from)
    })
}

#[cfg(test)]
mod tests;
