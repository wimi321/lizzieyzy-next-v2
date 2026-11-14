#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
use app_core::commentary_client::{
    CommentaryClient, CommentaryClientError, CommentaryMessage, normalize_base_url,
};
use common::within;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

struct Reply {
    status: u16,
    headers: String,
    chunks: Vec<Vec<u8>>,
    gate: Option<oneshot::Receiver<()>>,
}
impl Reply {
    fn sse(body: &str) -> Self {
        Self {
            status: 200,
            headers: String::new(),
            chunks: vec![body.as_bytes().to_vec()],
            gate: None,
        }
    }
    fn error(status: u16, body: &str) -> Self {
        Self {
            status,
            ..Self::sse(body)
        }
    }
}
struct Server {
    base: String,
    requests: mpsc::UnboundedReceiver<String>,
    task: JoinHandle<()>,
}
impl Server {
    async fn new(replies: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let (sent, requests) = mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            for reply in replies {
                let (mut socket, _) = listener.accept().await.unwrap();
                sent.send(read_request(&mut socket).await).unwrap();
                socket.write_all(format!("HTTP/1.1 {} Test\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n{}\r\n", reply.status, reply.headers).as_bytes()).await.unwrap();
                for (index, chunk) in reply.chunks.iter().enumerate() {
                    socket
                        .write_all(format!("{:x}\r\n", chunk.len()).as_bytes())
                        .await
                        .unwrap();
                    socket.write_all(chunk).await.unwrap();
                    socket.write_all(b"\r\n").await.unwrap();
                    if index == 0 && reply.gate.is_some() {
                        break;
                    }
                }
                if let Some(gate) = reply.gate {
                    gate.await.unwrap();
                    for chunk in reply.chunks.iter().skip(1) {
                        socket
                            .write_all(format!("{:x}\r\n", chunk.len()).as_bytes())
                            .await
                            .unwrap();
                        socket.write_all(chunk).await.unwrap();
                        socket.write_all(b"\r\n").await.unwrap();
                    }
                }
                let _ = socket.write_all(b"0\r\n\r\n").await;
            }
        });
        Self {
            base,
            requests,
            task,
        }
    }
}
async fn read_request(socket: &mut TcpStream) -> String {
    let mut request = Vec::new();
    loop {
        let mut buffer = [0; 4096];
        let count = socket.read(&mut buffer).await.unwrap();
        assert!(count > 0);
        request.extend_from_slice(&buffer[..count]);
        if let Some(end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&request[..end]);
            let length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|value| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if request.len() >= end + 4 + length {
                return String::from_utf8(request).unwrap();
            }
        }
    }
}
fn messages() -> Vec<CommentaryMessage> {
    vec![
        CommentaryMessage {
            role: "system".into(),
            content: "Explain only evidence".into(),
        },
        CommentaryMessage {
            role: "user".into(),
            content: "D4".into(),
        },
    ]
}

#[test]
fn addresses_are_canonical_and_only_explicit_loopback_allows_http() {
    for (input, expected) in [
        ("https://example.test", "https://example.test/v1"),
        ("https://example.test/", "https://example.test/v1"),
        (
            "https://example.test/custom///",
            "https://example.test/custom",
        ),
        ("http://localhost:1234", "http://localhost:1234/v1"),
        ("http://127.0.0.1:1234/v1/", "http://127.0.0.1:1234/v1"),
        ("http://[::1]:1234/", "http://[::1]:1234/v1"),
    ] {
        assert_eq!(normalize_base_url(input).unwrap(), expected);
    }
    for input in [
        "http://example.test",
        "http://localhost.evil",
        "http://127.1",
        "http://127.0.0.2",
        "http://0.0.0.0",
        "ftp://localhost",
        "https://user:pass@example.test",
        "https://@example.test",
        "https://example.test/?key=x",
        "https://example.test/#x",
        "https://example.test/?",
        "http://localhost./",
    ] {
        assert_eq!(
            normalize_base_url(input),
            Err(CommentaryClientError::InvalidAddress),
            "{input}"
        );
    }
    assert_eq!(
        normalize_base_url(""),
        Err(CommentaryClientError::NotConfigured)
    );
}

