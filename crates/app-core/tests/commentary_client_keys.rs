#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
use app_core::commentary_keys::{
    CommentaryKeyStore, CommentaryKeys, KeyStoreFuture, KeyStoreUnavailable,
};
use common::within;
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::{mpsc, oneshot};

#[derive(Debug)]
struct MemoryStore {
    commands: mpsc::UnboundedSender<Command>,
}
enum Command {
    Read(
        String,
        oneshot::Sender<Result<Option<String>, KeyStoreUnavailable>>,
    ),
    Write(
        String,
        String,
        oneshot::Sender<Result<(), KeyStoreUnavailable>>,
    ),
    Delete(String, oneshot::Sender<Result<(), KeyStoreUnavailable>>),
    Snapshot(oneshot::Sender<(BTreeMap<String, String>, usize)>),
    Available(bool, oneshot::Sender<()>),
    DenyDelete(String, bool, oneshot::Sender<()>),
}
impl MemoryStore {
    fn new() -> Arc<Self> {
        let (commands, mut receiver) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut keys = BTreeMap::new();
            let mut available = true;
            let mut reads = 0;
            let mut denied_deletions = std::collections::BTreeSet::new();
            while let Some(command) = receiver.recv().await {
                match command {
                    Command::Read(account, reply) => {
                        reads += 1;
                        reply
                            .send(if available {
                                Ok(keys.get(&account).cloned())
                            } else {
                                Err(KeyStoreUnavailable)
                            })
                            .unwrap();
                    }
                    Command::Write(account, key, reply) => {
                        reply
                            .send(if available {
                                keys.insert(account, key);
                                Ok(())
                            } else {
                                Err(KeyStoreUnavailable)
                            })
                            .unwrap();
                    }
                    Command::Delete(account, reply) => {
                        reply
                            .send(if available && !denied_deletions.contains(&account) {
                                keys.remove(&account);
                                Ok(())
                            } else {
                                Err(KeyStoreUnavailable)
                            })
                            .unwrap();
                    }
                    Command::Snapshot(reply) => {
                        reply.send((keys.clone(), reads)).unwrap();
                    }
                    Command::Available(value, reply) => {
                        available = value;
                        reply.send(()).unwrap();
                    }
                    Command::DenyDelete(account, denied, reply) => {
                        if denied {
                            denied_deletions.insert(account);
                        } else {
                            denied_deletions.remove(&account);
                        }
                        reply.send(()).unwrap();
                    }
                }
            }
        });
        Arc::new(Self { commands })
    }
    async fn snapshot(&self) -> (BTreeMap<String, String>, usize) {
        let (reply, receive) = oneshot::channel();
        self.commands.send(Command::Snapshot(reply)).unwrap();
        within(receive).await.unwrap()
    }
    async fn available(&self, value: bool) {
        let (reply, receive) = oneshot::channel();
        self.commands
            .send(Command::Available(value, reply))
            .unwrap();
        within(receive).await.unwrap();
    }
    async fn deny_delete(&self, account: &str, denied: bool) {
        let (reply, receive) = oneshot::channel();
        self.commands
            .send(Command::DenyDelete(account.into(), denied, reply))
            .unwrap();
        within(receive).await.unwrap();
    }
}
impl CommentaryKeyStore for MemoryStore {
    fn read(&self, account: String) -> KeyStoreFuture<'_, Option<String>> {
        Box::pin(async move {
            let (reply, receive) = oneshot::channel();
            self.commands
                .send(Command::Read(account, reply))
                .map_err(|_| KeyStoreUnavailable)?;
            receive.await.map_err(|_| KeyStoreUnavailable)?
        })
    }
    fn write(&self, account: String, key: String) -> KeyStoreFuture<'_, ()> {
        Box::pin(async move {
            let (reply, receive) = oneshot::channel();
            self.commands
                .send(Command::Write(account, key, reply))
                .map_err(|_| KeyStoreUnavailable)?;
            receive.await.map_err(|_| KeyStoreUnavailable)?
        })
    }
    fn delete(&self, account: String) -> KeyStoreFuture<'_, ()> {
        Box::pin(async move {
            let (reply, receive) = oneshot::channel();
            self.commands
                .send(Command::Delete(account, reply))
                .map_err(|_| KeyStoreUnavailable)?;
            receive.await.map_err(|_| KeyStoreUnavailable)?
        })
    }
}
const FIRST: &str = "https://first.example/v1";
const SECOND: &str = "http://127.0.0.1:1234/v1";

#[tokio::test]
async fn nonremembered_key_only_lives_in_memory_and_debug_is_secret_free() {
    let store = MemoryStore::new();
    let mut keys = CommentaryKeys::new(Some(store.clone()));
    let status = within(keys.set(FIRST, "private-memory-key".into(), false))
        .await
        .unwrap();
    assert!(status.has_key);
    assert!(!status.stored);
    assert!(status.available);
    assert_eq!(keys.key(), Some("private-memory-key"));
    assert!(!format!("{keys:?}").contains("private-memory-key"));
    assert_eq!(store.snapshot().await, (BTreeMap::new(), 0));
    let mut restarted = CommentaryKeys::new(Some(store));
    assert!(!within(restarted.configure(FIRST, false)).await.has_key);
    assert!(!within(keys.clear(FIRST)).await.has_key);
}

