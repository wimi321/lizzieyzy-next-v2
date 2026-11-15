use super::*;
use reqwest::{Method, header::HeaderMap};
use serde_json::{Value, json};
use std::time::Duration;

mod decode;

#[derive(Clone)]
pub struct ZhiziClient {
    client: reqwest::Client,
    base: url::Url,
}
impl std::fmt::Debug for ZhiziClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ZhiziClient")
    }
}

pub(super) struct Response {
    pub value: Value,
    pub retry_after: Option<u32>,
    pub request_id: String,
}
impl Response {
    fn incomplete(&self) -> ZhiziError {
        ZhiziError {
            status: 200,
            request_id: self.request_id.clone(),
            ..ZhiziError::new(ZhiziErrorKind::IncompleteResponse)
        }
    }
    fn token(&self) -> Result<String, ZhiziError> {
        self.value
            .get("token")
            .and_then(Value::as_str)
            .filter(|v| valid_token(v))
            .map(str::to_owned)
            .ok_or_else(|| self.incomplete())
    }
}

impl ZhiziClient {
    pub fn new() -> Result<Self, ZhiziError> {
        Self::with_base_url("https://www.zhizigo.com")
    }
    /// Endpoint injection permits HTTP only on loopback for offline tests.
    pub fn with_base_url(base: &str) -> Result<Self, ZhiziError> {
        let base =
            url::Url::parse(base).map_err(|_| ZhiziError::new(ZhiziErrorKind::UnsafeAddress))?;
        let loopback = base.host_str().is_some_and(|host| {
            host.parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
        });
        if !(base.scheme() == "https" || base.scheme() == "http" && loopback)
            || base.host().is_none()
            || !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
        {
            return Err(ZhiziError::new(ZhiziErrorKind::UnsafeAddress));
        }
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut builder = reqwest::Client::builder()
            .tls_backend_rustls()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(20))
            .user_agent("LizzieYzy-Next2");
        if loopback {
            builder = builder.no_proxy();
        }
        let client = builder
            .build()
            .map_err(|_| ZhiziError::new(ZhiziErrorKind::Network))?;
        Ok(Self { client, base })
    }

    pub(super) async fn login(
        &self,
        account: &str,
        secret: &str,
        code: bool,
    ) -> Result<String, ZhiziError> {
        let mut body = identifier(account)?;
        let path = if code { "fast-login" } else { "login" };
        body[if code { "verificationCode" } else { "password" }] = json!(secret);
        self.request(
            Method::POST,
            &format!("/api/cluster/account/{path}"),
            Some(body),
            None,
            if code {
                "invalid_verification_code"
            } else {
                "invalid_credentials"
            },
        )
        .await?
        .token()
    }
    pub(super) async fn reset(
        &self,
        account: &str,
        code: &str,
        password: &str,
    ) -> Result<String, ZhiziError> {
        if password.chars().count() < 8 {
            return Err(ZhiziError::new(ZhiziErrorKind::PasswordTooShort));
        }
        let mut body = identifier(account)?;
        body["verificationCode"] = json!(code);
        body["password"] = json!(password);
        self.request(
            Method::POST,
            "/api/cluster/account/reset-password",
            Some(body),
            None,
            "invalid_verification_code",
        )
        .await?
        .token()
    }
    pub(super) async fn send_code(
        &self,
        account: &str,
        purpose: ZhiziVerificationPurpose,
    ) -> Result<ZhiziCodeDelivery, ZhiziError> {
        let mut body = identifier(account)?;
        body["type"] = json!(match purpose {
            ZhiziVerificationPurpose::FastLogin => "fast_login",
            ZhiziVerificationPurpose::ResetPassword => "reset_password",
        });
        let response = self
            .request(
                Method::POST,
                "/api/cluster/account/send-code",
                Some(body),
                None,
                "unauthorized",
            )
            .await?;
        Ok(ZhiziCodeDelivery {
            retry_after_seconds: response.retry_after.unwrap_or(60),
            may_register_account: purpose == ZhiziVerificationPurpose::FastLogin,
        })
    }
    pub(super) async fn socket_token(
        &self,
        token: &str,
        args: &str,
    ) -> Result<ZhiziSocketToken, ZhiziError> {
        let response = self
            .request(
                Method::POST,
                "/api/cluster/account/fetch-socketio-token",
                Some(json!({"args":args})),
                Some(token),
                "unauthorized",
            )
            .await?;
        let token = response.token()?;
        let address = response
            .value
            .get("socketIOURL")
            .and_then(Value::as_str)
            .ok_or_else(|| response.incomplete())?;
        let url = url::Url::parse(address).map_err(|_| response.incomplete())?;
        if !matches!(url.scheme(), "https" | "wss")
            || url.host().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(ZhiziError {
                request_id: response.request_id,
                ..ZhiziError::new(ZhiziErrorKind::UnsafeAddress)
            });
        }
        Ok(ZhiziSocketToken {
            url: address.to_owned(),
            token,
        })
    }
    pub(super) async fn account(&self, token: &str) -> Result<ZhiziAccountProfile, ZhiziError> {
        decode::account(&self.get("/api/cluster/account/me", token).await?)
    }
    pub(super) async fn balance(&self, token: &str) -> Result<ZhiziBalance, ZhiziError> {
        decode::balance(&self.get("/api/cluster/balance", token).await?)
    }
    pub(super) async fn usages(
        &self,
        token: &str,
        page: u32,
        size: u32,
    ) -> Result<ZhiziUsagePage, ZhiziError> {
        let response = self
            .get(
                &format!("/api/cluster/usage/my-usages?page={page}&pageSize={size}"),
                token,
            )
            .await?;
        decode::usages(&response, page, size)
    }
    pub(super) async fn credits(
        &self,
        token: &str,
        page: u32,
    ) -> Result<ZhiziCreditPage, ZhiziError> {
        let response = self
            .get(
                &format!("/api/cluster/credit/my-credits?page={page}&pageSize=20"),
                token,
            )
            .await?;
        decode::credits(&response, page)
    }
    async fn get(&self, path: &str, token: &str) -> Result<Response, ZhiziError> {
        self.request(Method::GET, path, None, Some(token), "unauthorized")
            .await
    }
    async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        token: Option<&str>,
        unauthorized: &str,
    ) -> Result<Response, ZhiziError> {
        let url = self
            .base
            .join(path)
            .map_err(|_| ZhiziError::new(ZhiziErrorKind::UnsafeAddress))?;
        let sensitive: Vec<&str> = body
            .as_ref()
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|map| map.values())
            .filter_map(Value::as_str)
            .chain(token)
            .collect();
        let mut request = self
            .client
            .request(method, url)
            .header("Accept", "application/json");
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = &body {
            request = request
                .header("Content-Type", "application/json")
                .body(body.to_string());
        }
        let mut response = request
            .send()
            .await
            .map_err(|_| ZhiziError::new(ZhiziErrorKind::Network))?;
        let status = response.status().as_u16();
        let request_id = request_id(response.headers(), &sensitive);
        let retry_after = retry_after(response.headers());
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| ZhiziError {
            status,
            request_id: request_id.clone(),
            retry_after_seconds: retry_after.unwrap_or(0),
            ..ZhiziError::new(ZhiziErrorKind::Network)
        })? {
            if bytes.len().saturating_add(chunk.len()) > 1024 * 1024 {
                return Err(ZhiziError {
                    status,
                    request_id,
                    ..ZhiziError::new(ZhiziErrorKind::IncompleteResponse)
                });
            }
            bytes.extend_from_slice(&chunk);
        }
        // Neither URLs, response bodies nor credentials enter diagnostic logs.
        tracing::info!(status, "Zhizi HTTP request completed");
        let parsed = serde_json::from_slice::<Value>(&bytes);
        if !(200..300).contains(&status) {
            let key = parsed
                .as_ref()
                .ok()
                .and_then(|v| v.get("key"))
                .and_then(Value::as_str)
                .map(str::to_owned)
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| {
                    if status == 401 {
                        unauthorized
                    } else {
                        "unknown_error"
                    }
                    .to_owned()
                });
            let mut error = ZhiziError {
                request_id,
                retry_after_seconds: retry_after.unwrap_or(0),
                ..ZhiziError::from_key(&key, status)
            };
            if let ZhiziErrorKind::Unknown(key) = &mut error.kind {
                *key = redact(key, &sensitive);
            }
            return Err(error);
        }
        let value = if bytes.is_empty() && path.ends_with("/send-code") {
            json!({})
        } else {
            parsed.map_err(|_| ZhiziError {
                status,
                request_id: request_id.clone(),
                ..ZhiziError::new(ZhiziErrorKind::IncompleteResponse)
            })?
        };
        if !value.is_object() {
            return Err(ZhiziError {
                status,
                request_id,
                ..ZhiziError::new(ZhiziErrorKind::IncompleteResponse)
            });
        }
        let request_id = value
            .get("token")
            .and_then(Value::as_str)
            .map(|token| redact(&request_id, &[token]))
            .unwrap_or(request_id);
        Ok(Response {
            value,
            retry_after,
            request_id,
        })
    }
}

