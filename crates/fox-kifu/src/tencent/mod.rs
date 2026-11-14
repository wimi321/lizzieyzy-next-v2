//! Tencent's public kifu API; transport and SGF cleanup are shared with Fox.
mod parse;
mod types;

pub use types::*;

use std::time::Duration;

use reqwest::Client;
use thiserror::Error;

use crate::{FoxError, normalize, transport, validate_game_id};

#[derive(Debug, Error)]
pub enum TencentError {
    #[error("invalid Tencent query")]
    InvalidQuery,
    #[error("Tencent API returned result {result}, ret {ret:?}")]
    Api {
        result: i64,
        ret: Option<i64>,
        message: String,
    },
    #[error(transparent)]
    Common(#[from] FoxError),
}

#[derive(Clone, Debug)]
pub struct TencentEndpoints {
    pub list_url: String,
    pub sgf_url: String,
}

impl Default for TencentEndpoints {
    fn default() -> Self {
        Self {
            list_url: "https://cgi.huanle.qq.com/cgi-bin/CommonMobileCGI/TXWQFetchChessList".into(),
            sgf_url: "https://happyapp.huanle.qq.com/cgi-bin/CommonMobileCGI/TXWQFetchChess".into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TencentClient {
    http: Client,
    endpoints: TencentEndpoints,
}

impl TencentClient {
    pub fn new() -> Result<Self, TencentError> {
        Self::with_endpoints(TencentEndpoints::default(), Duration::from_secs(25))
    }

    pub fn with_endpoints(
        endpoints: TencentEndpoints,
        read_timeout: Duration,
    ) -> Result<Self, TencentError> {
        transport::validate_endpoint(&endpoints.list_url)?;
        transport::validate_endpoint(&endpoints.sgf_url)?;
        let http = transport::client(read_timeout, reqwest::redirect::Policy::limited(5))?;
        Ok(Self { http, endpoints })
    }

    pub async fn search(&self, query: &TencentQuery) -> Result<TencentPage, TencentError> {
        self.list(query, None).await
    }

    pub async fn list(
        &self,
        query: &TencentQuery,
        cursor: Option<&str>,
    ) -> Result<TencentPage, TencentError> {
        let query = query.normalized()?;
        let cursor = cursor.unwrap_or("0");
        validate_game_id(cursor)?;
        let srcuid = if query.0.bytes().all(|b| b.is_ascii_digit()) {
            query.0.as_str()
        } else {
            ""
        };
        let response = transport::json(
            &self.http,
            self.http.get(&self.endpoints.list_url).query(&[
                ("type", "7"),
                ("lastCode", cursor),
                ("username", &query.0),
                ("srcuid", srcuid),
                ("txwqsession", "lizzieyzy-next"),
                ("fetchnum", "100"),
            ]),
        )
        .await?;
        parse::page(&response, cursor)
    }

    pub async fn download(&self, chessid: &str) -> Result<String, TencentError> {
        validate_game_id(chessid)?;
        let response = transport::json(
            &self.http,
            self.http
                .get(&self.endpoints.sgf_url)
                .query(&[("chessid", chessid)]),
        )
        .await?;
        parse::success(&response)?;
        let sgf = response
            .get("chess")
            .and_then(serde_json::Value::as_str)
            .filter(|sgf| !sgf.trim().is_empty())
            .ok_or(FoxError::InvalidResponse("chess"))?;
        Ok(normalize::normalize_sgf(sgf)?)
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod transport_tests;
