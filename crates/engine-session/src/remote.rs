//! Remote connection inputs deliberately omit secret-bearing Debug output.
use crate::{SessionConfig, TransportSpec, transport};
use katago_protocol::{Action, ActionResponse, EngineLine, parse_line};
use std::{path::PathBuf, time::Duration};

#[derive(Clone)]
pub enum SshAuth {
    Password(String),
    PrivateKey {
        path: PathBuf,
        passphrase: Option<String>,
    },
}

impl std::fmt::Debug for SshAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Password(_) => f.write_str("Password([redacted])"),
            Self::PrivateKey { .. } => f.write_str("PrivateKey([redacted])"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RemoteTransportError {
    #[error("invalid remote engine configuration")]
    InvalidConfiguration,
    #[error("remote connection failed")]
    Connect,
    #[error("remote connection timed out")]
    ConnectTimeout,
    #[error("SSH authentication failed")]
    Authentication,
    #[error("SSH private key could not be read or decrypted")]
    PrivateKey,
    #[error("SSH known hosts could not be read or saved: {path}")]
    KnownHosts { path: PathBuf },
    #[error(
        "SSH host key changed: recorded {old_fingerprint}, presented {new_fingerprint}; known hosts: {path}"
    )]
    HostKeyChanged {
        old_fingerprint: String,
        new_fingerprint: String,
        path: PathBuf,
    },
    #[error("remote engine did not complete query_version")]
    Protocol,
    #[error("remote query_version timed out")]
    HandshakeTimeout,
    #[error("Zhizi did not become ready within 60 seconds")]
    ReadyTimeout,
    #[error("Zhizi login expired or access denied")]
    ZhiziAccessDenied,
    #[error("Zhizi balance is insufficient")]
    ZhiziBalance,
    #[error("Zhizi VIP subscription is unavailable or expired")]
    ZhiziVip,
}

impl From<russh::Error> for RemoteTransportError {
    fn from(_: russh::Error) -> Self {
        Self::Connect
    }
}

pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

pub(crate) fn redact(mut line: String, secrets: &[String]) -> String {
    for secret in secrets.iter().filter(|secret| !secret.is_empty()) {
        line = line.replace(secret, "[redacted]");
        if let Ok(encoded) = serde_json::to_string(secret) {
            line = line.replace(&encoded[1..encoded.len() - 1], "[redacted]");
        }
    }
    line
}

pub(crate) fn redact_protocol(line: String, secrets: &[String]) -> String {
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&line) else {
        return redact(line, secrets);
    };
    for key in ["warning", "error", "version", "git_hash"] {
        if let Some(serde_json::Value::String(text)) = value.get_mut(key) {
            let secrets: Vec<_> = secrets
                .iter()
                .filter(|secret| {
                    !matches!(key, "version" | "git_hash")
                        || secret.len() >= 4
                        || secret.as_str() == text
                })
                .cloned()
                .collect();
            *text = redact(std::mem::take(text), &secrets);
        }
    }
    if value.get("warning").is_some()
        && let Some(object) = value.as_object_mut()
    {
        // Warning IDs are diagnostic only; removing an untrusted one avoids
        // leaking an echoed secret without modifying actual query identities.
        object.remove("id");
    }
    serde_json::to_string(&value).unwrap_or_default()
}

/// Probe the actual transport, query its analysis-protocol version, then close.
/// No session, cache, settings, or secret-store changes are performed here.
pub async fn probe_remote(
    spec: &TransportSpec,
    cfg: SessionConfig,
) -> Result<String, RemoteTransportError> {
    if !spec.is_remote() {
        return Err(RemoteTransportError::InvalidConfiguration);
    }
    let mut transport = transport::start(spec, &cfg).await.map_err(|error| {
        error
            .get_ref()
            .and_then(|error| error.downcast_ref::<RemoteTransportError>())
            .cloned()
            .unwrap_or(RemoteTransportError::Connect)
    })?;
    let query = serde_json::to_string(&Action::QueryVersion {
        id: "remote-probe".into(),
    })
    .map_err(|_| RemoteTransportError::Protocol)?;
    let _ = transport.outgoing.send(query).await;
    let mut driver_done = false;
    let result = tokio::time::timeout(cfg.startup_timeout, async {
        loop {
            tokio::select! {
                _ = &mut transport.driver, if !driver_done => driver_done = true,
                output = transport.incoming.recv() => match output {
                    Some(transport::Output::Stdout(line)) => {
                        if let Ok(EngineLine::Action(ActionResponse::QueryVersion { id, version, .. })) = parse_line(&line)
                            && id == "remote-probe" { return Ok(version); }
                    }
                    Some(transport::Output::Stderr(_)) => {},
                    _ => return Err(RemoteTransportError::Protocol),
                }
            }
        }
    }).await.unwrap_or(Err(RemoteTransportError::HandshakeTimeout));
    if let Some(stop) = transport.stop.take() {
        let _ = stop.send(());
    }
    if !driver_done {
        // Drain output while shutdown is polled: a full receive channel must
        // never prevent the transport's cleanup from making progress.
        loop {
            tokio::select! {
                _ = &mut transport.driver => break,
                _ = transport.incoming.recv() => {},
            }
        }
    }
    result
}
