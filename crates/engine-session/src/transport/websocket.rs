use super::{CHANNEL_CAPACITY, Exit, Output, Transport};
use crate::{RemoteTransportError, SessionConfig, remote::CONNECT_TIMEOUT};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::instrument::WithSubscriber;

pub(super) async fn start(
    url: &str,
    cfg: &SessionConfig,
) -> Result<Transport, RemoteTransportError> {
    let parsed = url::Url::parse(url).map_err(|_| RemoteTransportError::InvalidConfiguration)?;
    if !matches!(parsed.scheme(), "ws" | "wss") || parsed.host_str().is_none() {
        return Err(RemoteTransportError::InvalidConfiguration);
    }
    let mut secrets = vec![url.to_owned(), parsed.as_str().to_owned()];
    secrets.extend(parsed.query_pairs().map(|(_, value)| value.into_owned()));
    secrets.extend(
        parsed
            .path_segments()
            .into_iter()
            .flatten()
            .map(str::to_owned),
    );
    secrets.extend(
        [parsed.username(), parsed.password().unwrap_or_default()]
            .into_iter()
            .map(str::to_owned),
    );
    // The TLS backend has a fixed WebPKI root store and verifies certificates.
    // Never forward tungstenite's error text (it can contain the token URL).
    let (socket, _) = tokio::time::timeout(
        CONNECT_TIMEOUT,
        connect_async(parsed.as_str())
            .with_subscriber(tracing::subscriber::NoSubscriber::default()),
    )
    .await
    .map_err(|_| RemoteTransportError::ConnectTimeout)?
    .map_err(|_| RemoteTransportError::Connect)?;
    let (outgoing, mut writes) = mpsc::channel::<String>(CHANNEL_CAPACITY);
    let (received, incoming) = mpsc::channel(CHANNEL_CAPACITY);
    let (exit, exited) = oneshot::channel();
    let (stop, mut stopping) = oneshot::channel();
    let write_timeout = cfg.write_timeout;
    let shutdown_timeout = cfg.shutdown_timeout;
    let driver = Box::pin(async move {
        let (mut sink, mut stream) = socket.split();
        let reason = loop {
            tokio::select! {
                _ = &mut stopping => {
                    let _ = tokio::time::timeout(shutdown_timeout, sink.close()).await;
                    break "WebSocket connection stopped".to_owned();
                }
                line = writes.recv() => match line {
                    Some(line) => {
                        if !matches!(tokio::time::timeout(write_timeout, sink.send(Message::Text(line.into()))).await, Ok(Ok(()))) {
                            break "WebSocket write failed or timed out".to_owned();
                        }
                    }
                    None => break "WebSocket input closed".to_owned(),
                },
                frame = stream.next() => match frame {
                    Some(Ok(Message::Text(text))) => {
                        for line in text.lines().filter(|line| !line.trim().is_empty()) {
                            if received.send(Output::Stdout(crate::remote::redact_protocol(line.to_owned(), &secrets))).await.is_err() { break; }
                        }
                    }
                    // Flush automatically queued pong responses, including when
                    // the engine is idle and no query is being written.
                    Some(Ok(Message::Ping(_))) => {
                        if !matches!(tokio::time::timeout(write_timeout, sink.flush()).await, Ok(Ok(()))) {
                            break "WebSocket heartbeat write failed".to_owned();
                        }
                    }
                    Some(Ok(Message::Close(frame))) => break frame.map_or_else(
                        || "WebSocket peer closed".into(),
                        |frame| format!("WebSocket peer closed (code {})", u16::from(frame.code)),
                    ),
                    Some(Ok(Message::Binary(_))) => break "WebSocket sent a binary protocol frame".into(),
                    Some(Ok(_)) => {},
                    Some(Err(_)) => break "WebSocket connection failed".into(),
                    None => break "WebSocket connection ended".into(),
                }
            }
        };
        let _ = exit.send(Exit { status: Ok(std::process::ExitStatus::default()), stderr_tail: Vec::new(), reason: Some(reason), fatal: false });
    }.with_subscriber(tracing::subscriber::NoSubscriber::default()));
    Ok(Transport {
        outgoing,
        incoming,
        exited,
        stop: Some(stop),
        driver,
    })
}
