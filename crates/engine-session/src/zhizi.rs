//! Ephemeral connection material supplied by app-core, never persisted here.
use crate::RemoteTransportError;
use std::{future::Future, pin::Pin, sync::Arc};

type ConnectFuture =
    Pin<Box<dyn Future<Output = Result<ZhiziConnection, RemoteTransportError>> + Send>>;

#[derive(Clone)]
pub struct ZhiziConnector(Arc<dyn Fn() -> ConnectFuture + Send + Sync>);

impl ZhiziConnector {
    pub fn new<F, Fut>(connect: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<ZhiziConnection, RemoteTransportError>> + Send + 'static,
    {
        Self(Arc::new(move || Box::pin(connect())))
    }

    pub(crate) async fn connect(&self) -> Result<ZhiziConnection, RemoteTransportError> {
        (self.0)().await
    }
}

impl std::fmt::Debug for ZhiziConnector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ZhiziConnector([redacted])")
    }
}

#[derive(Clone)]
pub struct ZhiziConnection {
    pub(crate) url: String,
    pub(crate) token: String,
    #[cfg(test)]
    pub(crate) tls: Option<tokio_tungstenite::Connector>,
}

impl ZhiziConnection {
    pub fn new(url: String, token: String) -> Self {
        Self {
            url,
            token,
            #[cfg(test)]
            tls: None,
        }
    }
}

impl std::fmt::Debug for ZhiziConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ZhiziConnection([redacted])")
    }
}

impl RemoteTransportError {
    /// Only documented status/code values decide whether retry is forbidden.
    /// Human-readable server messages are deliberately ignored.
    pub fn zhizi_fatal(status: Option<u16>, key: &str) -> Option<Self> {
        if matches!(status, Some(401 | 403)) || matches!(key, "unauthorized" | "forbidden") {
            return Some(Self::ZhiziAccessDenied);
        }
        match key {
            "insufficient_balance" | "balance_not_enough" | "balance_insufficient" => {
                Some(Self::ZhiziBalance)
            }
            "vip_expired" | "vip_required" | "not_vip" | "membership_expired"
            | "not_membership" => Some(Self::ZhiziVip),
            _ => None,
        }
    }
}
