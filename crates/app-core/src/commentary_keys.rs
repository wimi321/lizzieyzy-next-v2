//! Credential boundary. Headless callers inject a store or keep keys in memory;
//! only the desktop production host installs `SystemKeyStore`.
use crate::commentary_client::{CommentaryClientError, validate_key};
use serde::{Deserialize, Serialize};
use std::{future::Future, pin::Pin, sync::Arc};
use ts_rs::TS;

pub type KeyStoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, KeyStoreUnavailable>> + Send + 'a>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("system credential store is unavailable")]
pub struct KeyStoreUnavailable;

pub trait SecretStore: std::fmt::Debug + Send + Sync {
    fn read(&self, account: String) -> KeyStoreFuture<'_, Option<String>>;
    fn write(&self, account: String, key: String) -> KeyStoreFuture<'_, ()>;
    fn delete(&self, account: String) -> KeyStoreFuture<'_, ()>;
}

/// Compatibility name for the P3-16 caller; all secrets share this boundary.
pub use SecretStore as CommentaryKeyStore;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CommentaryKeyStatus {
    #[ts(as = "Option<bool>", optional)]
    pub has_key: bool,
    #[ts(as = "Option<bool>", optional)]
    pub stored: bool,
    #[ts(as = "Option<bool>", optional)]
    pub available: bool,
}

#[derive(Default)]
pub struct CommentaryKeys {
    store: Option<Arc<dyn CommentaryKeyStore>>,
    address: String,
    key: Option<String>,
    stored: bool,
    available: bool,
    loaded: bool,
    remember: bool,
    pending_deletions: Vec<String>,
}

impl std::fmt::Debug for CommentaryKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommentaryKeys")
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}

impl CommentaryKeys {
    pub fn new(store: Option<Arc<dyn CommentaryKeyStore>>) -> Self {
        Self {
            available: store.is_some(),
            store,
            ..Self::default()
        }
    }

    /// Initialize the saved account identity without reading the OS store.
    pub fn with_address(
        store: Option<Arc<dyn CommentaryKeyStore>>,
        address: String,
        remember: bool,
    ) -> Self {
        Self {
            address,
            remember,
            ..Self::new(store)
        }
    }

    pub fn status(&self) -> CommentaryKeyStatus {
        CommentaryKeyStatus {
            has_key: self.key.is_some(),
            stored: self.stored,
            available: self.available && self.pending_deletions.is_empty(),
        }
    }

    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    /// `address` is the canonical address saved in CommentarySettings. Reads
    /// happen once per address/remember transition, never at application spawn.
    pub async fn configure(&mut self, address: &str, remember: bool) -> CommentaryKeyStatus {
        self.retry_deletions().await;
        if self.address != address {
            if !self.address.is_empty() && (self.stored || self.remember) {
                self.delete_account(self.address.clone()).await;
            }
            self.address = address.to_owned();
            self.key = None;
            self.stored = false;
            self.loaded = false;
        }
        if !remember && self.remember {
            self.delete_account(address.to_owned()).await;
            self.stored = false;
            self.loaded = false;
        }
        self.remember = remember;
        if remember
            && !self.loaded
            && !address.is_empty()
            && !self
                .pending_deletions
                .iter()
                .any(|pending| pending == address)
        {
            self.loaded = true;
            if let Some(store) = &self.store {
                let result = if let Some(key) = self.key.clone() {
                    store
                        .write(address.to_owned(), key.clone())
                        .await
                        .map(|()| Some(key))
                } else {
                    store.read(address.to_owned()).await
                };
                match result {
                    Ok(key) => {
                        self.available = true;
                        self.key = key.filter(|key| validate_key(key).is_ok());
                        self.stored = self.key.is_some();
                    }
                    Err(_) => self.available = false,
                }
            }
        }
        self.status()
    }

