#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{TestConfig, protocol::*};
use crate::test_support::within;
use crate::{AppConfig, AppCore, AppEvent, AppSubscription, DocumentView};
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{Message, client::IntoClientRequest},
};

#[path = "lifecycle_tests.rs"]
mod lifecycle;
#[path = "push_tests.rs"]
mod push;
#[path = "trial_tests.rs"]
mod trial;

type Socket = WebSocketStream<TcpStream>;

struct Harness {
    directory: tempfile::TempDir,
    app: AppCore,
    events: AppSubscription,
    status: WebBoardStatus,
}

struct Initial {
    hello: WebHello,
    full: WebFullState,
    history: crate::GraphView,
    problems: crate::ProblemMovesView,
    trial: WebTrialState,
}

fn configuration(web: TestConfig) -> AppConfig {
    AppConfig {
        web_board: Some(web),
        ..AppConfig::default()
    }
}

impl Harness {
    async fn new() -> Self {
        Self::configured(configuration(TestConfig::default())).await
    }

    async fn configured(config: AppConfig) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let (app, events) = within(crate::spawn_with_config(directory.path(), config))
            .await
            .unwrap();
        within(app.set_auto_quick_analyze_on_load(false))
            .await
            .unwrap();
        within(app.set_autosave(false)).await.unwrap();
        within(app.set_cache_enabled(false)).await.unwrap();
        let status = within(app.start_web_board()).await.unwrap();
        assert!(status.running);
        Self {
            directory,
            app,
            events,
            status,
        }
    }

    async fn open(&mut self, bytes: &[u8]) -> DocumentView {
        let revision = within(self.app.open_bytes(bytes.to_vec())).await.unwrap();
        host_view(&mut self.events, revision).await
    }

    async fn connect(&self, reconnect: Option<&str>) -> (Socket, Initial) {
        let mut socket = self.connect_socket(reconnect).await;
        let initial = within(async {
            let (mut hello, mut full, mut history, mut problems, mut trial) =
                (None, None, None, None, None);
            while hello.is_none()
                || full.is_none()
                || history.is_none()
                || problems.is_none()
                || trial.is_none()
            {
                match receive(&mut socket).await {
                    WebServerMessage::Hello(value) => hello = Some(value),
                    WebServerMessage::FullState(value) => full = Some(value),
                    WebServerMessage::WinrateHistory(value) => history = Some(value),
                    WebServerMessage::ProblemMoves(value) => problems = Some(value),
                    WebServerMessage::TrialState(value) => trial = Some(value),
                    _ => {}
                }
            }
            Initial {
                hello: hello.unwrap(),
                full: full.unwrap(),
                history: history.unwrap(),
                problems: problems.unwrap(),
                trial: trial.unwrap(),
            }
        })
        .await;
        (socket, initial)
    }

    async fn connect_socket(&self, reconnect: Option<&str>) -> Socket {
        let entry = url::Url::parse(self.status.url.as_ref().unwrap()).unwrap();
        let port = self.status.port.unwrap();
        let mut target = format!("ws://127.0.0.1:{port}{}ws", entry.path());
        if let Some(credential) = reconnect {
            target.push_str("?reconnect=");
            target.push_str(credential);
        }
        let mut request = target.into_client_request().unwrap();
        request.headers_mut().insert(
            "Origin",
            format!("http://127.0.0.1:{port}").parse().unwrap(),
        );
        let tcp = within(TcpStream::connect(("127.0.0.1", port)))
            .await
            .unwrap();
        let (socket, response) = within(tokio_tungstenite::client_async(request, tcp))
            .await
            .unwrap();
        assert_eq!(response.status(), 101);
        socket
    }

    async fn shutdown(self) {
        within(self.app.shutdown()).await.unwrap();
    }
}

async fn send(socket: &mut Socket, message: WebClientMessage) {
    within(socket.send(Message::text(serde_json::to_string(&message).unwrap())))
        .await
        .unwrap();
}

