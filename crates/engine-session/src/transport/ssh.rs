use super::{CHANNEL_CAPACITY, Exit, Output, STDERR_LINES, Transport};
use crate::{
    RemoteTransportError as Error, SessionConfig, SshAuth, TransportSpec, remote::CONNECT_TIMEOUT,
};
use russh::{
    ChannelMsg, client,
    keys::{PrivateKeyWithHashAlg, PublicKeyOrCertificate, load_secret_key},
};
use std::{collections::VecDeque, path::PathBuf, sync::Arc};
use tokio::sync::{mpsc, oneshot};
use tracing::instrument::WithSubscriber;

mod known_hosts;

struct Client {
    host: String,
    port: u16,
    known_hosts: PathBuf,
}

impl client::Handler for Client {
    type Error = Error;
    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> Result<bool, Error> {
        let host = self.host.clone();
        let port = self.port;
        let path = self.known_hosts.clone();
        let key = key.public_key();
        tokio::task::spawn_blocking(move || known_hosts::verify(&path, &host, port, &key))
            .await
            .map_err(|_| Error::Connect)??;
        Ok(true)
    }
}

pub(super) async fn start(spec: &TransportSpec, cfg: &SessionConfig) -> Result<Transport, Error> {
    tokio::time::timeout(
        CONNECT_TIMEOUT,
        connect(spec, cfg).with_subscriber(tracing::subscriber::NoSubscriber::default()),
    )
    .await
    .map_err(|_| Error::ConnectTimeout)?
}

async fn connect(spec: &TransportSpec, cfg: &SessionConfig) -> Result<Transport, Error> {
    let TransportSpec::Ssh {
        host,
        port,
        user,
        auth,
        command,
        known_hosts,
    } = spec
    else {
        return Err(Error::InvalidConfiguration);
    };
    if host.is_empty()
        || host.chars().any(char::is_whitespace)
        || *port == 0
        || user.is_empty()
        || command.is_empty()
    {
        return Err(Error::InvalidConfiguration);
    }
    let config = Arc::new(client::Config::default());
    let handler = Client {
        host: host.clone(),
        port: *port,
        known_hosts: known_hosts.clone(),
    };
    // check_server_key completes before connect returns, so authentication can
    // never run until the TOFU record has been verified and durably saved.
    let mut session = client::connect(config, (host.as_str(), *port), handler).await?;
    let result = match auth {
        SshAuth::Password(password) => session.authenticate_password(user, password).await,
        SshAuth::PrivateKey { path, passphrase } => {
            let path = path.clone();
            let passphrase = passphrase.clone();
            let key =
                tokio::task::spawn_blocking(move || load_secret_key(path, passphrase.as_deref()))
                    .await
                    .map_err(|_| Error::PrivateKey)?
                    .map_err(|_| Error::PrivateKey)?;
            let hash = session.best_supported_rsa_hash().await?.flatten();
            session
                .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
                .await
        }
    }
    .map_err(|_| Error::Authentication)?;
    if !result.success() {
        return Err(Error::Authentication);
    }
    let mut channel = session.channel_open_session().await?;
    channel.exec(true, command.as_bytes()).await?;
    let mut startup_output = VecDeque::new();
    loop {
        match channel.wait().await {
            Some(ChannelMsg::Success) => break,
            Some(ChannelMsg::Failure | ChannelMsg::Close) | None => return Err(Error::Connect),
            Some(message) => {
                if startup_output.len() >= CHANNEL_CAPACITY {
                    return Err(Error::Protocol);
                }
                startup_output.push_back(message);
            }
        }
    }
    let (outgoing, mut writes) = mpsc::channel::<String>(CHANNEL_CAPACITY);
    let (received, incoming) = mpsc::channel(CHANNEL_CAPACITY);
    let (exit, exited) = oneshot::channel();
    let (stop, mut stopping) = oneshot::channel();
    let write_timeout = cfg.write_timeout;
    let shutdown_timeout = cfg.shutdown_timeout;
    let secrets: Vec<String> = match auth {
        SshAuth::Password(value) => vec![value.clone()],
        SshAuth::PrivateKey { passphrase, .. } => passphrase.iter().cloned().collect(),
    };
    let driver = Box::pin(async move {
        let mut stdout = Lines::default();
        let mut stderr = Lines::default();
        let mut tail = VecDeque::new();
        let reason = loop {
            tokio::select! {
                _ = &mut stopping => break "SSH connection stopped".to_owned(),
                line = writes.recv() => match line {
                    Some(mut line) => {
                        line.push('\n');
                        if !matches!(tokio::time::timeout(write_timeout, channel.data(line.as_bytes())).await, Ok(Ok(()))) {
                            break "SSH write failed or timed out".to_owned();
                        }
                    },
                    None => break "SSH input closed".into(),
                },
                message = async {
                    if let Some(message) = startup_output.pop_front() { Some(message) }
                    else { channel.wait().await }
                } => match message {
                    Some(ChannelMsg::Data { data }) => {
                        if stdout.push(&data, &received, false, &mut tail, &secrets).await.is_err() { break "SSH stdout line is invalid or too long".into(); }
                    },
                    Some(ChannelMsg::ExtendedData { data, ext: 1 }) => {
                        if stderr.push(&data, &received, true, &mut tail, &secrets).await.is_err() { break "SSH stderr line is invalid or too long".into(); }
                    },
                    Some(ChannelMsg::ExitStatus { exit_status }) => {
                        // ExitStatus precedes EOF/Close; retain and drain final output.
                        stderr.exit_status = Some(exit_status);
                    },
                    Some(ChannelMsg::Close) | None => break match stderr.exit_status {
                        Some(code) => format!("SSH remote command exited (code {code})"),
                        None => "SSH connection closed".into(),
                    },
                    _ => {},
                }
            }
        };
        let _ = stdout.finish(&received, false, &mut tail, &secrets).await;
        let _ = stderr.finish(&received, true, &mut tail, &secrets).await;
        let _ = tokio::time::timeout(shutdown_timeout, async {
            let _ = channel.eof().await;
            let _ = channel.close().await;
            let _ = session.disconnect(russh::Disconnect::ByApplication, "", "").await;
        }).await;
        let _ = exit.send(Exit { status: Ok(std::process::ExitStatus::default()), stderr_tail: tail.into(), reason: Some(reason), fatal: false });
    }.with_subscriber(tracing::subscriber::NoSubscriber::default()));
    Ok(Transport {
        outgoing,
        incoming,
        exited,
        stop: Some(stop),
        driver,
    })
}

