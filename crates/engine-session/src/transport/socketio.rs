//! Minimal Engine.IO v4 / Socket.IO v5, websocket transport only.
//! The session actor polls this driver; no detached connection owner exists.
use super::{CHANNEL_CAPACITY, Exit, Output, Transport};
use crate::{RemoteTransportError as Error, SessionConfig, ZhiziConnection};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use std::{collections::VecDeque, time::Duration};
use tokio::{
    net::TcpStream,
    sync::{mpsc, oneshot},
    time::{Instant, timeout, timeout_at},
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async_tls_with_config,
    tungstenite::{Message, protocol::WebSocketConfig},
};
use tracing::instrument::WithSubscriber;

mod packet;
use packet::{Packet, Parser};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const READY_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_LINE: usize = 1024 * 1024;

fn endpoint(connection: &ZhiziConnection) -> Result<url::Url, Error> {
    let mut url = url::Url::parse(&connection.url).map_err(|_| Error::InvalidConfiguration)?;
    if !matches!(url.scheme(), "https" | "wss")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || connection.token.is_empty()
    {
        return Err(Error::InvalidConfiguration);
    }
    url.set_scheme("wss")
        .map_err(|_| Error::InvalidConfiguration)?;
    url.set_path("/socket.io.v4/");
    url.set_query(None);
    url.set_fragment(None);
    url.query_pairs_mut()
        .append_pair("EIO", "4")
        .append_pair("transport", "websocket")
        .append_pair("zz-socketio-token", &connection.token);
    Ok(url)
}

pub(super) async fn start(
    connection: ZhiziConnection,
    cfg: &SessionConfig,
) -> Result<Transport, Error> {
    start_inner(connection, cfg)
        .with_subscriber(tracing::subscriber::NoSubscriber::default())
        .await
}

async fn start_inner(connection: ZhiziConnection, cfg: &SessionConfig) -> Result<Transport, Error> {
    let url = endpoint(&connection)?;
    #[cfg(test)]
    let connector = connection.tls.clone();
    #[cfg(not(test))]
    let connector = None;
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_LINE))
        .max_frame_size(Some(MAX_LINE));
    let (mut socket, _) = timeout(
        CONNECT_TIMEOUT,
        connect_async_tls_with_config(url.as_str(), Some(config), false, connector),
    )
    .await
    .map_err(|_| Error::ConnectTimeout)?
    .map_err(|error| match error {
        tokio_tungstenite::tungstenite::Error::Http(response) => {
            Error::zhizi_fatal(Some(response.status().as_u16()), "").unwrap_or(Error::Connect)
        }
        _ => Error::Connect,
    })?;
    let mut parser = Parser::default();
    let mut buffered = Vec::new();
    let mut connected = false;
    let ready_deadline = Instant::now() + READY_TIMEOUT;
    loop {
        let packet = timeout_at(
            ready_deadline,
            next_packet(&mut socket, &mut parser, cfg.write_timeout),
        )
        .await
        .map_err(|_| Error::ReadyTimeout)??;
        match packet {
            Packet::Open => send(&mut socket, "40".into(), cfg.write_timeout).await?,
            Packet::Connected => connected = true,
            Packet::Event { name, .. } if name == "ready" && connected => break,
            Packet::Event { name, payload } if matches!(name.as_str(), "stdout" | "stderr") => {
                // Bound diagnostics received before ready; never print remote text here.
                if buffered.len() >= CHANNEL_CAPACITY {
                    return Err(Error::Protocol);
                }
                buffered.push((name, payload));
            }
            Packet::Failure(error) => return Err(error),
            _ => {}
        }
    }
    let (outgoing, mut writes) = mpsc::channel::<String>(CHANNEL_CAPACITY);
    let (received, incoming) = mpsc::channel(CHANNEL_CAPACITY);
    let (exit, exited) = oneshot::channel();
    let (stop, mut stopping) = oneshot::channel();
    let write_timeout = cfg.write_timeout;
    let shutdown_timeout = cfg.shutdown_timeout;
    let mut lines = Lines::new(vec![connection.url, connection.token, url.to_string()]);
    let driver = Box::pin(async move {
        let mut error = None;
        for (name, payload) in buffered {
            if let Err(failure) = lines.deliver(&name, payload, &received).await { error = Some(failure); break; }
        }
        let reason = loop {
            if let Some(error) = error.take() { break error; }
            tokio::select! {
                _ = &mut stopping => {
                    graceful_close(&mut socket, shutdown_timeout).await;
                    break Error::Connect;
                }
                line = writes.recv() => match line {
                    Some(mut line) => {
                        if !line.ends_with('\n') { line.push('\n'); }
                        if let Err(error) = send(&mut socket, format!("42{}", json!(["stdin",line])), write_timeout).await { break error; }
                    }
                    None => { graceful_close(&mut socket, shutdown_timeout).await; break Error::Connect; }
                },
                packet = next_packet(&mut socket, &mut parser, write_timeout) => match packet {
                    Ok(Packet::Event { name, payload }) if matches!(name.as_str(), "stdout" | "stderr") => {
                        if let Err(error) = lines.deliver(&name, payload, &received).await { break error; }
                    }
                    Ok(Packet::Failure(error)) | Err(error) => break error,
                    Ok(_) => {},
                }
            }
        };
        let fatal = matches!(reason, Error::ZhiziAccessDenied | Error::ZhiziBalance | Error::ZhiziVip);
        let _ = exit.send(Exit { status: Ok(std::process::ExitStatus::default()), stderr_tail: lines.tail.into(), reason: Some(reason.to_string()), fatal });
    }.with_subscriber(tracing::subscriber::NoSubscriber::default()));
    Ok(Transport {
        outgoing,
        incoming,
        exited,
        stop: Some(stop),
        driver,
    })
}

