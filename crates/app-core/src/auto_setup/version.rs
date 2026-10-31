use std::{path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
};

const TIMEOUT: Duration = Duration::from_secs(3);
const OUTPUT_LIMIT: usize = 16 * 1024;

/// The version subcommand does not load a model or initialize an analysis
/// session. Drain both pipes concurrently, with independent memory limits.
pub(super) async fn probe(program: &Path) -> Result<String, String> {
    probe_with_timeout(program, TIMEOUT).await
}

async fn probe_with_timeout(program: &Path, timeout: Duration) -> Result<String, String> {
    let mut child = Command::new(program)
        .arg("version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| format!("cannot start version check: {error}"))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let result = tokio::time::timeout(timeout, async {
        let stdout = stdout.ok_or_else(|| "version check has no stdout pipe".to_owned())?;
        let stderr = stderr.ok_or_else(|| "version check has no stderr pipe".to_owned())?;
        tokio::try_join!(
            async {
                child
                    .wait()
                    .await
                    .map_err(|error| format!("cannot wait for version check: {error}"))
            },
            read_bounded(stdout, "stdout"),
            read_bounded(stderr, "stderr"),
        )
    })
    .await
    .map_err(|_| format!("version check timed out after {timeout:?}"))
    .and_then(|result| result);
    let (status, stdout, stderr) = match result {
        Ok(result) => result,
        Err(error) => {
            // kill() waits for the direct child to exit. A caller cancelling
            // this future also triggers the child's kill_on_drop safeguard.
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(error);
        }
    };
    if !status.success() {
        return Err(format!(
            "version check exited with {status}: {} {}",
            stdout.trim(),
            stderr.trim(),
        ));
    }
    output_version(&stdout)
        .or_else(|| output_version(&stderr))
        .ok_or_else(|| "version check returned no recognizable KataGo version".to_owned())
}

async fn read_bounded(reader: impl AsyncRead + Unpin, name: &str) -> Result<String, String> {
    let mut bytes = Vec::new();
    reader
        .take((OUTPUT_LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| format!("cannot read version {name}: {error}"))?;
    if bytes.len() > OUTPUT_LIMIT {
        return Err(format!("version {name} exceeds {OUTPUT_LIMIT} bytes"));
    }
    String::from_utf8(bytes).map_err(|_| format!("version {name} is not UTF-8"))
}

fn output_version(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let mut words = line.split_whitespace();
        let first = words.next()?;
        let candidate = if first.eq_ignore_ascii_case("katago") {
            words.next()?
        } else if words.next().is_none() {
            first
        } else {
            return None;
        };
        parse(candidate).map(|_| candidate.to_owned())
    })
}

/// An unknown version never satisfies a catalog model's minimum requirement.
pub(super) fn supports(version: &str, minimum: &str) -> bool {
    match (parse(version), parse(minimum)) {
        (Some(version), Some(minimum)) => version >= minimum,
        _ => false,
    }
}

fn parse(version: &str) -> Option<(u64, u64, u64)> {
    let version = version.trim().strip_prefix('v').unwrap_or(version.trim());
    let core = match version.find(['-', '+']) {
        Some(index) => {
            let suffix = &version[index + 1..];
            if suffix.is_empty()
                || !suffix
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
            {
                return None;
            }
            &version[..index]
        }
        None => version,
    };
    let mut parts = core.split('.');
    let mut number = || {
        let part = parts.next()?;
        if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        part.parse().ok()
    };
    let result = (number()?, number()?, number()?);
    parts.next().is_none().then_some(result)
}

#[cfg(test)]
mod tests;
