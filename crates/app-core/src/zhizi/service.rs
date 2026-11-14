use super::*;
use crate::commentary_keys::{KeyStoreFuture, KeyStoreUnavailable, SecretStore};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{
    sync::{mpsc, oneshot, watch},
    time::Instant,
};
use tracing::instrument::WithSubscriber;

const TOKEN_ACCOUNT: &str = "zhizi:login";
type Reply<T> = oneshot::Sender<Result<T, ZhiziError>>;

async fn stored<T>(operation: KeyStoreFuture<'_, T>) -> Result<T, KeyStoreUnavailable> {
    tokio::time::timeout(Duration::from_secs(2), operation)
        .await
        .map_err(|_| KeyStoreUnavailable)?
}

enum Command {
    Restore(Reply<ZhiziStatus>),
    Login {
        account: String,
        secret: String,
        code: bool,
        remember: bool,
        reply: Reply<ZhiziStatus>,
    },
    Reset {
        account: String,
        code: String,
        password: String,
        remember: bool,
        reply: Reply<ZhiziStatus>,
    },
    SendCode(String, ZhiziVerificationPurpose, Reply<ZhiziCodeDelivery>),
    Logout(Reply<ZhiziStatus>),
    Overview(bool, Reply<ZhiziOverview>),
    Usages(u32, Reply<ZhiziUsagePage>),
    Credits(u32, Reply<ZhiziCreditPage>),
    SocketToken(String, Reply<ZhiziSocketToken>),
}

/// A credential actor; HTTP never blocks the document actor and reconnects
/// request a fresh socket token through this same account owner.
#[derive(Clone)]
pub struct ZhiziService {
    commands: mpsc::Sender<Command>,
    status: watch::Receiver<ZhiziStatus>,
}
impl std::fmt::Debug for ZhiziService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = self.status();
        f.debug_struct("ZhiziService")
            .field("logged_in", &status.logged_in)
            .field("remembered", &status.remembered)
            .field("store_available", &status.store_available)
            .finish_non_exhaustive()
    }
}

impl ZhiziService {
    #[cfg(test)]
    pub(crate) fn inactive_for_test() -> Self {
        let (commands, _receive) = mpsc::channel(1);
        let (_send, status) = watch::channel(ZhiziStatus::default());
        Self { commands, status }
    }
    pub fn spawn(client: Option<ZhiziClient>, store: Option<Arc<dyn SecretStore>>) -> Self {
        let (commands, receive) = mpsc::channel(32);
        let initial = ZhiziStatus {
            store_available: store.is_some(),
            ..ZhiziStatus::default()
        };
        let (send_status, status) = watch::channel(initial);
        tokio::spawn(
            State {
                client,
                store: store.map(super::store::ordered),
                token: None,
                account: None,
                status: initial,
                send_status,
                loaded: false,
                pending_delete: false,
                overview: None,
                cooldowns: HashMap::new(),
            }
            .run(receive)
            .with_current_subscriber(),
        );
        Self { commands, status }
    }
    pub fn status(&self) -> ZhiziStatus {
        *self.status.borrow()
    }
    pub async fn restore(&self) -> Result<ZhiziStatus, ZhiziError> {
        self.request(Command::Restore).await
    }
    pub async fn login_password(
        &self,
        account: String,
        password: String,
        remember: bool,
    ) -> Result<ZhiziStatus, ZhiziError> {
        self.request(|reply| Command::Login {
            account,
            secret: password,
            code: false,
            remember,
            reply,
        })
        .await
    }
    /// Unlike password login, this operation can create an account.
    pub async fn login_code(
        &self,
        account: String,
        code: String,
        remember: bool,
    ) -> Result<ZhiziStatus, ZhiziError> {
        self.request(|reply| Command::Login {
            account,
            secret: code,
            code: true,
            remember,
            reply,
        })
        .await
    }
    pub async fn reset_password(
        &self,
        account: String,
        code: String,
        password: String,
        remember: bool,
    ) -> Result<ZhiziStatus, ZhiziError> {
        self.request(|reply| Command::Reset {
            account,
            code,
            password,
            remember,
            reply,
        })
        .await
    }
    pub async fn send_code(
        &self,
        account: String,
        purpose: ZhiziVerificationPurpose,
    ) -> Result<ZhiziCodeDelivery, ZhiziError> {
        self.request(|reply| Command::SendCode(account, purpose, reply))
            .await
    }
    pub async fn logout(&self) -> Result<ZhiziStatus, ZhiziError> {
        self.request(Command::Logout).await
    }
    pub async fn overview(&self, force: bool) -> Result<ZhiziOverview, ZhiziError> {
        self.request(|reply| Command::Overview(force, reply)).await
    }
    pub async fn usages(&self, page: u32) -> Result<ZhiziUsagePage, ZhiziError> {
        self.request(|reply| Command::Usages(page, reply)).await
    }
    pub async fn credits(&self, page: u32) -> Result<ZhiziCreditPage, ZhiziError> {
        self.request(|reply| Command::Credits(page, reply)).await
    }
    pub async fn socket_token(&self, args: String) -> Result<ZhiziSocketToken, ZhiziError> {
        self.request(|reply| Command::SocketToken(args, reply))
            .await
    }
    async fn request<T>(&self, make: impl FnOnce(Reply<T>) -> Command) -> Result<T, ZhiziError> {
        let (reply, result) = oneshot::channel();
        self.commands
            .send(make(reply))
            .await
            .map_err(|_| ZhiziError::new(ZhiziErrorKind::Closed))?;
        result
            .await
            .map_err(|_| ZhiziError::new(ZhiziErrorKind::Closed))?
    }
}

