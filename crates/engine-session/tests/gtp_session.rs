#![allow(clippy::unwrap_used, clippy::expect_used)]
use engine_session::{
    TransportSpec,
    gtp::{self, GtpDirection, GtpHandle, GtpSessionConfig, GtpSessionError, GtpStatus},
};
use std::{
    collections::BTreeMap,
    future::Future,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
    task::{Context, Poll, Waker},
    time::Duration,
};
use tokio::time::timeout;

async fn within<T>(future: impl Future<Output = T>) -> T {
    timeout(Duration::from_secs(5), future)
        .await
        .expect("GTP test safety deadline")
}

fn fake_binary() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .canonicalize()
                .unwrap();
            let output = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
                .args(["build", "--locked", "--manifest-path"])
                .arg(root.join("tools/fake-gtp/Cargo.toml"))
                .arg("--target-dir")
                .arg(root.join("target"))
                .arg("--message-format=json")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .find_map(|line| {
                    let value: serde_json::Value = serde_json::from_str(line).ok()?;
                    (value["reason"] == "compiler-artifact"
                        && value["target"]["name"] == "fake-gtp")
                        .then(|| value["executable"].as_str().map(PathBuf::from))
                        .flatten()
                })
                .expect("fake executable artifact")
        })
        .as_path()
}

fn start(args: &[&str], cfg: GtpSessionConfig) -> GtpHandle {
    let binary = fake_binary().to_owned();
    gtp::spawn(
        TransportSpec::LocalProcess {
            program: binary,
            args: args.iter().map(Into::into).collect(),
            cwd: None,
            env: BTreeMap::new(),
        },
        cfg,
    )
}

