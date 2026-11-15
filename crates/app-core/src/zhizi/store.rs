//! Preserve native operation order even when a caller stops waiting. A native
//! keychain operation may already be running in spawn_blocking and cannot be
//! cancelled by dropping its future; a timed-out write must precede its delete.
use crate::commentary_keys::{KeyStoreFuture, KeyStoreUnavailable, SecretStore};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

type Reply<T> = oneshot::Sender<Result<T, KeyStoreUnavailable>>;
enum Command {
    Read(String, Reply<Option<String>>),
    Write(String, String, Reply<()>),
    Delete(String, Reply<()>),
}

#[derive(Debug)]
struct OrderedStore {
    commands: mpsc::Sender<Command>,
}

pub(super) fn ordered(store: Arc<dyn SecretStore>) -> Arc<dyn SecretStore> {
    let (commands, mut receive) = mpsc::channel(16);
    tokio::spawn(async move {
        while let Some(command) = receive.recv().await {
            match command {
                Command::Read(account, reply) => {
                    let result = store.read(account).await;
                    let _ = reply.send(result);
                }
                Command::Write(account, token, reply) => {
                    let result = store.write(account, token).await;
                    let _ = reply.send(result);
                }
                Command::Delete(account, reply) => {
                    let result = store.delete(account).await;
                    let _ = reply.send(result);
                }
            }
        }
    });
    Arc::new(OrderedStore { commands })
}

impl SecretStore for OrderedStore {
    fn read(&self, account: String) -> KeyStoreFuture<'_, Option<String>> {
        Box::pin(async move {
            let (reply, receive) = oneshot::channel();
            self.commands
                .send(Command::Read(account, reply))
                .await
                .map_err(|_| KeyStoreUnavailable)?;
            receive.await.map_err(|_| KeyStoreUnavailable)?
        })
    }
    fn write(&self, account: String, token: String) -> KeyStoreFuture<'_, ()> {
        Box::pin(async move {
            let (reply, receive) = oneshot::channel();
            self.commands
                .send(Command::Write(account, token, reply))
                .await
                .map_err(|_| KeyStoreUnavailable)?;
            receive.await.map_err(|_| KeyStoreUnavailable)?
        })
    }
    fn delete(&self, account: String) -> KeyStoreFuture<'_, ()> {
        Box::pin(async move {
            let (reply, receive) = oneshot::channel();
            self.commands
                .send(Command::Delete(account, reply))
                .await
                .map_err(|_| KeyStoreUnavailable)?;
            receive.await.map_err(|_| KeyStoreUnavailable)?
        })
    }
}
