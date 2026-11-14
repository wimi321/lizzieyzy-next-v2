//! Public Tencent queries and one-shot online imports. Network futures run in
//! the caller; only settings persistence and document replacement use the actor.

pub use fox_kifu::{
    online::OpenUrlError,
    tencent::{
        TencentClient, TencentEndpoints, TencentError, TencentGame, TencentPage, TencentPlayer,
        TencentQuery, TencentRank,
    },
};
use tokio::sync::oneshot;

use crate::{AppCore, AppError, actor};

pub(crate) enum Request {
    Client(oneshot::Sender<Result<TencentClient, AppError>>),
    Remember(TencentQuery, oneshot::Sender<Result<(), AppError>>),
    Clear(oneshot::Sender<Result<(), AppError>>),
}

impl AppCore {
    /// Remember the trimmed query only after successfully retrieving page one.
    pub async fn tencent_search(&self, query: TencentQuery) -> Result<TencentPage, AppError> {
        let query = query.normalized()?;
        let client = self.tencent_request(Request::Client).await?;
        let page = client.search(&query).await?;
        self.tencent_request(|reply| Request::Remember(query, reply))
            .await?;
        Ok(page)
    }

    /// Pagination belongs to this query and caller, not a shared actor cursor.
    pub async fn tencent_next_page(
        &self,
        query: TencentQuery,
        cursor: String,
    ) -> Result<TencentPage, AppError> {
        let query = query.normalized()?;
        let client = self.tencent_request(Request::Client).await?;
        Ok(client.list(&query, Some(&cursor)).await?)
    }

    pub async fn tencent_open_game(&self, chessid: String) -> Result<u64, AppError> {
        // The existing gate also rejects active game/match/update modes. The
        // open_bytes transaction checks again after the asynchronous download.
        self.send(actor::Input::EnsureNoBatch).await?;
        let client = self.tencent_request(Request::Client).await?;
        let text = client.download(&chessid).await?;
        self.open_bytes(text.into_bytes()).await
    }

    pub async fn tencent_clear_history(&self) -> Result<(), AppError> {
        self.tencent_request(Request::Clear).await
    }

    /// Fetch once, then reuse the ordinary encoding/parser/open transaction.
    pub async fn open_url(&self, url: String) -> Result<u64, AppError> {
        self.send(actor::Input::EnsureNoBatch).await?;
        let client = self.fox_request(crate::fox::Request::Client).await?;
        let bytes = client.fetch_url(&url).await?;
        self.open_bytes(bytes).await.map_err(|error| match error {
            AppError::Sgf(_) | AppError::EmptyCollection | AppError::Document(_) => {
                OpenUrlError::InvalidSgf.into()
            }
            other => other,
        })
    }

    async fn tencent_request<T>(
        &self,
        request: impl FnOnce(oneshot::Sender<Result<T, AppError>>) -> Request,
    ) -> Result<T, AppError> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::Tencent(request(reply)))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
}

pub(crate) fn normalize_history(history: &mut Vec<TencentQuery>) {
    let mut queries = std::collections::HashSet::new();
    *history = history
        .iter()
        .filter_map(|query| query.normalized().ok())
        .filter(|query| queries.insert(query.0.clone()))
        .take(10)
        .collect();
}