fn directory() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/gtp-tests")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[tokio::test]
async fn handshake_init_queue_order_ids_rejection_and_quit() {
    let engine = start(
        &["--kata"],
        GtpSessionConfig {
            init_commands: vec!["komi 7.5".into()],
            ..Default::default()
        },
    );
    let info = within(engine.ready()).await.unwrap();
    assert_eq!(info.protocol_version, "2");
    assert_eq!(info.name, "Fake GTP");
    assert_eq!(info.version, "1.0-fake");
    assert!(info.supports("kata-set-rules"));
    let mut futures = (0..20)
        .map(|id| Box::pin(engine.command(format!("{id} echo item-{id}"))))
        .collect::<Vec<_>>();
    // Poll once in order: each command enters the actor mailbox before it can reply.
    for future in &mut futures {
        assert!(matches!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Pending
        ));
    }
    for (index, future) in futures.into_iter().enumerate() {
        let response = within(future).await.unwrap();
        assert_eq!(response.id, Some(index as u32));
        assert_eq!(response.body, format!("item-{index}"));
    }
    assert!(matches!(
        within(engine.command("not_supported")).await,
        Err(GtpSessionError::Rejected { .. })
    ));
    assert_eq!(
        within(engine.command("name")).await.unwrap().body,
        "Fake GTP"
    );
    let sent = within(engine.console())
        .await
        .unwrap()
        .into_iter()
        .filter(|line| line.direction == GtpDirection::Sent)
        .map(|line| line.text)
        .collect::<Vec<_>>();
    assert_eq!(
        &sent[..5],
        [
            "protocol_version",
            "name",
            "version",
            "list_commands",
            "komi 7.5"
        ]
    );
    assert_eq!(
        &sent[5..25],
        &(0..20)
            .map(|id| format!("{id} echo item-{id}"))
            .collect::<Vec<_>>()
    );
    within(engine.command("quit")).await.unwrap();
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn init_failure_and_protocol_version_fail_startup() {
    let engine = start(
        &["--fail-command", "komi"],
        GtpSessionConfig {
            init_commands: vec!["komi 7.5".into()],
            ..Default::default()
        },
    );
    assert!(
        matches!(within(engine.ready()).await, Err(GtpSessionError::Startup { command, .. }) if command == "komi 7.5")
    );
    within(engine.shutdown()).await.unwrap();
    let engine = start(&["--protocol-version", "1"], GtpSessionConfig::default());
    assert!(matches!(
        within(engine.ready()).await,
        Err(GtpSessionError::Startup { .. })
    ));
    within(engine.shutdown()).await.unwrap();
    let engine = start(&["--fail-startup"], GtpSessionConfig::default());
    assert!(matches!(
        within(engine.ready()).await,
        Err(GtpSessionError::Process(_))
    ));
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn bounded_queue_timeout_fails_all_and_does_not_restart() {
    let directory = directory();
    let launches = directory.join("launches");
    let engine = start(
        &[
            "--hang-command",
            "genmove",
            "--launch-log",
            launches.to_str().unwrap(),
        ],
        GtpSessionConfig {
            queue_capacity: 2,
            response_timeout: Duration::from_secs(60),
            ..Default::default()
        },
    );
    within(engine.ready()).await.unwrap();
    let mut lines = engine.subscribe();
    let command_engine = engine.clone();
    let active = tokio::spawn(async move { command_engine.command("genmove b").await });
    within(async { while lines.recv().await.unwrap().text != "genmove b" {} }).await;
    let mut pending = Box::pin(engine.command("name"));
    let mut pending2 = Box::pin(engine.command("version"));
    assert!(
        pending
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    assert!(
        pending2
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    assert_eq!(
        engine.command("echo overflow").await,
        Err(GtpSessionError::Capacity)
    );
    let sent = within(engine.console())
        .await
        .unwrap()
        .into_iter()
        .filter(|line| line.direction == GtpDirection::Sent)
        .map(|line| line.text)
        .collect::<Vec<_>>();
    assert_eq!(
        sent,
        [
            "protocol_version",
            "name",
            "version",
            "list_commands",
            "genmove b"
        ]
    );
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(61)).await;
    tokio::time::resume();
    assert!(
        matches!(within(active).await.unwrap(), Err(GtpSessionError::Timeout { command }) if command == "genmove b")
    );
    assert!(matches!(
        within(pending).await,
        Err(GtpSessionError::Timeout { .. })
    ));
    assert!(matches!(
        within(pending2).await,
        Err(GtpSessionError::Timeout { .. })
    ));
    assert_eq!(
        std::fs::read_to_string(launches).unwrap().lines().count(),
        1
    );
    within(engine.shutdown()).await.unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn handshake_timeout_and_mid_game_exit_are_typed() {
    let engine = start(
        &["--hang-command", "name"],
        GtpSessionConfig {
            startup_timeout: Duration::from_secs(60),
            ..Default::default()
        },
    );
    let mut lines = engine.subscribe();
    within(async { while lines.recv().await.unwrap().text != "name" {} }).await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(61)).await;
    tokio::time::resume();
    assert!(
        matches!(within(engine.ready()).await, Err(GtpSessionError::Timeout { command }) if command == "name")
    );
    within(engine.shutdown()).await.unwrap();
    let engine = start(&["--exit-command", "genmove"], GtpSessionConfig::default());
    within(engine.ready()).await.unwrap();
    assert!(matches!(
        within(engine.command("genmove b")).await,
        Err(GtpSessionError::Process(_))
    ));
    assert!(matches!(*engine.status().borrow(), GtpStatus::Failed(_)));
    assert!(
        within(engine.console())
            .await
            .unwrap()
            .iter()
            .any(|line| line.direction == GtpDirection::Stderr
                && line.text.contains("scripted exit"))
    );
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn console_records_pushes_and_retains_exactly_latest_2000_lines() {
    let engine = start(&[], GtpSessionConfig::default());
    within(engine.ready()).await.unwrap();
    let mut lines = engine.subscribe();
    let peer = engine.clone();
    let spam = tokio::spawn(async move { peer.command("spam 2300").await });
    within(async {
        loop {
            match lines.recv().await {
                Ok(line)
                    if line.direction == GtpDirection::Stderr && line.text == "fake spam 2299" =>
                {
                    break;
                }
                Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(error) => panic!("{error}"),
            }
        }
    })
    .await;
    within(spam).await.unwrap().unwrap();
    within(engine.command("echo end")).await.unwrap();
    let history = within(engine.console()).await.unwrap();
    assert_eq!(history.len(), 2000);
    assert!(
        history
            .windows(2)
            .all(|pair| pair[0].sequence + 1 == pair[1].sequence)
    );
    assert!(
        history
            .iter()
            .any(|line| line.direction == GtpDirection::Sent && line.text == "echo end")
    );
    assert!(
        history
            .iter()
            .any(|line| line.direction == GtpDirection::Received && line.text == "= end")
    );
    assert!(
        history
            .iter()
            .any(|line| line.direction == GtpDirection::Stderr)
    );
    within(engine.clone().shutdown()).await.unwrap();
    assert_eq!(within(engine.console()).await.unwrap(), history);
}

#[tokio::test]
async fn malformed_reply_is_fatal_and_drop_reaps_process() {
    let engine = start(
        &["--malformed-command", "genmove"],
        GtpSessionConfig::default(),
    );
    within(engine.ready()).await.unwrap();
    assert!(matches!(
        within(engine.command("genmove b")).await,
        Err(GtpSessionError::Protocol(_))
    ));
    within(engine.shutdown()).await.unwrap();
    let directory = directory();
    let exits = directory.join("exits");
    let engine = start(
        &["--exit-log", exits.to_str().unwrap()],
        GtpSessionConfig::default(),
    );
    within(engine.ready()).await.unwrap();
    let mut status = engine.status();
    drop(engine);
    within(async {
        while *status.borrow_and_update() != GtpStatus::Stopped {
            status.changed().await.unwrap();
        }
    })
    .await;
    assert_eq!(std::fs::read_to_string(exits).unwrap().lines().count(), 1);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn synchronous_enqueue_preserves_fifo_without_awaiting_responses() {
    let engine = start(&[], GtpSessionConfig::default());
    within(engine.ready()).await.unwrap();
    let first = engine.enqueue_command("echo first").unwrap();
    let second = engine.enqueue_command("echo second").unwrap();
    // The caller can handle other work and await replies in either order.
    assert_eq!(within(second.recv()).await.unwrap().body, "second");
    assert_eq!(within(first.recv()).await.unwrap().body, "first");
    let sent = within(engine.console())
        .await
        .unwrap()
        .into_iter()
        .filter(|line| line.direction == GtpDirection::Sent)
        .map(|line| line.text)
        .collect::<Vec<_>>();
    assert_eq!(&sent[sent.len() - 2..], ["echo first", "echo second"]);
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn per_command_deadline_can_exceed_default_and_zero_is_rejected() {
    let engine = start(
        &["--hang-command", "genmove"],
        GtpSessionConfig {
            response_timeout: Duration::from_secs(30),
            ..Default::default()
        },
    );
    within(engine.ready()).await.unwrap();
    assert!(matches!(
        engine.command_with_timeout("name", Duration::ZERO).await,
        Err(GtpSessionError::InvalidConfig(_))
    ));
    let mut lines = engine.subscribe();
    let peer = engine.clone();
    let active = tokio::spawn(async move {
        peer.command_with_timeout("genmove b", Duration::from_secs(90))
            .await
    });
    within(async { while lines.recv().await.unwrap().text != "genmove b" {} }).await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(31)).await;
    tokio::time::resume();
    assert!(!active.is_finished());
    assert!(matches!(*engine.status().borrow(), GtpStatus::Ready(_)));
    assert!(!within(engine.console()).await.unwrap().is_empty());
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(60)).await;
    tokio::time::resume();
    assert!(
        matches!(within(active).await.unwrap(), Err(GtpSessionError::Timeout { command }) if command == "genmove b")
    );
    within(engine.shutdown()).await.unwrap();
}

#[tokio::test]
async fn deadline_override_does_not_change_following_commands() {
    let engine = start(
        &["--hang-command", "genmove"],
        GtpSessionConfig {
            response_timeout: Duration::from_secs(30),
            ..Default::default()
        },
    );
    within(engine.ready()).await.unwrap();
    assert_eq!(
        within(engine.command_with_timeout("echo timed", Duration::from_secs(90)))
            .await
            .unwrap()
            .body,
        "timed"
    );
    let mut lines = engine.subscribe();
    let peer = engine.clone();
    let active = tokio::spawn(async move { peer.command("genmove b").await });
    within(async { while lines.recv().await.unwrap().text != "genmove b" {} }).await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(31)).await;
    tokio::time::resume();
    assert!(matches!(
        within(active).await.unwrap(),
        Err(GtpSessionError::Timeout { .. })
    ));
    within(engine.shutdown()).await.unwrap();
}