struct State {
    client: Option<ZhiziClient>,
    store: Option<Arc<dyn SecretStore>>,
    token: Option<String>,
    account: Option<String>,
    status: ZhiziStatus,
    send_status: watch::Sender<ZhiziStatus>,
    loaded: bool,
    pending_delete: bool,
    overview: Option<(Instant, ZhiziOverview)>,
    cooldowns: HashMap<String, Instant>,
}

impl State {
    async fn run(mut self, mut receive: mpsc::Receiver<Command>) {
        while let Some(command) = receive.recv().await {
            if self.pending_delete {
                self.delete_stored().await;
            }
            match command {
                Command::Restore(reply) => {
                    self.restore().await;
                    self.publish();
                    let _ = reply.send(Ok(self.status));
                }
                Command::Login {
                    account,
                    secret,
                    code,
                    remember,
                    reply,
                } => {
                    let result = self.login(account, secret, code, remember).await;
                    self.publish();
                    let _ = reply.send(result);
                }
                Command::Reset {
                    account,
                    code,
                    password,
                    remember,
                    reply,
                } => {
                    let result = self.reset(account, code, password, remember).await;
                    self.publish();
                    let _ = reply.send(result);
                }
                Command::SendCode(account, purpose, reply) => {
                    let result = self.send_code(account, purpose).await;
                    self.publish();
                    let _ = reply.send(result);
                }
                Command::Logout(reply) => {
                    self.clear().await;
                    self.publish();
                    let _ = reply.send(Ok(self.status));
                }
                Command::Overview(force, reply) => {
                    let result = self.overview(force).await;
                    let result = self.checked(result).await;
                    let _ = reply.send(result);
                }
                Command::Usages(page, reply) => {
                    let result = match self.credentials() {
                        Ok((client, token)) => client.usages(&token, page, 20).await,
                        Err(error) => Err(error),
                    };
                    let result = self.checked(result).await;
                    let _ = reply.send(result);
                }
                Command::Credits(page, reply) => {
                    let result = match self.credentials() {
                        Ok((client, token)) => client.credits(&token, page).await,
                        Err(error) => Err(error),
                    };
                    let result = self.checked(result).await;
                    let _ = reply.send(result);
                }
                Command::SocketToken(args, reply) => {
                    let result = match self.credentials() {
                        Ok((client, token)) => client.socket_token(&token, &args).await,
                        Err(error) => Err(error),
                    };
                    let result = self.checked(result).await;
                    let _ = reply.send(result);
                }
            }
        }
    }
    fn client(&mut self) -> Result<ZhiziClient, ZhiziError> {
        if let Some(client) = &self.client {
            return Ok(client.clone());
        }
        let client = ZhiziClient::new()?;
        self.client = Some(client.clone());
        Ok(client)
    }
    fn credentials(&mut self) -> Result<(ZhiziClient, String), ZhiziError> {
        let token = self
            .token
            .clone()
            .ok_or_else(|| ZhiziError::new(ZhiziErrorKind::LoginRequired))?;
        Ok((self.client()?, token))
    }
    fn publish(&self) {
        self.send_status.send_replace(self.status);
    }
    async fn restore(&mut self) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        if let Some(store) = &self.store {
            match stored(store.read(TOKEN_ACCOUNT.to_owned())).await {
                Ok(token) => {
                    self.token = token.filter(|v| client::valid_token(v));
                    self.status.logged_in = self.token.is_some();
                    self.status.remembered = self.token.is_some();
                    self.status.store_available = true;
                }
                Err(_) => self.status.store_available = false,
            }
        }
    }
    async fn delete_stored(&mut self) {
        self.status.remembered = false;
        if let Some(store) = &self.store {
            let result = stored(store.delete(TOKEN_ACCOUNT.to_owned())).await;
            self.pending_delete = result.is_err();
            self.status.store_available = result.is_ok();
        }
    }
    async fn clear(&mut self) {
        self.token = None;
        self.account = None;
        self.overview = None;
        self.loaded = true;
        self.status.logged_in = false;
        self.status.may_register_account = false;
        self.delete_stored().await;
    }
    async fn checked<T>(&mut self, mut result: Result<T, ZhiziError>) -> Result<T, ZhiziError> {
        if let Err(error) = &mut result {
            for secret in [self.account.as_deref(), self.token.as_deref()]
                .into_iter()
                .flatten()
            {
                if !secret.is_empty() && error.request_id.contains(secret) {
                    error.request_id = "redacted".into();
                }
                if let ZhiziErrorKind::Unknown(key) = &mut error.kind
                    && !secret.is_empty()
                    && key.contains(secret)
                {
                    *key = "redacted".into();
                }
            }
        }
        if result.as_ref().is_err_and(ZhiziError::is_unauthorized) {
            self.clear().await;
        }
        self.publish();
        result
    }
    async fn accept(
        &mut self,
        account: String,
        token: String,
        remember: bool,
        may_register: bool,
    ) -> ZhiziStatus {
        // Delete the former account's entry before accepting a replacement.
        // Only tokens are stored; account names never leave this actor.
        self.delete_stored().await;
        self.token = Some(token.clone());
        self.account = Some(account.trim().to_owned());
        self.loaded = true;
        self.overview = None;
        self.status.logged_in = true;
        self.status.may_register_account = may_register;
        if remember
            && !self.pending_delete
            && let Some(store) = &self.store
        {
            match stored(store.write(TOKEN_ACCOUNT.to_owned(), token)).await {
                Ok(()) => {
                    self.status.remembered = true;
                    self.status.store_available = true;
                }
                Err(_) => {
                    self.delete_stored().await;
                    self.status.store_available = false;
                    self.status.remembered = false;
                }
            }
        }
        self.status
    }
    async fn login(
        &mut self,
        account: String,
        secret: String,
        code: bool,
        remember: bool,
    ) -> Result<ZhiziStatus, ZhiziError> {
        let result = self.client()?.login(&account, &secret, code).await;
        let token = self.checked(result).await?;
        Ok(self.accept(account, token, remember, code).await)
    }
    async fn reset(
        &mut self,
        account: String,
        code: String,
        password: String,
        remember: bool,
    ) -> Result<ZhiziStatus, ZhiziError> {
        let result = self.client()?.reset(&account, &code, &password).await;
        let token = self.checked(result).await?;
        Ok(self.accept(account, token, remember, false).await)
    }
    async fn send_code(
        &mut self,
        account: String,
        purpose: ZhiziVerificationPurpose,
    ) -> Result<ZhiziCodeDelivery, ZhiziError> {
        client::identifier(&account)?;
        let identity = blake3::hash(account.trim().as_bytes()).to_hex().to_string();
        let now = Instant::now();
        self.cooldowns.retain(|_, deadline| *deadline > now);
        if let Some(deadline) = self.cooldowns.get(&identity) {
            let remaining = (*deadline - now).as_secs_f64().ceil() as u32;
            return Err(ZhiziError {
                retry_after_seconds: remaining,
                ..ZhiziError::new(ZhiziErrorKind::TooFrequent)
            });
        }
        if self.cooldowns.len() >= 128 {
            return Err(ZhiziError {
                retry_after_seconds: 600,
                ..ZhiziError::new(ZhiziErrorKind::TooFrequent)
            });
        }
        let mut result = self.client()?.send_code(&account, purpose).await;
        if let Err(error) = &mut result
            && error.kind == ZhiziErrorKind::TooFrequent
            && error.retry_after_seconds == 0
        {
            error.retry_after_seconds = 60;
        }
        let delay = match &result {
            Ok(delivery) => Some(delivery.retry_after_seconds),
            Err(error) if error.kind == ZhiziErrorKind::TooFrequent => {
                Some(if error.retry_after_seconds == 0 {
                    60
                } else {
                    error.retry_after_seconds
                })
            }
            _ => None,
        };
        if let Some(delay) = delay {
            self.cooldowns.insert(
                identity,
                Instant::now() + Duration::from_secs(u64::from(delay)),
            );
        }
        self.checked(result).await
    }
    async fn overview(&mut self, force: bool) -> Result<ZhiziOverview, ZhiziError> {
        if !force
            && let Some((loaded, overview)) = &self.overview
            && loaded.elapsed() < Duration::from_secs(45)
        {
            return Ok(overview.clone());
        }
        let (client, token) = self.credentials()?;
        let (account, balance, usage) = tokio::join!(
            client.account(&token),
            client.balance(&token),
            client.usages(&token, 0, 5)
        );
        // An expired credential takes precedence even if a concurrent endpoint
        // also failed; never retain a token after observing any 401.
        for error in [
            account.as_ref().err(),
            balance.as_ref().err(),
            usage.as_ref().err(),
        ]
        .into_iter()
        .flatten()
        {
            if error.is_unauthorized() {
                return Err(error.clone());
            }
        }
        let overview = ZhiziOverview {
            account: account?,
            balance: balance?,
            recent_usage: usage?,
        };
        self.overview = Some((Instant::now(), overview.clone()));
        Ok(overview)
    }
}
