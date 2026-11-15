//! A deterministic test process for the KataGo analysis protocol.
//!
//! This crate belongs in consumers' `dev-dependencies` only.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

use serde_json::Value;
use thiserror::Error;

static BINARY_PATH: Mutex<BTreeMap<&'static str, PathBuf>> = Mutex::new(BTreeMap::new());

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
/// checks freshness even if an older executable exists. Initialization is
/// serialized because Cargo can remove and relink the executable: concurrent
/// builds could otherwise make another caller's path check or process spawn
/// fail. Other callers wait for the first successful build and reuse its path;
/// failed builds are not cached, allowing a later caller to retry.
/// Call this at test runtime, not in build.rs.
pub fn binary_path() -> Result<PathBuf, BinaryPathError> {
    named_binary_path("fake-katago")
}

/// A separately compiled CPU fixture with the same protocol and a distinct SHA.
pub fn cpu_binary_path() -> Result<PathBuf, BinaryPathError> {
    named_binary_path("fake-katago-cpu")
}

fn named_binary_path(name: &'static str) -> Result<PathBuf, BinaryPathError> {
    let mut cached_path = BINARY_PATH
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(path) = cached_path.get(name) {
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
            name,
            "--message-format=json",
        ])
        .output()?;
    if !output.status.success() {
        return Err(BinaryPathError::BuildFailed {
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    let path = named_artifact_path(&String::from_utf8_lossy(&output.stdout), name)
        .filter(|path| path.is_file())
        .ok_or(BinaryPathError::MissingExecutable)?;
    cached_path.insert(name, path.clone());
    Ok(path)
}

#[cfg(test)]
fn artifact_path(messages: &str) -> Option<PathBuf> {
    named_artifact_path(messages, "fake-katago")
}

fn named_artifact_path(messages: &str, name: &str) -> Option<PathBuf> {
    messages.lines().find_map(|line| {
        let value: Value = serde_json::from_str(line).ok()?;
        if value["reason"] != "compiler-artifact"
            || value["target"]["name"] != name
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