#[tokio::test]
async fn streams_before_completion_and_sends_required_headers_and_messages() {
    let (release, gate) = oneshot::channel();
    let mut server = Server::new(vec![Reply { gate: Some(gate), chunks: vec![
        b"data: {\"choices\":[{\"delta\":{\"content\":\"D4 \"}}]}\n\n".to_vec(),
        "data: {\"choices\":[{\"delta\":{\"content\":\"连接\"},\"finish_reason\":\"stop\"}]}\n\n".as_bytes().to_vec(),
    ], ..Reply::sse("") }]).await;
    let client = CommentaryClient::new(&server.base).unwrap();
    let (sent, mut deltas) = mpsc::unbounded_channel();
    let request = tokio::spawn(async move {
        client
            .complete("test-key", "test-model", &messages(), |delta| {
                sent.send(delta).unwrap();
            })
            .await
    });
    assert_eq!(within(deltas.recv()).await.unwrap(), "D4 ");
    assert!(!request.is_finished());
    let raw = within(server.requests.recv()).await.unwrap();
    let lowercase = raw.to_ascii_lowercase();
    assert!(raw.starts_with("POST /v1/chat/completions "));
    assert!(lowercase.contains("authorization: bearer test-key"));
    assert!(lowercase.contains("user-agent: lizzieyzy-next-ai-commentary"));
    let body: serde_json::Value =
        serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(body["stream"], true);
    assert_eq!(body["model"], "test-model");
    assert_eq!(body["messages"], serde_json::to_value(messages()).unwrap());
    release.send(()).unwrap();
    assert_eq!(within(deltas.recv()).await.unwrap(), "连接");
    assert_eq!(within(request).await.unwrap().unwrap(), "D4 连接");
    within(server.task).await.unwrap();
}

#[tokio::test]
async fn arbitrary_utf8_chunks_crlf_multiline_data_and_done_are_supported() {
    let stream = ": keepalive\r\ndata: {\"choices\":\r\ndata: [{\"delta\":{\"content\":\"连接D4\"}}]}\r\n\r\ndata: [DONE]\r\n\r\n";
    let server = Server::new(vec![Reply {
        chunks: stream.as_bytes().chunks(1).map(Vec::from).collect(),
        ..Reply::sse("")
    }])
    .await;
    let mut deltas = Vec::new();
    assert_eq!(
        within(CommentaryClient::new(&server.base).unwrap().complete(
            "key",
            "model",
            &messages(),
            |delta| deltas.push(delta)
        ))
        .await
        .unwrap(),
        "连接D4"
    );
    assert_eq!(deltas, ["连接D4"]);
    within(server.task).await.unwrap();
}

#[tokio::test]
async fn missing_terminal_length_filter_and_failed_responses_are_interrupted() {
    for body in [
        "data: {\"choices\":[{\"delta\":{\"content\":\"D4\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"length\"}]}\n\ndata: [DONE]\n\n",
        "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"content_filter\"}]}\n\n",
        "data: [DONE]\n",
    ] {
        let server = Server::new(vec![Reply::sse(body)]).await;
        assert_eq!(
            within(CommentaryClient::new(&server.base).unwrap().complete(
                "key",
                "model",
                &messages(),
                |_| {}
            ))
            .await,
            Err(CommentaryClientError::Interrupted)
        );
        within(server.task).await.unwrap();
    }
    for body in [
        "data: {\"type\":\"response.incomplete\"}\n\n",
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"D4\"}\n\n",
    ] {
        let server = Server::new(vec![Reply::error(404, "missing"), Reply::sse(body)]).await;
        assert_eq!(
            within(CommentaryClient::new(&server.base).unwrap().complete(
                "key",
                "model",
                &messages(),
                |_| {}
            ))
            .await,
            Err(CommentaryClientError::Interrupted)
        );
        within(server.task).await.unwrap();
    }
}