    pub async fn set(
        &mut self,
        address: &str,
        key: String,
        remember: bool,
    ) -> Result<CommentaryKeyStatus, CommentaryClientError> {
        validate_key(&key)?;
        self.configure(address, remember).await;
        self.key = Some(key.clone());
        self.stored = false;
        if remember && let Some(store) = &self.store {
            match store.write(address.to_owned(), key).await {
                Ok(()) => {
                    self.available = true;
                    self.stored = true;
                }
                Err(_) => {
                    self.available = false;
                    // If updating an old credential fails, do not claim that
                    // the newly supplied in-memory key was persisted.
                    self.delete_account(address.to_owned()).await;
                    self.available = false;
                }
            }
        }
        Ok(self.status())
    }

    pub async fn clear(&mut self, address: &str) -> CommentaryKeyStatus {
        self.configure(address, false).await;
        self.key = None;
        self.stored = false;
        self.loaded = true;
        if !address.is_empty() {
            self.delete_account(address.to_owned()).await;
        }
        self.status()
    }

    async fn delete_account(&mut self, account: String) {
        if let Some(store) = &self.store {
            match store.delete(account.clone()).await {
                Ok(()) => {
                    self.available = true;
                    self.pending_deletions.retain(|pending| pending != &account);
                }
                Err(_) => {
                    self.available = false;
                    if !self.pending_deletions.contains(&account) {
                        self.pending_deletions.push(account);
                    }
                }
            }
        }
    }

    async fn retry_deletions(&mut self) {
        for account in self.pending_deletions.clone() {
            self.delete_account(account).await;
        }
    }
}

/// No OS call is made by construction. Each explicit operation runs in a
/// blocking task; native failures are reduced to a secret-free typed error.
#[derive(Debug)]
pub struct SystemKeyStore;

impl SecretStore for SystemKeyStore {
    fn read(&self, account: String) -> KeyStoreFuture<'_, Option<String>> {
        Box::pin(async move {
            tokio::task::spawn_blocking(move || native::read(&account))
                .await
                .map_err(|_| KeyStoreUnavailable)?
        })
    }
    fn write(&self, account: String, key: String) -> KeyStoreFuture<'_, ()> {
        Box::pin(async move {
            tokio::task::spawn_blocking(move || native::write(&account, &key))
                .await
                .map_err(|_| KeyStoreUnavailable)?
        })
    }
    fn delete(&self, account: String) -> KeyStoreFuture<'_, ()> {
        Box::pin(async move {
            tokio::task::spawn_blocking(move || native::delete(&account))
                .await
                .map_err(|_| KeyStoreUnavailable)?
        })
    }
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
mod native {
    use super::KeyStoreUnavailable;
    fn entry(account: &str) -> Result<keyring::Entry, KeyStoreUnavailable> {
        keyring::Entry::new("io.github.wimi321.lizzieyzy.next2.commentary", account)
            .map_err(|_| KeyStoreUnavailable)
    }
    pub(super) fn read(account: &str) -> Result<Option<String>, KeyStoreUnavailable> {
        match entry(account)?.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(KeyStoreUnavailable),
        }
    }
    pub(super) fn write(account: &str, key: &str) -> Result<(), KeyStoreUnavailable> {
        entry(account)?
            .set_password(key)
            .map_err(|_| KeyStoreUnavailable)
    }
    pub(super) fn delete(account: &str) -> Result<(), KeyStoreUnavailable> {
        match entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(KeyStoreUnavailable),
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod native {
    use super::KeyStoreUnavailable;
    pub(super) fn read(_: &str) -> Result<Option<String>, KeyStoreUnavailable> {
        Err(KeyStoreUnavailable)
    }
    pub(super) fn write(_: &str, _: &str) -> Result<(), KeyStoreUnavailable> {
        Err(KeyStoreUnavailable)
    }
    pub(super) fn delete(_: &str) -> Result<(), KeyStoreUnavailable> {
        Err(KeyStoreUnavailable)
    }
}
