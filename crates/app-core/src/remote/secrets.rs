use super::{
    RemoteEngineConfig, RemoteEngineError, RemoteEngineInput, RemoteSshAuth, SshAuthConfig,
};
use crate::{AppError, EngineProfile, commentary_keys::SecretStore};
use engine_session::{SshAuth, TransportSpec};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

#[derive(Default)]
pub(crate) struct RemoteSecrets {
    store: Option<Arc<dyn SecretStore>>,
    values: BTreeMap<String, String>,
    pending: Option<(String, String, Option<String>)>,
}
impl RemoteSecrets {
    pub fn new(store: Option<Arc<dyn SecretStore>>) -> Self {
        Self {
            store,
            values: BTreeMap::new(),
            pending: None,
        }
    }

    pub async fn import(&mut self, input: RemoteEngineInput) -> Result<EngineProfile, AppError> {
        let (config, secret) = prepare(input)?;
        let account = config.account()?;
        self.load(&account).await;
        let previous = self.values.insert(account.clone(), secret.clone());
        self.pending = Some((account, secret, previous));
        Ok(EngineProfile {
            remote: Some(config),
            ..EngineProfile::default()
        })
    }

    async fn load(&mut self, account: &str) {
        if !self.values.contains_key(account)
            && let Some(store) = &self.store
            && let Ok(Some(value)) = store.read(account.to_owned()).await
        {
            self.values.insert(account.to_owned(), value);
        }
    }
    pub async fn commit(&mut self) {
        if let Some((account, secret, _)) = self.pending.take()
            && let Some(store) = &self.store
            && store.write(account.clone(), secret).await.is_err()
        {
            let _ = store.delete(account).await;
        }
    }
    pub fn rollback(&mut self) {
        if let Some((account, _, previous)) = self.pending.take() {
            match previous {
                Some(value) => {
                    self.values.insert(account, value);
                }
                None => {
                    self.values.remove(&account);
                }
            }
        }
    }

    pub async fn transport(
        &mut self,
        remote: &RemoteEngineConfig,
        known_hosts: PathBuf,
    ) -> Result<TransportSpec, AppError> {
        remote.validate()?;
        let account = remote.account()?;
        if !self.values.contains_key(&account)
            && let Some(store) = &self.store
            && let Ok(Some(value)) = store.read(account.clone()).await
        {
            self.values.insert(account.clone(), value);
        }
        let secret = self.values.get(&account).cloned();
        transport(remote, secret, known_hosts)
    }

    pub fn preview(
        input: RemoteEngineInput,
        known_hosts: PathBuf,
    ) -> Result<TransportSpec, AppError> {
        let (remote, secret) = prepare(input)?;
        transport(&remote, Some(secret), known_hosts)
    }
}

fn prepare(input: RemoteEngineInput) -> Result<(RemoteEngineConfig, String), AppError> {
    let (remote, secret) = match input {
        RemoteEngineInput::WebSocket { url } => {
            let normalized = crate::normalize_remote_link(&url)?;
            let parsed = url::Url::parse(&normalized)
                .map_err(|_| RemoteEngineError::InvalidConfiguration)?;
            (
                RemoteEngineConfig::WebSocket {
                    host: parsed
                        .host_str()
                        .ok_or(RemoteEngineError::InvalidConfiguration)?
                        .to_owned(),
                    port: parsed
                        .port_or_known_default()
                        .ok_or(RemoteEngineError::InvalidConfiguration)?,
                    secure: parsed.scheme() == "wss",
                    identity: blake3::hash(normalized.as_bytes()).to_hex().to_string(),
                    version: None,
                },
                normalized,
            )
        }
        RemoteEngineInput::Ssh {
            host,
            port,
            user,
            command,
            auth,
        } => {
            let (auth, secret) = match auth {
                RemoteSshAuth::Password { password } => (SshAuthConfig::Password, password),
                RemoteSshAuth::PrivateKey { path, passphrase } => (
                    SshAuthConfig::PrivateKey { path },
                    passphrase.unwrap_or_default(),
                ),
            };
            (
                RemoteEngineConfig::Ssh {
                    host,
                    port,
                    user,
                    command,
                    auth,
                    version: None,
                },
                secret,
            )
        }
    };
    remote.validate()?;
    Ok((remote, secret))
}

fn transport(
    remote: &RemoteEngineConfig,
    secret: Option<String>,
    known_hosts: PathBuf,
) -> Result<TransportSpec, AppError> {
    Ok(match remote {
        RemoteEngineConfig::Zhizi { .. } => {
            return Err(RemoteEngineError::ZhiziLoginRequired.into());
        }
        RemoteEngineConfig::WebSocket { identity, .. } => {
            let url = secret.ok_or(RemoteEngineError::MissingSecret)?;
            if blake3::hash(url.as_bytes()).to_hex().as_str() != identity {
                return Err(RemoteEngineError::MissingSecret.into());
            }
            TransportSpec::WebSocket { url }
        }
        RemoteEngineConfig::Ssh {
            host,
            port,
            user,
            command,
            auth,
            ..
        } => {
            let auth = match auth {
                SshAuthConfig::Password => {
                    SshAuth::Password(secret.ok_or(RemoteEngineError::MissingSecret)?)
                }
                SshAuthConfig::PrivateKey { path } => SshAuth::PrivateKey {
                    path: path.clone(),
                    passphrase: secret.filter(|s| !s.is_empty()),
                },
            };
            TransportSpec::Ssh {
                host: host.clone(),
                port: *port,
                user: user.clone(),
                command: command.clone(),
                auth,
                known_hosts,
            }
        }
    })
}
