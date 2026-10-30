//! Public Fox account and kifu HTTP API. This crate owns no application state.

mod error;
mod normalize;
mod parse;
mod types;

pub use error::FoxError;
pub use parse::format_rank;
pub use types::*;

use std::time::Duration;

use reqwest::{Client, Request, RequestBuilder, Url};
use serde_json::Value;

const MOBILE_USER_AGENT: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const REQUEST_TOTAL_TIMEOUT: Duration = Duration::from_secs(3 * 60);
const MAX_TRANSPORT_ATTEMPTS: u32 = 3;

/// Full endpoint URLs, injectable for local HTTP replay. All but the last SGF
/// endpoint receive a POST form; the last receives the legacy H5 GET request.
#[derive(Clone, Debug)]
pub struct FoxEndpoints {
    pub user_url: String,
    pub list_url: String,
    pub sgf_urls: Vec<String>,
}

impl Default for FoxEndpoints {
    fn default() -> Self {
        Self {
            user_url: "https://newframe.foxwq.com/cgi/QueryUserInfoPanel".into(),
            list_url: "https://h5.foxwq.com/yehuDiamond/chessbook_local/YHWQFetchChessList".into(),
            sgf_urls: vec![
                "http://happyapp.huanle.qq.com/cgi-bin/CommonMobileCGI/TXWQFetchChess".into(),
                "http://cgi.foxwq.com/cgi-bin/CommonMobileCGI/TXWQFetchChess".into(),
                "https://h5.foxwq.com/yehuDiamond/chessbook_local/YHWQFetchChess".into(),
            ],
        }
    }
}

#[derive(Clone, Debug)]
pub struct FoxClient {
    http: Client,
    endpoints: FoxEndpoints,
}

impl FoxClient {
    pub fn new() -> Result<Self, FoxError> {
        Self::with_endpoints(FoxEndpoints::default(), Duration::from_secs(25))
    }