async fn send(socket: &mut Socket, text: String, duration: Duration) -> Result<(), Error> {
    timeout(duration, socket.send(Message::Text(text.into())))
        .await
        .map_err(|_| Error::ConnectTimeout)?
        .map_err(|_| Error::Connect)
}

async fn graceful_close(socket: &mut Socket, duration: Duration) {
    let _ = timeout(duration, async {
        socket
            .send(Message::Text(
                format!("42{}", json!(["stdin", "quit\n"])).into(),
            ))
            .await?;
        socket.send(Message::Text("41".into())).await?;
        socket.close(None).await
    })
    .await;
}

async fn next_packet(
    socket: &mut Socket,
    parser: &mut Parser,
    write_timeout: Duration,
) -> Result<Packet, Error> {
    loop {
        let frame = timeout_at(parser.deadline(), socket.next())
            .await
            .map_err(|_| Error::ConnectTimeout)?;
        match frame {
            Some(Ok(Message::Text(text))) => {
                if let Some(ping) = text.strip_prefix('2') {
                    parser.heartbeat();
                    send(socket, format!("3{ping}"), write_timeout).await?;
                } else if let Some(packet) = parser.text(&text)? {
                    return Ok(packet);
                }
            }
            Some(Ok(Message::Binary(bytes))) => return parser.binary(&bytes),
            Some(Ok(Message::Ping(_))) => {
                timeout(write_timeout, socket.flush())
                    .await
                    .map_err(|_| Error::ConnectTimeout)?
                    .map_err(|_| Error::Connect)?;
            }
            Some(Ok(Message::Pong(_))) => {}
            _ => return Err(Error::Connect),
        }
    }
}

struct Lines {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    secrets: Vec<String>,
    tail: VecDeque<String>,
}
impl Lines {
    fn new(secrets: Vec<String>) -> Self {
        Self {
            stdout: Vec::new(),
            stderr: Vec::new(),
            secrets,
            tail: VecDeque::new(),
        }
    }
    async fn deliver(
        &mut self,
        name: &str,
        bytes: Vec<u8>,
        received: &mpsc::Sender<Output>,
    ) -> Result<(), Error> {
        let buffer = if name == "stderr" {
            &mut self.stderr
        } else {
            &mut self.stdout
        };
        if buffer.len().saturating_add(bytes.len()) > MAX_LINE {
            return Err(Error::Protocol);
        }
        buffer.extend(bytes);
        while let Some(end) = buffer.iter().position(|byte| *byte == b'\n') {
            let raw: Vec<_> = buffer.drain(..=end).collect();
            let text = std::str::from_utf8(&raw[..end])
                .map_err(|_| Error::Protocol)?
                .trim_end_matches('\r');
            let line = crate::remote::redact(text.to_owned(), &self.secrets);
            let output = if name == "stderr" {
                if self.tail.len() == 200 {
                    self.tail.pop_front();
                }
                self.tail.push_back(line.clone());
                Output::Stderr(line)
            } else {
                Output::Stdout(line)
            };
            received.send(output).await.map_err(|_| Error::Connect)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
