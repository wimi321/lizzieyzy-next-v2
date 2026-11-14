//! OpenAI-compatible transport. No provider credentials or response bodies are logged.
use reqwest::{Client, Response, StatusCode, Url, header};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;
use ts_rs::TS;

mod sse;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_BODY_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS, thiserror::Error)]
#[serde(tag = "type", content = "data")]
pub enum CommentaryClientError {
    #[error("commentary service is not configured")]
    NotConfigured,
    #[error("invalid commentary service address")]
    InvalidAddress,
    #[error("invalid commentary API key")]
    InvalidKey,
    #[error("commentary HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("commentary rate limited: {body}")]
    RateLimited {
        retry_after: Option<String>,
        body: String,
    },
    #[error("commentary network request failed")]
    Network,
    #[error("commentary request timed out")]
    Timeout,
    #[error("commentary answer was interrupted")]
    Interrupted,
    #[error("invalid commentary service response")]
    InvalidResponse,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentaryMessage {
    pub role: String,
    pub content: String,
}

#[derive(Clone)]
pub struct CommentaryClient {
    client: Client,
    base: String,
}

impl std::fmt::Debug for CommentaryClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CommentaryClient")
            .finish_non_exhaustive()
    }
}

/// Only explicit loopback authorities may use cleartext HTTP. URL parsing also
/// rejects unsupported schemes, userinfo, query and fragment before any I/O.
pub fn normalize_base_url(address: &str) -> Result<String, CommentaryClientError> {
    let address = address.trim();
    if address.is_empty() {
        return Err(CommentaryClientError::NotConfigured);
    }
    if address.chars().any(char::is_control) {
        return Err(CommentaryClientError::InvalidAddress);
    }
    let mut url = Url::parse(address).map_err(|_| CommentaryClientError::InvalidAddress)?;
    let authority = address
        .split_once("://")
        .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or_default());
    if !url.username().is_empty()
        || url.password().is_some()
        || authority.is_some_and(|value| value.contains('@'))
        || url.query().is_some()
        || url.fragment().is_some()
        || url.host_str().is_none()
    {
        return Err(CommentaryClientError::InvalidAddress);
    }
    let local_authority = authority.is_some_and(|value| {
        let host = if value.starts_with('[') {
            value
                .split_once(']')
                .map(|(host, _)| format!("{host}]"))
                .unwrap_or_default()
        } else {
            value
                .split(':')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase()
        };
        matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]")
    });
    if url.scheme() != "https" && !(url.scheme() == "http" && local_authority) {
        return Err(CommentaryClientError::InvalidAddress);
    }
    let path = url.path().trim_end_matches('/');
    let path = if path.is_empty() { "/v1" } else { path }.to_owned();
    url.set_path(&path);
    Ok(url.to_string().trim_end_matches('/').to_owned())
}

impl CommentaryClient {
    pub fn new(base: &str) -> Result<Self, CommentaryClientError> {
        let base = normalize_base_url(base)?;
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut builder = Client::builder()
            .tls_backend_rustls()
            .retry(reqwest::retry::never())
            .user_agent("LizzieYzy-Next-AI-Commentary")
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(15))
            .timeout(REQUEST_TIMEOUT);
        // Explicit loopback HTTP must stay local even when the host has a
        // global proxy configured (including the offline fixture servers).
        if base.starts_with("http://") {
            builder = builder.no_proxy();
        }
        let client = builder.build().map_err(map_network)?;
        Ok(Self { client, base })
    }

    /// Runs outside the application actor. Dropping this future cancels the
    /// in-flight body read; deltas are delivered before the terminal frame.
    pub async fn complete(
        &self,
        key: &str,
        model: &str,
        messages: &[CommentaryMessage],
        mut on_delta: impl FnMut(String),
    ) -> Result<String, CommentaryClientError> {
        validate_key(key)?;
        if model.trim().is_empty() {
            return Err(CommentaryClientError::NotConfigured);
        }
        tokio::time::timeout(REQUEST_TIMEOUT, async {
            let response = self
                .post(
                    "chat/completions",
                    key,
                    json!({
                        "model": model, "stream": true, "messages": messages,
                    }),
                )
                .await?;
            if matches!(
                response.status(),
                StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
            ) {
                // Only these two route errors permit one compatibility fallback.
                drop(response);
                let response = self
                    .post(
                        "responses",
                        key,
                        json!({
                            "model": model, "stream": true, "input": messages,
                        }),
                    )
                    .await?;
                let response = require_success(response, key).await?;
                sse::read(response, true, &mut on_delta).await
            } else {
                let response = require_success(response, key).await?;
                sse::read(response, false, &mut on_delta).await
            }
        })
        .await
        .map_err(|_| CommentaryClientError::Timeout)?
    }

    pub async fn models(&self, key: &str) -> Result<Vec<String>, CommentaryClientError> {
        validate_key(key)?;
        tokio::time::timeout(REQUEST_TIMEOUT, async {
            let response = self
                .client
                .get(format!("{}/models", self.base))
                .header(header::AUTHORIZATION, authorization(key)?)
                .send()
                .await
                .map_err(map_network)?;
            let response = require_success(response, key).await?;
            let body = read_bounded(response, MAX_BODY_BYTES).await?;
            let value: serde_json::Value = serde_json::from_slice(&body)
                .map_err(|_| CommentaryClientError::InvalidResponse)?;
            let data = value
                .get("data")
                .and_then(|value| value.as_array())
                .ok_or(CommentaryClientError::InvalidResponse)?;
            let mut models = Vec::new();
            for model in data {
                let id = model
                    .get("id")
                    .and_then(|id| id.as_str())
                    .filter(|id| !id.trim().is_empty())
                    .ok_or(CommentaryClientError::InvalidResponse)?;
                if !models.iter().any(|existing| existing == id) {
                    models.push(id.to_owned());
                }
            }
            Ok(models)
        })
        .await
        .map_err(|_| CommentaryClientError::Timeout)?
    }

    async fn post(
        &self,
        route: &str,
        key: &str,
        body: serde_json::Value,
    ) -> Result<Response, CommentaryClientError> {
        self.client
            .post(format!("{}/{route}", self.base))
            .header(header::AUTHORIZATION, authorization(key)?)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "text/event-stream")
            .body(body.to_string())
            .send()
            .await
            .map_err(map_network)
    }
}

