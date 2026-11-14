use engine_session::{
    AnalysisRequest, EngineHandle, EngineStatus, QueryEvent, QuerySubscription, SessionConfig,
};
use serde_json::{Value, json};
use std::{future::Future, panic::Location, path::Path, time::Duration};
use tokio::process::Child;

#[track_caller]
pub fn within<T>(future: impl Future<Output = T>) -> impl Future<Output = T> {
    let caller = Location::caller();
    async move {
        tokio::time::timeout(Duration::from_secs(5), future)
            .await
            .unwrap_or_else(|_| {
                panic!(
                    "operation exceeded safety deadline (5s) at {}:{}",
                    caller.file(),
                    caller.line()
                )
            })
    }
}

pub fn config() -> SessionConfig {
    SessionConfig {
        startup_timeout: Duration::from_secs(60),
        response_timeout: Duration::from_secs(60),
        write_timeout: Duration::from_secs(60),
        restart_backoff: Duration::from_millis(1),
        max_restart_backoff: Duration::from_millis(2),
        ..SessionConfig::default()
    }
}

pub async fn ready(engine: &EngineHandle) {
    let mut status = engine.status();
    within(async {
        loop {
            match status.borrow_and_update().clone() {
                EngineStatus::Ready { version } => {
                    assert_eq!(version, "1.16.5-fake");
                    return;
                }
                EngineStatus::Failed { reason, .. } => panic!("engine failed: {reason}"),
                _ => {}
            }
            status.changed().await.unwrap();
        }
    })
    .await;
}

pub fn request(extra: Value) -> AnalysisRequest {
    let mut value = json!({"moves":[["B","D4"]], "rules":"chinese", "boardXSize":9, "boardYSize":9, "maxVisits":10, "includeOwnership":true });
    value
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    serde_json::from_value(value).unwrap()
}

pub async fn next(subscription: &mut QuerySubscription) -> QueryEvent {
    within(subscription.recv()).await.expect("query event")
}

pub async fn complete(
    subscription: &mut QuerySubscription,
) -> Vec<katago_protocol::AnalysisResponse> {
    let mut finals = Vec::new();
    while let Some(event) = within(subscription.recv()).await {
        match event {
            QueryEvent::Final(response) => finals.push(response),
            QueryEvent::Partial(_) => {}
            event => panic!("unexpected event: {event:?}"),
        }
    }
    finals
}

pub async fn fake(binary: &Path) -> Child {
    let mut command = tokio::process::Command::new(binary);
    command
        .args([
            "analysis",
            "-config",
            "unused.cfg",
            "-model",
            "unused.bin.gz",
            "--threads",
            "2",
            "--default-perspective-to-play",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    engine_session::spawn_process(&mut command).await.unwrap()
}
