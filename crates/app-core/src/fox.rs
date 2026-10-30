//! Fox public-game operations. HTTP is polled by the calling future; only
//! history persistence and opening a completed download enter the app actor.

pub use fox_kifu::{
    FoxAccount, FoxClient, FoxColor, FoxEndpoints, FoxError, FoxGame, FoxPage, FoxPlayer, FoxQuery,
    FoxResult, FoxResultKind, FoxScoreUnit, FoxSearchResult,
};
use tokio::sync::oneshot;

use crate::{AppCore, AppError, actor};

pub(crate) enum Request {
    Client(oneshot::Sender<Result<FoxClient, AppError>>),
    Remember(FoxAccount, oneshot::Sender<Result<(), AppError>>),
    Clear(oneshot::Sender<Result<(), AppError>>),
}

impl AppCore {
    /// Resolve an explicitly selected nickname/UID and retrieve its first page.
    /// Only successful searches are remembered, using resolved account identity.
    pub async fn fox_search(&self, query: FoxQuery) -> Result<FoxSearchResult, AppError> {
        let client = self.fox_request(Request::Client).await?;
        let result = client.search(&query).await?;
        self.fox_request(|reply| Request::Remember(result.account.clone(), reply))
            .await?;
        Ok(result)
    }

    /// Request the next page using the previous page's next_cursor verbatim.
    /// Cursors belong to the caller's account/page, so concurrent searches do
    /// not overwrite a global pagination session and failures remain retryable.
    pub async fn fox_next_page(&self, uid: String, cursor: String) -> Result<FoxPage, AppError> {
        let client = self.fox_request(Request::Client).await?;
        Ok(client.list(&uid, Some(&cursor)).await?)
    }

    /// Downloads an SGF then uses exactly the local-SGF open transaction.
    pub async fn fox_open_game(&self, game_id: String) -> Result<u64, AppError> {
        let client = self.fox_request(Request::Client).await?;
        let text = client.download(&game_id).await?;
        self.open_bytes(text.into_bytes()).await
    }

    pub async fn fox_clear_history(&self) -> Result<(), AppError> {
        self.fox_request(Request::Clear).await
    }

    async fn fox_request<T>(
        &self,
        request: impl FnOnce(oneshot::Sender<Result<T, AppError>>) -> Request,
    ) -> Result<T, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::Fox(request(reply)))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
}

/// Settings may have been hand-edited or produced by an older release. Keep
/// the first (newest) valid entry for each UID and enforce the storage bound.
pub(crate) fn normalize_history(history: &mut Vec<FoxAccount>) {
    let mut uids = std::collections::HashSet::new();
    history.retain(|account| {
        fox_kifu::validate_uid(&account.uid).is_ok() && uids.insert(account.uid.clone())
    });
    history.truncate(10);
}
