use std::convert::Infallible;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use base64::{Engine, engine::general_purpose::STANDARD};
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::header::{
    CONNECTION, CONTENT_TYPE, HOST, ORIGIN, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY,
    SEC_WEBSOCKET_VERSION, UPGRADE,
};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
use tokio_tungstenite::tungstenite::protocol::{Role, WebSocketConfig};

use super::connection::{self, TransportEvent};
use super::security::resource_path;
use super::{WebBoardAssets, WebBoardError};

#[derive(Debug, Clone)]
pub(crate) struct Config {
    pub bind: Ipv4Addr,
    pub start_port: u16,
    pub analysis_interval: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind: Ipv4Addr::UNSPECIFIED,
            start_port: 9998,
            analysis_interval: Duration::from_millis(100),
        }
    }
}

pub(crate) async fn bind(config: &Config) -> Result<(TcpListener, u16), WebBoardError> {
    let mut last_error = None;
    for offset in 0..10 {
        let Some(port) = config.start_port.checked_add(offset) else {
            break;
        };
        match TcpListener::bind((config.bind, port)).await {
            Ok(listener) => {
                let actual_port = listener
                    .local_addr()
                    .map_err(|error| WebBoardError::Bind {
                        reason: error.to_string(),
                    })?
                    .port();
                return Ok((listener, actual_port));
            }
            Err(error) => {
                let occupied = error.kind() == std::io::ErrorKind::AddrInUse;
                last_error = Some(error);
                if !occupied || config.start_port == 0 {
                    break;
                }
            }
        }
    }
    Err(WebBoardError::Bind {
        reason: last_error
            .map(|error| error.to_string())
            .unwrap_or_default(),
    })
}

pub(crate) async fn run(
    listener: TcpListener,
    token: String,
    assets: Arc<dyn WebBoardAssets>,
    events: mpsc::Sender<TransportEvent>,
    mut stop: watch::Receiver<bool>,
    analysis_interval: Duration,
) {
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            _ = connection::stopped(&mut stop) => break,
            _ = connections.join_next(), if !connections.is_empty() => {},
            accepted = listener.accept() => {
                let Ok((stream, _)) = accepted else {
                    // Resource exhaustion may be temporary. Back off without
                    // spinning or abandoning active viewers and their owner.
                    tokio::select! {
                        biased;
                        _ = connection::stopped(&mut stop) => break,
                        _ = tokio::time::sleep(Duration::from_millis(250)) => {},
                    }
                    continue;
                };
                // HTTP handshakes also have a bounded lifetime and count, so an
                // unauthenticated peer cannot allocate unbounded socket tasks.
                if connections.len() >= 64 { continue; }
                connections.spawn(serve(
                    stream, token.clone(), assets.clone(), events.clone(),
                    stop.clone(), analysis_interval,
                ));
            },
        }
    }
    drop(listener);
    while connections.join_next().await.is_some() {}
}

struct UpgradeRequest {
    upgrade: hyper::upgrade::OnUpgrade,
    reconnect: Option<String>,
}

async fn serve(
    stream: TcpStream,
    token: String,
    assets: Arc<dyn WebBoardAssets>,
    events: mpsc::Sender<TransportEvent>,
    mut stop: watch::Receiver<bool>,
    analysis_interval: Duration,
) {
    let (upgrades, mut received) = mpsc::channel(1);
    let service = service_fn(move |request| {
        std::future::ready(Ok::<_, Infallible>(respond(
            request,
            &token,
            assets.as_ref(),
            &upgrades,
        )))
    });
    let mut builder = http1::Builder::new();
    builder
        // The upgrade handshake needs Connection: Upgrade to survive hyper's
        // response encoding. Ordinary responses explicitly close below.
        .keep_alive(true)
        .max_buf_size(8192)
        .max_headers(64)
        .timer(TokioTimer::new())
        .header_read_timeout(Duration::from_secs(5));
    let connection = builder
        .serve_connection(TokioIo::new(stream), service)
        .with_upgrades();
    tokio::select! {
        biased;
        _ = connection::stopped(&mut stop) => return,
        result = connection => { if result.is_err() { return; } },
    }
    let Ok(request) = received.try_recv() else {
        return;
    };
    let upgraded = tokio::select! {
        biased;
        _ = connection::stopped(&mut stop) => return,
        upgraded = request.upgrade => upgraded,
    };
    if let Ok(upgraded) = upgraded {
        let config = WebSocketConfig::default()
            .max_message_size(Some(4096))
            .max_frame_size(Some(4096))
            .write_buffer_size(0);
        let socket =
            WebSocketStream::from_raw_socket(TokioIo::new(upgraded), Role::Server, Some(config))
                .await;
        connection::run(socket, request.reconnect, events, stop, analysis_interval).await;
    }
}