#[tokio::test]
async fn remember_loads_once_and_service_change_deletes_previous_account() {
    let store = MemoryStore::new();
    let mut keys = CommentaryKeys::new(Some(store.clone()));
    assert!(
        within(keys.set(FIRST, "persisted-key".into(), true))
            .await
            .unwrap()
            .stored
    );
    assert_eq!(
        store.snapshot().await.0.get(FIRST).map(String::as_str),
        Some("persisted-key")
    );
    let mut restarted = CommentaryKeys::with_address(Some(store.clone()), FIRST.into(), true);
    assert_eq!(store.snapshot().await.1, 1); // constructor is inert
    assert!(within(restarted.configure(FIRST, true)).await.has_key);
    assert!(within(restarted.configure(FIRST, true)).await.stored);
    assert_eq!(store.snapshot().await.1, 2); // repeated status never rereads
    assert!(!within(restarted.configure(SECOND, true)).await.has_key);
    assert!(store.snapshot().await.0.is_empty());
    assert!(
        within(restarted.set(SECOND, "new-key".into(), true))
            .await
            .unwrap()
            .stored
    );
    assert_eq!(
        store.snapshot().await.0.keys().collect::<Vec<_>>(),
        [SECOND]
    );
    assert!(!within(restarted.configure(SECOND, false)).await.stored);
    assert_eq!(restarted.key(), Some("new-key"));
    assert!(store.snapshot().await.0.is_empty());
}

#[tokio::test]
async fn changing_address_before_first_lookup_still_deletes_saved_account() {
    let store = MemoryStore::new();
    within(store.write(FIRST.into(), "saved-key".into()))
        .await
        .unwrap();
    let mut keys = CommentaryKeys::with_address(Some(store.clone()), FIRST.into(), true);
    within(keys.configure(SECOND, false)).await;
    assert!(store.snapshot().await.0.is_empty());
    assert_eq!(store.snapshot().await.1, 0);
}

#[tokio::test]
async fn unavailable_store_keeps_key_in_memory_and_never_loads_cleared_key_again() {
    let store = MemoryStore::new();
    let mut keys = CommentaryKeys::new(Some(store.clone()));
    within(keys.set(FIRST, "old-key".into(), true))
        .await
        .unwrap();
    store.available(false).await;
    let status = within(keys.set(FIRST, "new-memory-key".into(), true))
        .await
        .unwrap();
    assert!(status.has_key);
    assert!(!status.stored);
    assert!(!status.available);
    assert_eq!(keys.key(), Some("new-memory-key"));
    within(keys.clear(FIRST)).await;
    assert!(!within(keys.configure(FIRST, true)).await.has_key);
    store.available(true).await;
    assert!(!within(keys.configure(FIRST, true)).await.has_key);
    assert!(store.snapshot().await.0.is_empty()); // failed removal is retried
    let mut no_store = CommentaryKeys::default();
    let status = within(no_store.set(FIRST, "temporary".into(), true))
        .await
        .unwrap();
    assert!(status.has_key);
    assert!(!status.available);
    assert!(!status.stored);
}

#[tokio::test]
async fn invalid_key_is_rejected_without_changing_memory_or_persistence() {
    let store = MemoryStore::new();
    let mut keys = CommentaryKeys::new(Some(store.clone()));
    within(keys.set(FIRST, "good-key".into(), true))
        .await
        .unwrap();
    for key in ["", "bad\nvalue", "bad key"] {
        assert!(within(keys.set(SECOND, key.into(), true)).await.is_err());
        assert_eq!(keys.key(), Some("good-key"));
        assert_eq!(
            store.snapshot().await.0.get(FIRST).map(String::as_str),
            Some("good-key")
        );
    }
}

#[tokio::test]
async fn turning_on_remember_persists_existing_memory_key_without_reloading() {
    let store = MemoryStore::new();
    let mut keys = CommentaryKeys::new(Some(store.clone()));
    within(keys.set(FIRST, "memory-key".into(), false))
        .await
        .unwrap();
    let status = within(keys.configure(FIRST, true)).await;
    assert!(status.has_key && status.stored && status.available);
    assert_eq!(keys.key(), Some("memory-key"));
    assert_eq!(
        store.snapshot().await.0.get(FIRST).map(String::as_str),
        Some("memory-key")
    );
    assert_eq!(store.snapshot().await.1, 0);
}

#[tokio::test]
async fn pending_old_account_deletion_stays_visible_even_after_new_account_write_succeeds() {
    let store = MemoryStore::new();
    let mut keys = CommentaryKeys::new(Some(store.clone()));
    within(keys.set(FIRST, "old-key".into(), true))
        .await
        .unwrap();
    store.deny_delete(FIRST, true).await;
    let status = within(keys.set(SECOND, "new-key".into(), true))
        .await
        .unwrap();
    assert!(status.has_key && status.stored);
    assert!(!status.available);
    assert_eq!(store.snapshot().await.0.len(), 2);
    store.deny_delete(FIRST, false).await;
    assert!(within(keys.configure(SECOND, true)).await.available);
    let snapshot = store.snapshot().await.0;
    assert!(!snapshot.contains_key(FIRST));
    assert_eq!(snapshot.get(SECOND).map(String::as_str), Some("new-key"));
}