    /// `read_timeout` limits idle time waiting for response data, not the time
    /// required to transfer an entire page. Each operation has a three-minute
    /// ceiling including retries; connections have a separate 20-second limit.
    pub fn with_endpoints(
        endpoints: FoxEndpoints,
        read_timeout: Duration,
    ) -> Result<Self, FoxError> {
        if read_timeout.is_zero()
            || read_timeout > Duration::from_secs(24 * 60 * 60)
            || endpoints.sgf_urls.is_empty()
        {
            return Err(FoxError::InvalidConfiguration);
        }
        for endpoint in [&endpoints.user_url, &endpoints.list_url]
            .into_iter()
            .chain(endpoints.sgf_urls.iter())
        {
            let url = Url::parse(endpoint).map_err(|_| FoxError::InvalidConfiguration)?;
            if !matches!(url.scheme(), "https" | "http")
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err(FoxError::InvalidConfiguration);
            }
        }
        // reqwest's no-provider feature requires a runtime provider. A previous
        // installation is harmless and is retained for other rustls clients.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = Client::builder()
            .tls_backend_rustls()
            .user_agent(MOBILE_USER_AGENT)
            .connect_timeout(Duration::from_secs(20))
            .read_timeout(read_timeout)
            .timeout(REQUEST_TOTAL_TIMEOUT)
            // Count all transport retries here, including protocol NACKs.
            .retry(reqwest::retry::never())
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()?;
        Ok(Self { http, endpoints })
    }

    pub async fn search(&self, query: &FoxQuery) -> Result<FoxSearchResult, FoxError> {
        let mut account = match query {
            FoxQuery::Nickname(nickname) => self.lookup(nickname).await?,
            FoxQuery::Uid(uid) => {
                validate_uid(uid)?;
                FoxAccount {
                    uid: uid.clone(),
                    nickname: String::new(),
                }
            }
        };
        let page = self.list(&account.uid, None).await?;
        if let Some(player) = page
            .games
            .iter()
            .flat_map(|game| [&game.black, &game.white])
            .find(|player| player.uid == account.uid && !player.nickname.is_empty())
        {
            account.nickname.clone_from(&player.nickname);
        }
        Ok(FoxSearchResult { account, page })
    }

    pub async fn lookup(&self, nickname: &str) -> Result<FoxAccount, FoxError> {
        let nickname = nickname.trim();
        if nickname.is_empty() || nickname.len() > 256 {
            return Err(FoxError::InvalidNickname);
        }
        let response = self
            .json(
                self.http
                    .get(&self.endpoints.user_url)
                    .query(&[("srcuid", "0"), ("username", nickname)]),
            )
            .await?;
        parse::account(&response, nickname)
    }

    pub async fn list(&self, uid: &str, cursor: Option<&str>) -> Result<FoxPage, FoxError> {
        validate_uid(uid)?;
        let cursor = cursor.unwrap_or("0");
        validate_game_id(cursor)?;
        let response = self
            .json(self.http.get(&self.endpoints.list_url).query(&[
                ("srcuid", "0"),
                ("dstuid", uid),
                ("type", "1"),
                ("lastcode", cursor),
                ("searchkey", ""),
                ("uin", uid),
            ]))
            .await?;
        parse::page(&response, cursor)
    }

    pub async fn download(&self, game_id: &str) -> Result<String, FoxError> {
        validate_game_id(game_id)?;
        let mut failures = Vec::new();
        for index in 0..self.endpoints.sgf_urls.len() {
            match self.download_from(index, game_id).await {
                Ok((sgf, _)) => return Ok(sgf),
                Err(error) => failures.push(error),
            }
        }
        Err(FoxError::DownloadFailed { failures })
    }

    async fn download_from(
        &self,
        index: usize,
        game_id: &str,
    ) -> Result<(String, Value), FoxError> {
        let url = &self.endpoints.sgf_urls[index];
        let request = if index + 1 == self.endpoints.sgf_urls.len() {
            self.http.get(url).query(&[("chessid", game_id)])
        } else {
            self.http
                .post(url)
                .header("User-Agent", "okhttp/3.12.12")
                .form(&[("chessid", game_id)])
        };
        let response = self.json(request).await?;
        parse::success(&response)?;
        let sgf = response
            .get("chess")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .ok_or(FoxError::InvalidResponse("chess"))?
            .to_owned();
        Ok((normalize::normalize_sgf(&sgf)?, response))
    }

    async fn json(&self, request: RequestBuilder) -> Result<Value, FoxError> {
        let request = request
            .header("Accept", "application/json,text/plain,*/*")
            .build()?;
        tokio::time::timeout(REQUEST_TOTAL_TIMEOUT, self.json_with_retries(request))
            .await
            .map_err(|_| FoxError::Timeout)?
    }

    async fn json_with_retries(&self, request: Request) -> Result<Value, FoxError> {
        for attempt in 1..=MAX_TRANSPORT_ATTEMPTS {
            let request = request.try_clone().ok_or(FoxError::InvalidConfiguration)?;
            match self.json_once(request).await {
                Err(error)
                    if error.is_retryable_transport() && attempt < MAX_TRANSPORT_ATTEMPTS =>
                {
                    tokio::time::sleep(Duration::from_millis(350 * u64::from(attempt))).await;
                }
                result => return result,
            }
        }
        Err(FoxError::InvalidConfiguration)
    }

    async fn json_once(&self, request: Request) -> Result<Value, FoxError> {
        let mut response = self.http.execute(request).await?;
        if !response.status().is_success() {
            return Err(FoxError::Http {
                status: response.status().as_u16(),
            });
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
        {
            return Err(FoxError::ResponseTooLarge);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
                return Err(FoxError::ResponseTooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(serde_json::from_slice(&bytes)?)
    }
}

pub fn validate_uid(uid: &str) -> Result<(), FoxError> {
    if uid.is_empty() || uid.len() > 128 || !uid.bytes().all(|byte| byte.is_ascii_digit()) {
        Err(FoxError::InvalidUid)
    } else {
        Ok(())
    }
}

fn validate_game_id(id: &str) -> Result<(), FoxError> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        Err(FoxError::InvalidGameId)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod transport_tests;