pub(crate) fn validate_key(key: &str) -> Result<(), CommentaryClientError> {
    if key.is_empty() {
        return Err(CommentaryClientError::NotConfigured);
    }
    if key.len() > 8192 || key.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(CommentaryClientError::InvalidKey);
    }
    Ok(())
}

fn authorization(key: &str) -> Result<header::HeaderValue, CommentaryClientError> {
    let mut value = header::HeaderValue::from_str(&format!("Bearer {key}"))
        .map_err(|_| CommentaryClientError::InvalidKey)?;
    value.set_sensitive(true);
    Ok(value)
}

fn map_network(error: reqwest::Error) -> CommentaryClientError {
    // reqwest errors may include request URLs; never retain their Display or
    // Debug text, headers or source chains in externally visible errors/logs.
    if error.is_timeout() {
        CommentaryClientError::Timeout
    } else {
        CommentaryClientError::Network
    }
}

async fn read_bounded(
    mut response: Response,
    maximum: usize,
) -> Result<Vec<u8>, CommentaryClientError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(map_network)? {
        if bytes.len().saturating_add(chunk.len()) > maximum {
            return Err(CommentaryClientError::InvalidResponse);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

async fn require_success(
    mut response: Response,
    key: &str,
) -> Result<Response, CommentaryClientError> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status().as_u16();
    let retry_after = response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .map(|value| redact(value, key));
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(map_network)? {
        let remaining = (64 * 1024_usize).saturating_sub(body.len());
        body.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        if body.len() >= 64 * 1024 {
            break;
        }
    }
    let body = redact(&String::from_utf8_lossy(&body), key);
    Err(if status == 429 {
        CommentaryClientError::RateLimited { retry_after, body }
    } else {
        CommentaryClientError::Http { status, body }
    })
}

fn redact(input: &str, key: &str) -> String {
    let mut text = input.replace(key, "[REDACTED]");
    if let Ok(escaped) = serde_json::to_string(key) {
        text = text.replace(escaped.trim_matches('"'), "[REDACTED]");
    }
    let mut output = String::new();
    let mut rest = text.as_str();
    while let Some(offset) = rest.to_ascii_lowercase().find("bearer") {
        let end = offset + 6;
        output.push_str(&rest[..end]);
        rest = &rest[end..];
        let whitespace = rest.len() - rest.trim_start().len();
        if whitespace == 0 {
            continue;
        }
        output.push(' ');
        rest = &rest[whitespace..];
        if let Some(quote @ ('\'' | '"')) = rest.chars().next() {
            rest = &rest[quote.len_utf8()..];
            let end = rest.find(quote).unwrap_or(rest.len());
            output.push_str("[REDACTED]");
            rest = &rest[end..];
            if rest.starts_with(quote) {
                rest = &rest[quote.len_utf8()..];
            }
            continue;
        }
        let token_end = rest
            .find(|c: char| {
                c.is_whitespace() || matches!(c, '"' | '\'' | ',' | ';' | '}' | '<' | '>')
            })
            .unwrap_or(rest.len());
        if token_end > 0 {
            output.push_str("[REDACTED]");
        }
        rest = &rest[token_end..];
    }
    output.push_str(rest);
    output.chars().take(240).collect()
}
