//! Shared bounded HTTP transport for public kifu services.
use std::time::Duration;

use reqwest::{Client, Request, RequestBuilder, Response, Url};
use serde_json::Value;

use crate::{FoxError, MAX_RESPONSE_BYTES, MOBILE_USER_AGENT, REQUEST_TOTAL_TIMEOUT};

const MAX_TRANSPORT_ATTEMPTS: u32 = 3;

pub(crate) fn validate_endpoint(endpoint: &str) -> Result<(), FoxError> {
    let url = Url::parse(endpoint).map_err(|_| FoxError::InvalidConfiguration)?;
    if !matches!(url.scheme(), "https" | "http")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(FoxError::InvalidConfiguration);
    }
    Ok(())
}

pub(crate) fn client(
    read_timeout: Duration,
    redirects: reqwest::redirect::Policy,
) -> Result<Client, FoxError> {
    if read_timeout.is_zero() || read_timeout > Duration::from_secs(24 * 60 * 60) {
        return Err(FoxError::InvalidConfiguration);
    }
    // Keep any provider already installed by another rustls client.
    let _ = rustls::crypto::ring::default_provider().install_default();
    Ok(Client::builder()
        .tls_backend_rustls()
        .user_agent(MOBILE_USER_AGENT)
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(read_timeout)
        .timeout(REQUEST_TOTAL_TIMEOUT)
        .retry(reqwest::retry::never())
        .redirect(redirects)
        .build()?)
}

pub(crate) async fn json(http: &Client, request: RequestBuilder) -> Result<Value, FoxError> {
    let request = request
        .header("Accept", "application/json,text/plain,*/*")
        .build()?;
    tokio::time::timeout(REQUEST_TOTAL_TIMEOUT, json_with_retries(http, request))
        .await
        .map_err(|_| FoxError::Timeout)?
}

async fn json_with_retries(http: &Client, request: Request) -> Result<Value, FoxError> {
    for attempt in 1..=MAX_TRANSPORT_ATTEMPTS {
        let request = request.try_clone().ok_or(FoxError::InvalidConfiguration)?;
        let result: Result<Value, FoxError> = async {
            let response = http.execute(request).await?;
            let bytes = body(response).await?;
            Ok(serde_json::from_slice(&bytes)?)
        }
        .await;
        match result {
            Err(ref error)
                if error.is_retryable_transport() && attempt < MAX_TRANSPORT_ATTEMPTS =>
            {
                tokio::time::sleep(Duration::from_millis(350 * u64::from(attempt))).await;
            }
            result => return result,
        }
    }
    Err(FoxError::InvalidConfiguration)
}

pub(crate) async fn body(mut response: Response) -> Result<Vec<u8>, FoxError> {
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
    Ok(bytes)
}