pub(super) fn valid_token(value: &str) -> bool {
    !value.is_empty() && value.len() <= 16 * 1024 && !value.chars().any(char::is_control)
}

pub(super) fn identifier(account: &str) -> Result<Value, ZhiziError> {
    let account = account.trim();
    let email = account.contains('@');
    let valid = !account.is_empty()
        && account.len() <= 320
        && !account.chars().any(char::is_whitespace)
        && if email {
            account.split_once('@').is_some_and(|(left, right)| {
                !left.is_empty() && !right.is_empty() && !right.contains('@')
            })
        } else {
            account
                .strip_prefix('+')
                .unwrap_or(account)
                .bytes()
                .all(|v| v.is_ascii_digit())
                && account.bytes().filter(u8::is_ascii_digit).count() >= 5
        };
    if !valid {
        return Err(ZhiziError::new(ZhiziErrorKind::InvalidAccount));
    }
    Ok(if email {
        json!({"email":account})
    } else {
        json!({"phone":account})
    })
}

fn redact(value: &str, secrets: &[&str]) -> String {
    if secrets
        .iter()
        .any(|secret| !secret.is_empty() && value.contains(secret))
    {
        return "redacted".to_owned();
    }
    value.to_owned()
}
fn request_id(headers: &HeaderMap, secrets: &[&str]) -> String {
    ["x-request-id", "x-correlation-id", "traceparent"]
        .iter()
        .find_map(|name| {
            headers
                .get(*name)
                .and_then(|v| v.to_str().ok())
                .filter(|v| !v.trim().is_empty())
        })
        .map(|v| {
            redact(v.trim(), secrets)
                .chars()
                .filter(|ch| !ch.is_control())
                .take(128)
                .collect()
        })
        .unwrap_or_default()
}
fn retry_after(headers: &HeaderMap) -> Option<u32> {
    let value = headers.get("retry-after")?.to_str().ok()?.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(seconds.min(600) as u32);
    }
    let date = chrono::DateTime::parse_from_rfc2822(value).ok()?;
    Some((date.timestamp() - chrono::Utc::now().timestamp()).clamp(0, 600) as u32)
}