#[derive(Default)]
struct Lines {
    pending: Vec<u8>,
    exit_status: Option<u32>,
}
impl Lines {
    async fn push(
        &mut self,
        bytes: &[u8],
        sender: &mpsc::Sender<Output>,
        stderr: bool,
        tail: &mut VecDeque<String>,
        secrets: &[String],
    ) -> Result<(), ()> {
        self.pending.extend_from_slice(bytes);
        while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
            let bytes: Vec<_> = self.pending.drain(..=end).collect();
            Self::emit(&bytes, sender, stderr, tail, secrets).await?;
        }
        if self.pending.len() > 16 * 1024 * 1024 {
            return Err(());
        }
        Ok(())
    }
    async fn finish(
        &mut self,
        sender: &mpsc::Sender<Output>,
        stderr: bool,
        tail: &mut VecDeque<String>,
        secrets: &[String],
    ) -> Result<(), ()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        Self::emit(
            &std::mem::take(&mut self.pending),
            sender,
            stderr,
            tail,
            secrets,
        )
        .await
    }
    async fn emit(
        bytes: &[u8],
        sender: &mpsc::Sender<Output>,
        stderr: bool,
        tail: &mut VecDeque<String>,
        secrets: &[String],
    ) -> Result<(), ()> {
        let line = std::str::from_utf8(bytes)
            .map_err(|_| ())?
            .trim_end_matches(['\r', '\n'])
            .to_owned();
        let line = if stderr {
            crate::remote::redact(line, secrets)
        } else {
            crate::remote::redact_protocol(line, secrets)
        };
        let output = if stderr {
            if tail.len() == STDERR_LINES {
                tail.pop_front();
            }
            tail.push_back(line.clone());
            Output::Stderr(line)
        } else {
            Output::Stdout(line)
        };
        sender.send(output).await.map_err(|_| ())
    }
}