async fn receive(socket: &mut Socket) -> WebServerMessage {
    within(async {
        loop {
            match socket
                .next()
                .await
                .expect("web socket ended")
                .expect("web socket failure")
            {
                Message::Text(text) => return serde_json::from_str(&text).unwrap(),
                Message::Ping(_) => socket.flush().await.unwrap(),
                other => panic!("unexpected websocket message: {other:?}"),
            }
        }
    })
    .await
}

async fn message_where(
    socket: &mut Socket,
    mut predicate: impl FnMut(&WebServerMessage) -> bool,
) -> WebServerMessage {
    within(async {
        loop {
            let message = receive(socket).await;
            if predicate(&message) {
                return message;
            }
        }
    })
    .await
}

async fn trial_where(
    socket: &mut Socket,
    predicate: impl Fn(&WebTrialState) -> bool,
) -> WebTrialState {
    match message_where(
        socket,
        |m| matches!(m, WebServerMessage::TrialState(t) if predicate(t)),
    )
    .await
    {
        WebServerMessage::TrialState(value) => value,
        _ => unreachable!(),
    }
}

async fn full_where(
    socket: &mut Socket,
    predicate: impl Fn(&WebFullState) -> bool,
) -> WebFullState {
    match message_where(
        socket,
        |m| matches!(m, WebServerMessage::FullState(t) if predicate(t)),
    )
    .await
    {
        WebServerMessage::FullState(value) => value,
        _ => unreachable!(),
    }
}

async fn host_view(events: &mut AppSubscription, revision: u64) -> DocumentView {
    within(async {
        loop {
            if let AppEvent::DocumentChanged {
                revision: current,
                view,
            } = events.recv().await.unwrap()
                && current == revision
            {
                return view;
            }
        }
    })
    .await
}

async fn web_status(
    events: &mut AppSubscription,
    predicate: impl Fn(&WebBoardStatus) -> bool,
) -> WebBoardStatus {
    within(async {
        loop {
            if let AppEvent::WebBoardChanged(value) = events.recv().await.unwrap()
                && predicate(&value)
            {
                return value;
            }
        }
    })
    .await
}

async fn closed(socket: &mut Socket) {
    within(async {
        loop {
            match socket.next().await {
                None | Some(Ok(Message::Close(_))) | Some(Err(_)) => return,
                _ => {}
            }
        }
    })
    .await;
}

#[tokio::test]
async fn options_default_persist_but_running_state_and_tokens_do_not() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("settings.json"), r#"{"version":1}"#).unwrap();
    let (app, _events) = within(crate::spawn_with_config(
        directory.path(),
        configuration(TestConfig::default()),
    ))
    .await
    .unwrap();
    assert!(within(app.settings()).await.unwrap().web_board.allow_trial);
    assert!(!within(app.web_board_status()).await.unwrap().running);
    within(app.set_web_board_options(WebBoardOptions { allow_trial: false }))
        .await
        .unwrap();
    let first = within(app.start_web_board()).await.unwrap();
    assert_eq!(within(app.start_web_board()).await.unwrap(), first);
    within(app.stop_web_board()).await.unwrap();
    let second = within(app.start_web_board()).await.unwrap();
    assert_ne!(
        url::Url::parse(first.url.as_ref().unwrap()).unwrap().path(),
        url::Url::parse(second.url.as_ref().unwrap())
            .unwrap()
            .path()
    );
    within(app.shutdown()).await.unwrap();
    let settings_text = std::fs::read_to_string(directory.path().join("settings.json")).unwrap();
    let value: serde_json::Value = serde_json::from_str(&settings_text).unwrap();
    assert_eq!(value["version"], 1);
    assert_eq!(value["web_board"], serde_json::json!({"allow_trial":false}));
    assert!(!settings_text.contains(first.url.as_ref().unwrap()));
    let (app, _events) = within(crate::spawn_with_config(
        directory.path(),
        configuration(TestConfig::default()),
    ))
    .await
    .unwrap();
    assert!(!within(app.settings()).await.unwrap().web_board.allow_trial);
    assert_eq!(
        within(app.web_board_status()).await.unwrap(),
        WebBoardStatus::default()
    );
    within(app.shutdown()).await.unwrap();
}