fn respond(
    mut request: Request<Incoming>,
    token: &str,
    assets: &dyn WebBoardAssets,
    upgrades: &mpsc::Sender<UpgradeRequest>,
) -> Response<Full<Bytes>> {
    if request.method() != Method::GET {
        return empty(StatusCode::METHOD_NOT_ALLOWED);
    }
    let Some(path) = resource_path(request.uri().path(), token) else {
        return empty(StatusCode::NOT_FOUND);
    };
    if path != "ws" {
        let Some(asset) = assets.get(path) else {
            return empty(StatusCode::NOT_FOUND);
        };
        let Ok(mime) = asset.mime.parse() else {
            return empty(StatusCode::NOT_FOUND);
        };
        let mut response = Response::new(Full::new(Bytes::from(asset.bytes)));
        response
            .headers_mut()
            .insert(CONNECTION, hyper::header::HeaderValue::from_static("close"));
        response.headers_mut().insert(CONTENT_TYPE, mime);
        response.headers_mut().insert(
            "x-content-type-options",
            hyper::header::HeaderValue::from_static("nosniff"),
        );
        response.headers_mut().insert(
            "referrer-policy",
            hyper::header::HeaderValue::from_static("no-referrer"),
        );
        response.headers_mut().insert(
            "cache-control",
            hyper::header::HeaderValue::from_static("no-store"),
        );
        return response;
    }
    let headers = request.headers();
    let Some(host) = headers.get(HOST).and_then(|value| value.to_str().ok()) else {
        return empty(StatusCode::FORBIDDEN);
    };
    if headers.get_all(ORIGIN).iter().count() != 1
        || headers.get_all(HOST).iter().count() != 1
        || headers.get(ORIGIN).and_then(|value| value.to_str().ok())
            != Some(format!("http://{host}").as_str())
    {
        return empty(StatusCode::FORBIDDEN);
    }
    let has_upgrade = headers.get_all(CONNECTION).iter().any(|value| {
        value.to_str().is_ok_and(|value| {
            value
                .split(',')
                .any(|item| item.trim().eq_ignore_ascii_case("upgrade"))
        })
    });
    let websocket = headers
        .get(UPGRADE)
        .is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(b"websocket"));
    let version = headers
        .get(SEC_WEBSOCKET_VERSION)
        .is_some_and(|value| value == "13");
    let Some(key) = headers.get(SEC_WEBSOCKET_KEY).filter(|value| {
        STANDARD
            .decode(value.as_bytes())
            .is_ok_and(|decoded| decoded.len() == 16)
    }) else {
        return empty(StatusCode::BAD_REQUEST);
    };
    if !has_upgrade || !websocket || !version {
        return empty(StatusCode::BAD_REQUEST);
    }
    let Ok(accept) = derive_accept_key(key.as_bytes()).parse() else {
        return empty(StatusCode::BAD_REQUEST);
    };
    let reconnect = match request.uri().query() {
        None => None,
        Some(query) => {
            let Some(value) = query.strip_prefix("reconnect=").filter(|value| {
                value.len() == 22
                    && value
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
            }) else {
                return empty(StatusCode::BAD_REQUEST);
            };
            Some(value.to_owned())
        }
    };
    let upgrade = hyper::upgrade::on(&mut request);
    if upgrades
        .try_send(UpgradeRequest { upgrade, reconnect })
        .is_err()
    {
        return empty(StatusCode::SERVICE_UNAVAILABLE);
    }
    let mut response = empty(StatusCode::SWITCHING_PROTOCOLS);
    response.headers_mut().insert(
        UPGRADE,
        hyper::header::HeaderValue::from_static("websocket"),
    );
    response.headers_mut().insert(
        CONNECTION,
        hyper::header::HeaderValue::from_static("Upgrade"),
    );
    response.headers_mut().insert(SEC_WEBSOCKET_ACCEPT, accept);
    response
}

fn empty(status: StatusCode) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::new()));
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(CONNECTION, hyper::header::HeaderValue::from_static("close"));
    response
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
