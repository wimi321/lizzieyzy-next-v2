//! One-shot HTTP(S) kifu loading. SGF decoding belongs to the application's
//! existing import path; this module returns untouched bytes for generic URLs.
use reqwest::Url;
use thiserror::Error;

use crate::{FoxClient, FoxError, REQUEST_TOTAL_TIMEOUT, transport};

#[derive(Debug, Error)]
pub enum OpenUrlError {
    #[error("invalid online URL")]
    InvalidUrl,
    #[error("only HTTP and HTTPS URLs are supported")]
    UnsupportedScheme,
    #[error("Yike links are not supported yet")]
    UnsupportedYike,
    #[error("online URL exceeded five redirects")]
    RedirectLimit,
    #[error("invalid online URL redirect")]
    InvalidRedirect,
    #[error(transparent)]
    Request(#[from] FoxError),
    #[error("online response is not a game record")]
    InvalidSgf,
}

fn validate(url: &Url) -> Result<(), OpenUrlError> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(OpenUrlError::UnsupportedScheme);
    }
    let host = url.host_str().ok_or(OpenUrlError::InvalidUrl)?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(OpenUrlError::InvalidUrl);
    }
    let host = host.trim_end_matches('.');
    if host == "yikeweiqi.com" || host.ends_with(".yikeweiqi.com") {
        return Err(OpenUrlError::UnsupportedYike);
    }
    Ok(())
}

impl FoxClient {
    pub async fn fetch_url(&self, input: &str) -> Result<Vec<u8>, OpenUrlError> {
        let input = input.trim_matches(|c: char| c.is_whitespace() || c == '\'' || c == '"');
        let url = Url::parse(input).map_err(|_| OpenUrlError::InvalidUrl)?;
        validate(&url)?;
        let share_host = url.host_str().is_some_and(|host| {
            let host = host.to_ascii_lowercase();
            host == "foxwq.com"
                || host.ends_with(".foxwq.com")
                || host == "qq.com"
                || host.ends_with(".qq.com")
        });
        if share_host
            && let Some((_, chessid)) = url.query_pairs().find(|(key, _)| key == "chessid")
        {
            return Ok(self.download(&chessid).await?.into_bytes());
        }
        tokio::time::timeout(REQUEST_TOTAL_TIMEOUT, self.fetch_once(url))
            .await
            .map_err(|_| FoxError::Timeout)?
    }

    async fn fetch_once(&self, mut url: Url) -> Result<Vec<u8>, OpenUrlError> {
        for redirects in 0..=5 {
            let response = self
                .online_http
                .get(url.clone())
                .send()
                .await
                .map_err(FoxError::from)?;
            if matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .ok_or(OpenUrlError::InvalidRedirect)?;
                let next = url
                    .join(location)
                    .map_err(|_| OpenUrlError::InvalidRedirect)?;
                validate(&next)?;
                if redirects == 5 {
                    return Err(OpenUrlError::RedirectLimit);
                }
                url = next;
            } else {
                return Ok(transport::body(response).await?);
            }
        }
        Err(OpenUrlError::RedirectLimit)
    }
}

#[cfg(test)]
mod tests;