#[tokio::test]
async fn only_404_and_405_fall_back_once_to_responses() {
    for status in [404, 405] {
        let mut server = Server::new(vec![Reply::error(status, "unsupported"), Reply::sse("event: response.output_text.delta\ndata: {\"delta\":\"D4\"}\n\ndata: {\"type\":\"response.completed\"}\n\n")]).await;
        let answer = within(CommentaryClient::new(&server.base).unwrap().complete(
            "key",
            "model",
            &messages(),
            |_| {},
        ))
        .await
        .unwrap();
        assert_eq!(answer, "D4");
        assert!(
            within(server.requests.recv())
                .await
                .unwrap()
                .starts_with("POST /v1/chat/completions ")
        );
        let raw = within(server.requests.recv()).await.unwrap();
        assert!(raw.starts_with("POST /v1/responses "));
        let body: serde_json::Value =
            serde_json::from_str(raw.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(body["input"], serde_json::to_value(messages()).unwrap());
        within(server.task).await.unwrap();
    }
    let server = Server::new(vec![
        Reply::error(404, "missing"),
        Reply::error(405, "missing too"),
    ])
    .await;
    assert!(matches!(
        within(CommentaryClient::new(&server.base).unwrap().complete(
            "key",
            "model",
            &messages(),
            |_| {}
        ))
        .await,
        Err(CommentaryClientError::Http { status: 405, .. })
    ));
    within(server.task).await.unwrap();
}

#[tokio::test]
async fn rate_limit_retains_retry_after_and_errors_redact_secrets_before_truncating() {
    let body = format!(
        "key=private-test-key; Authorization: BeArEr another-secret\nBearer \"quoted-secret\"\n{}",
        "中".repeat(300)
    );
    let server = Server::new(vec![Reply {
        headers: "Retry-After: 17\r\n".into(),
        ..Reply::error(429, &body)
    }])
    .await;
    let error = within(CommentaryClient::new(&server.base).unwrap().complete(
        "private-test-key",
        "model",
        &messages(),
        |_| {},
    ))
    .await
    .unwrap_err();
    let CommentaryClientError::RateLimited { retry_after, body } = &error else {
        panic!("unexpected error")
    };
    assert_eq!(retry_after.as_deref(), Some("17"));
    assert_eq!(body.chars().count(), 240);
    for rendered in [
        format!("{error:?}"),
        error.to_string(),
        serde_json::to_string(&error).unwrap(),
    ] {
        assert!(!rendered.contains("private-test-key"));
        assert!(!rendered.contains("another-secret"));
        assert!(!rendered.contains("quoted-secret"));
    }
    within(server.task).await.unwrap();
}

#[tokio::test]
async fn models_are_read_and_redirects_never_followed() {
    let mut server = Server::new(vec![
        Reply::sse(r#"{"data":[{"id":"model-a"},{"id":"model-b"},{"id":"model-a"}]}"#),
        Reply {
            headers: "Location: http://127.0.0.1:1/leak\r\n".into(),
            ..Reply::error(302, "redirect forbidden")
        },
    ])
    .await;
    let client = CommentaryClient::new(&server.base).unwrap();
    assert_eq!(
        within(client.models("key")).await.unwrap(),
        ["model-a", "model-b"]
    );
    assert!(
        within(server.requests.recv())
            .await
            .unwrap()
            .starts_with("GET /v1/models ")
    );
    assert!(matches!(
        within(client.complete("key", "model", &messages(), |_| {})).await,
        Err(CommentaryClientError::Http { status: 302, .. })
    ));
    within(server.task).await.unwrap();
}

#[derive(Clone)]
struct Capture(mpsc::UnboundedSender<String>);
impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Capture {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        struct Fields(String);
        impl tracing::field::Visit for Fields {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                self.0.push_str(&format!("{}={value:?}", field.name()));
            }
        }
        let mut fields = Fields(String::new());
        event.record(&mut fields);
        self.0.send(fields.0).unwrap();
    }
}

#[tokio::test]
async fn transport_debug_and_tracing_do_not_expose_key() {
    use tracing::instrument::WithSubscriber;
    use tracing_subscriber::prelude::*;
    let server = Server::new(vec![Reply::error(401, "Bearer secret-log-test-key")]).await;
    let (sent, mut logs) = mpsc::unbounded_channel();
    let subscriber = tracing_subscriber::registry().with(Capture(sent));
    let client = CommentaryClient::new(&server.base).unwrap();
    async {
        tracing::info!("capture active");
        let error = client
            .complete("secret-log-test-key", "model", &messages(), |_| {})
            .await
            .unwrap_err();
        tracing::warn!(?error, ?client, "expected failure");
    }
    .with_subscriber(subscriber)
    .await;
    let mut count = 0;
    while let Ok(line) = logs.try_recv() {
        count += 1;
        assert!(!line.contains("secret-log-test-key"));
    }
    assert!(count >= 2);
    within(server.task).await.unwrap();
}

#[tokio::test]
#[ignore = "explicit live acceptance only; never part of offline verification"]
async fn live_commentary_service() {
    let base = std::env::var("LIZZIE_COMMENTARY_BASE").expect("explicit base required");
    let model = std::env::var("LIZZIE_COMMENTARY_MODEL").expect("explicit model required");
    let key = std::env::var("LIZZIE_COMMENTARY_KEY").expect("explicit key required");
    let answer = CommentaryClient::new(&base)
        .unwrap()
        .complete(&key, &model, &messages(), |_| {})
        .await
        .unwrap();
    assert!(!answer.trim().is_empty());
}

#[tokio::test]
async fn timeout_is_typed_and_dropping_request_closes_pending_work() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (accepted, ready) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        accepted.send(()).unwrap();
        let mut byte = [0];
        assert_eq!(socket.read(&mut byte).await.unwrap(), 0);
    });
    let client = CommentaryClient::new(&base).unwrap();
    let request =
        tokio::spawn(async move { client.complete("key", "model", &messages(), |_| {}).await });
    within(ready).await.unwrap();
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(301)).await;
    tokio::time::resume();
    assert_eq!(
        within(request).await.unwrap(),
        Err(CommentaryClientError::Timeout)
    );
    within(server).await.unwrap();
}
