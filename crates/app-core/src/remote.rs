//! Remote engine identities are persistent; credentials cross only the shared
//! secret-store boundary. No URL, password or passphrase belongs in Settings.
use crate::{AppCore, AppError, EngineProfile, Settings, actor::Command};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::oneshot;
use ts_rs::TS;

mod secrets;
pub(crate) use secrets::RemoteSecrets;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum SshAuthConfig {
    Password,
    PrivateKey {
        #[ts(as = "Option<PathBuf>", optional)]
        path: PathBuf,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum RemoteEngineConfig {
    WebSocket {
        #[ts(as = "Option<String>", optional)]
        host: String,
        #[ts(as = "Option<u16>", optional)]
        port: u16,
        #[ts(as = "Option<bool>", optional)]
        secure: bool,
        /// Hash of the entire canonical link, including its path. The path can
        /// itself contain a token, so only this identity is saved on disk.
        #[ts(as = "Option<String>", optional)]
        identity: String,
        #[ts(optional = nullable)]
        version: Option<String>,
    },
    Ssh {
        #[ts(as = "Option<String>", optional)]
        host: String,
        #[ts(as = "Option<u16>", optional)]
        port: u16,
        #[ts(as = "Option<String>", optional)]
        user: String,
        #[ts(as = "Option<String>", optional)]
        command: String,
        #[ts(as = "Option<SshAuthConfig>", optional)]
        auth: SshAuthConfig,
        #[ts(optional = nullable)]
        version: Option<String>,
    },
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum RemoteSshAuth {
    Password {
        password: String,
    },
    PrivateKey {
        path: PathBuf,
        passphrase: Option<String>,
    },
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum RemoteEngineInput {
    WebSocket {
        url: String,
    },
    Ssh {
        host: String,
        port: u16,
        user: String,
        command: String,
        auth: RemoteSshAuth,
    },
}

impl std::fmt::Debug for RemoteSshAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RemoteSshAuth([redacted])")
    }
}
impl std::fmt::Debug for RemoteEngineInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RemoteEngineInput([redacted])")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS, thiserror::Error)]
#[serde(tag = "type", content = "data")]
pub enum RemoteEngineError {
    #[error("remote engine does not support this operation")]
    Unsupported,
    #[error("invalid remote engine configuration")]
    InvalidConfiguration,
    #[error("remote credentials are unavailable; enter them again")]
    MissingSecret,
    #[error("no usable local engine exists")]
    NoLocalEngine,
    #[error("remote engine connection failed")]
    Connect,
    #[error("remote engine connection timed out")]
    ConnectTimeout,
    #[error("SSH authentication failed")]
    Authentication,
    #[error("SSH private key could not be loaded")]
    PrivateKey,
    #[error("cannot access SSH known-hosts file: {path}")]
    KnownHosts { path: PathBuf },
    #[error("SSH host key changed from {old_fingerprint} to {new_fingerprint}; file: {path}")]
    HostKeyChanged {
        old_fingerprint: String,
        new_fingerprint: String,
        path: PathBuf,
    },
    #[error("remote engine protocol failed")]
    Protocol,
    #[error("remote engine version handshake timed out")]
    HandshakeTimeout,
}

pub(crate) enum Request {
    Add(
        String,
        RemoteEngineInput,
        oneshot::Sender<Result<Settings, AppError>>,
    ),
    Update(
        String,
        String,
        RemoteEngineInput,
        oneshot::Sender<Result<Settings, AppError>>,
    ),
    Prepare(
        RemoteEngineInput,
        oneshot::Sender<
            Result<(engine_session::TransportSpec, engine_session::SessionConfig), AppError>,
        >,
    ),
    Local(oneshot::Sender<Result<Settings, AppError>>),
}

impl AppCore {
    pub async fn add_remote_engine(
        &self,
        name: String,
        entry: RemoteEngineInput,
    ) -> Result<Settings, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Remote(Request::Add(name, entry, reply)))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
    pub async fn update_remote_engine(
        &self,
        id: String,
        name: String,
        entry: RemoteEngineInput,
    ) -> Result<Settings, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Remote(Request::Update(id, name, entry, reply)))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
    pub async fn test_remote_engine(&self, entry: RemoteEngineInput) -> Result<String, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Remote(Request::Prepare(entry, reply)))
            .await
            .map_err(|_| AppError::Closed)?;
        let (transport, config) = response.await.map_err(|_| AppError::Closed)??;
        engine_session::probe_remote(&transport, config)
            .await
            .map_err(|error| RemoteEngineError::from(error).into())
    }
    pub async fn switch_to_local_engine(&self) -> Result<Settings, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Remote(Request::Local(reply)))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
}

impl RemoteEngineConfig {
    pub(crate) fn account(&self) -> Result<String, AppError> {
        let mut identity = self.clone();
        identity.set_version(None);
        let encoded = serde_json::to_vec(&identity).map_err(crate::SettingsError::from)?;
        Ok(format!("remote:{}", blake3::hash(&encoded).to_hex()))
    }
    pub(crate) fn set_version(&mut self, value: Option<String>) {
        match self {
            Self::WebSocket { version, .. } | Self::Ssh { version, .. } => *version = value,
        }
    }
    pub(crate) fn validate(&self) -> Result<(), RemoteEngineError> {
        let valid = match self {
            Self::WebSocket {
                host,
                port,
                identity,
                ..
            } => {
                !host.trim().is_empty()
                    && *port > 0
                    && identity.len() == 64
                    && identity.bytes().all(|b| b.is_ascii_hexdigit())
            }
            Self::Ssh {
                host,
                port,
                user,
                command,
                auth,
                ..
            } => {
                !host.trim().is_empty()
                    && !host.chars().any(char::is_whitespace)
                    && *port > 0
                    && !user.trim().is_empty()
                    && !command.trim().is_empty()
                    && !command.contains('\0')
                    && match auth {
                        SshAuthConfig::Password => true,
                        SshAuthConfig::PrivateKey { path } => !path.as_os_str().is_empty(),
                    }
            }
        };
        if valid {
            Ok(())
        } else {
            Err(RemoteEngineError::InvalidConfiguration)
        }
    }
}

impl EngineProfile {
    pub fn with_remote_version(&self, version: &str) -> Self {
        let mut profile = self.clone();
        if let Some(remote) = &mut profile.remote {
            remote.set_version(Some(version.to_owned()));
        }
        profile
    }
    pub(crate) fn validate_transport(&self) -> Result<(), AppError> {
        match &self.remote {
            Some(remote) => Ok(remote.validate()?),
            None => {
                self.transport()?;
                Ok(())
            }
        }
    }
}

impl From<engine_session::RemoteTransportError> for RemoteEngineError {
    fn from(error: engine_session::RemoteTransportError) -> Self {
        use engine_session::RemoteTransportError as E;
        match error {
            E::InvalidConfiguration => Self::InvalidConfiguration,
            E::Connect => Self::Connect,
            E::ConnectTimeout => Self::ConnectTimeout,
            E::Authentication => Self::Authentication,
            E::PrivateKey => Self::PrivateKey,
            E::KnownHosts { path } => Self::KnownHosts { path },
            E::HostKeyChanged {
                old_fingerprint,
                new_fingerprint,
                path,
            } => Self::HostKeyChanged {
                old_fingerprint,
                new_fingerprint,
                path,
            },
            E::Protocol => Self::Protocol,
            E::HandshakeTimeout => Self::HandshakeTimeout,
        }
    }
}

#[cfg(test)]
mod tests;
