#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{collections::BTreeMap, future::Future, path::Path, time::Duration};

use app_core::{AppConfig, AppCore, AppEvent, DocumentView, EngineProfile, EngineStatus, Settings};
use serde_json::{Value, json};
use tauri::{
    WebviewWindow,
    ipc::{CallbackFn, InvokeBody, InvokeError, InvokeResponse},
    test::{INVOKE_KEY, MockRuntime, mock_builder},
    webview::InvokeRequest,
};
use tempfile::TempDir;
use tokio::sync::{mpsc, oneshot};

mod board_sync;
mod engine_auto_setup;
mod problem_moves;
mod review;
mod strength;
mod tsumego;
mod variation;

const SAFETY: Duration = Duration::from_secs(5);

mod fox;

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(SAFETY, future)
        .await
        .expect("IPC operation exceeded its safety deadline")
}

struct Message {
    callback: u32,
    index: usize,
    event: AppEvent,
}

struct Harness {
    app: tauri::App<MockRuntime>,
    main: WebviewWindow<MockRuntime>,
    core: AppCore,
    output: mpsc::UnboundedReceiver<Message>,
    indices: BTreeMap<u32, usize>,
    directory: TempDir,
}

impl Harness {
    async fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let mut config = AppConfig::default();
        config.analysis.live.max_visits = 1;
        config.analysis.live.report_interval = Duration::from_millis(10);
        // No timeout fault is under test. A slow CPU must not trigger recovery.
        config.session.write_timeout = Duration::from_secs(60);
        config.session.shutdown_timeout = Duration::from_secs(60);
        let (core, subscription) = within(app_core::spawn_with_config(directory.path(), config))
            .await
            .unwrap();
        let (send, output) = mpsc::unbounded_channel();
        let resources = directory.path().join("resources/katago");
        std::fs::create_dir_all(&resources).unwrap();
        let default_config_source = resources.join("analysis.cfg");
        std::fs::write(&default_config_source, "reportAnalysisWinratesAs = BLACK\n").unwrap();
        let app = crate::commands(mock_builder())
            .manage(crate::DesktopState::new(core.clone(), subscription))
            .manage(crate::board_sync::BoardSyncEnvironment::from_resource_dir(
                &directory.path().join("resources"),
            ))
            .manage(crate::engine_setup::SetupEnvironment {
                roots: app_core::engine_setup::SearchRoots {
                    application_dirs: vec![directory.path().to_owned()],
                    ..Default::default()
                },
                default_config_source,
            })
            .channel_interceptor(move |_webview, callback, index, body| {
                let _ = send.send(Message {
                    callback: callback.0,
                    index,
                    event: body.clone().deserialize().unwrap(),
                });
                true
            })
            // The real context is essential: mock_context omits the app ACL.
            .build(crate::context())
            .unwrap();
        let main = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        Self {
            app,
            main,
            core,
            output,
            indices: BTreeMap::new(),
            directory,
        }
    }

    async fn invoke(&self, command: &str, arguments: Value) -> Value {
        request(&self.main, command, arguments, local_url())
            .await
            .unwrap_or_else(|error| panic!("{command}: {error}"))
    }

    async fn subscribe(&self, callback: u32) -> u32 {
        serde_json::from_value(
            self.invoke(
                "subscribe",
                json!({ "channel": format!("__CHANNEL__:{callback}") }),
            )
            .await,
        )
        .unwrap()
    }

    async fn next(&mut self) -> Message {
        let message = within(self.output.recv()).await.expect("channel closed");
        let index = self.indices.entry(message.callback).or_default();
        assert_eq!(message.index, *index, "channel message order changed");
        *index += 1;
        message
    }

    async fn document(&mut self, callback: u32, revision: u64) -> DocumentView {
        within(async {
            loop {
                let message = self.next().await;
                assert_eq!(message.callback, callback);
                match message.event {
                    AppEvent::DocumentChanged {
                        revision: actual,
                        view,
                    } => {
                        if actual == revision {
                            return view;
                        }
                        assert!(actual < revision, "unexpected future document");
                    }
                    AppEvent::AnalysisUpdated(view) => {
                        assert!(view.revision < revision, "analysis overtook its document");
                    }
                    AppEvent::EngineStatusChanged(_)
                    | AppEvent::WholeGameProgress(_)
                    | AppEvent::BoardSyncStatusChanged(_) => {}
                }
            }
        })
        .await
    }

    async fn finish(self) {
        within(self.core.shutdown()).await.unwrap();
    }
}

fn local_url() -> &'static str {
    if cfg!(windows) {
        "http://tauri.localhost"
    } else {
        "tauri://localhost"
    }
}

async fn request(
    window: &WebviewWindow<MockRuntime>,
    command: &str,
    arguments: Value,
    url: &str,
) -> Result<Value, Value> {
    let (send, receive) = oneshot::channel();
    window.as_ref().clone().on_message(
        InvokeRequest {
            cmd: command.to_owned(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: url.parse().unwrap(),
            body: InvokeBody::Json(arguments),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_owned(),
        },
        Box::new(move |_window, _command, response, _callback, _error| {
            let _ = send.send(response);
        }),
    );
    match within(receive).await.unwrap() {
        InvokeResponse::Ok(body) => Ok(body.deserialize().unwrap()),
        InvokeResponse::Err(InvokeError(error)) => Err(error),
    }
}

fn profile(directory: &Path) -> EngineProfile {
    let program = fake_katago::binary_path().unwrap();
    let model = directory.join("fake model.bin");
    let config = directory.join("fake configuration.cfg");
    std::fs::write(&model, b"fake model").unwrap();
    std::fs::write(&config, b"numAnalysisThreads = 1\n").unwrap();
    EngineProfile {
        program,
        model,
        config,
        extra_args: Vec::new(),
        canonical_model_name: None,
    }
}

#[tokio::test]
async fn open_command_streams_document_before_analysis_on_one_channel() {
    let mut harness = Harness::new().await;
    // Building the fake binary is not part of an IPC/engine response deadline.
    let profile = profile(harness.directory.path());
    let subscription = harness.subscribe(41).await;
    harness.document(41, 0).await;
    harness
        .invoke("configure_engine", json!({ "profile": profile }))
        .await;
    let mut ready = false;
    within(async {
        loop {
            let message = harness.next().await;
            assert_eq!(message.callback, 41);
            match message.event {
                AppEvent::EngineStatusChanged(EngineStatus::Ready { version }) => {
                    assert_eq!(version, "1.16.5-fake");
                    ready = true;
                }
                AppEvent::EngineStatusChanged(EngineStatus::Failed {
                    reason,
                    stderr_tail,
                }) => {
                    panic!("fake engine failed: {reason}: {stderr_tail:?}");
                }
                AppEvent::AnalysisUpdated(view) if !view.is_during_search => {
                    assert_eq!(view.revision, 0);
                    break;
                }
                _ => {}
            }
        }
    })
    .await;
    assert!(ready, "Ready did not arrive on the shared Channel");
    let path = harness.directory.path().join("opened game.sgf");
    std::fs::write(&path, "(;SZ[9]C[opened through IPC];B[aa])").unwrap();
    assert_eq!(
        harness.invoke("open_path", json!({ "path": path })).await,
        1
    );
    let document = harness.document(41, 1).await;
    assert_eq!(document.comment.as_deref(), Some("opened through IPC"));
    assert_eq!(document.board.unwrap().width, 9);
    within(async {
        loop {
            let message = harness.next().await;
            assert_eq!(message.callback, 41);
            match message.event {
                AppEvent::AnalysisUpdated(view) => {
                    assert_eq!((view.revision, view.node_id), (1, document.tree.current));
                    assert_eq!(view.visits, 1);
                    assert_eq!(view.ownership.unwrap().len(), 81);
                    break;
                }
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("fake engine failed: {reason}");
                }
                _ => {}
            }
        }
    })
    .await;
    harness
        .invoke("set_pondering", json!({ "enabled": false }))
        .await;
    harness
        .invoke("unsubscribe", json!({ "id": subscription }))
        .await;
    let replacement = harness.subscribe(43).await;
    // Reconnecting must restore the document but cannot replay an analysis
    // invalidated by pausing. The unsubscribe acknowledgment is our barrier:
    // every replay send precedes it, and no subsequent send can use this id.
    harness.document(43, 1).await;
    harness
        .invoke("unsubscribe", json!({ "id": replacement }))
        .await;
    while let Ok(message) = harness.output.try_recv() {
        assert_eq!(message.callback, 43);
        let index = harness.indices.entry(message.callback).or_default();
        assert_eq!(message.index, *index);
        *index += 1;
        assert!(!matches!(message.event, AppEvent::AnalysisUpdated(_)));
    }
    harness.finish().await;
}

#[tokio::test]
async fn commands_forward_edits_navigation_storage_settings_and_errors() {
    let mut harness = Harness::new().await;
    harness.subscribe(42).await;
    harness.document(42, 0).await;
    assert_eq!(
        harness
            .invoke("open_bytes", json!({ "bytes": b"(;SZ[9];B[aa];W[bb])" }))
            .await,
        1
    );
    let initial = harness.document(42, 1).await;
    assert_eq!(initial.tree.main_line.len(), 3);
    assert_eq!(harness.invoke("step", json!({ "count": 1 })).await, 2);
    let first = harness.document(42, 2).await;
    assert_eq!(first.board.unwrap().move_number, 1);
    assert_eq!(
        harness
            .invoke("go_to", json!({ "nodeId": initial.tree.root }))
            .await,
        3
    );
    harness.document(42, 3).await;
    assert_eq!(
        harness
            .invoke(
                "play",
                json!({ "mv": {
            "type": "Play", "data": { "color": "Black", "point": { "x": 2, "y": 2 } }
        } })
            )
            .await,
        4
    );
    let played = harness.document(42, 4).await;
    assert_eq!(
        played.board.as_ref().unwrap().stones[20],
        Some(app_core::ColorView::Black)
    );
    assert_eq!(harness.invoke("undo", json!({})).await, 5);
    assert_eq!(
        harness.document(42, 5).await.tree.current,
        initial.tree.root
    );
    assert_eq!(harness.invoke("redo", json!({})).await, 6);
    let redone = harness.document(42, 6).await;
    assert!(redone.tree.structure_revision > played.tree.structure_revision);
    let mut expected = played;
    expected.tree.structure_revision = redone.tree.structure_revision;
    assert_eq!(redone, expected);
    let path = harness.directory.path().join("saved game.sgf");
    harness.invoke("save", json!({ "path": path })).await;
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("B[cc]"));
    harness
        .invoke("set_pondering", json!({ "enabled": false }))
        .await;
    harness
        .invoke("configure_engine", json!({ "profile": null }))
        .await;
    let settings: Settings =
        serde_json::from_value(harness.invoke("settings", json!({})).await).unwrap();
    assert!(!settings.pondering);
    assert_eq!(settings.engine, None);
    let persisted: Settings = serde_json::from_slice(
        &std::fs::read(harness.directory.path().join("settings.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(settings, persisted);
    for (command, arguments) in [
        ("open_bytes", json!({ "bytes": b"not SGF" })),
        ("go_to", json!({ "nodeId": u32::MAX })),
        (
            "play",
            json!({ "mv": { "type": "Pass", "data": { "color": "Black" } } }),
        ),
    ] {
        let error = request(&harness.main, command, arguments, local_url())
            .await
            .unwrap_err();
        assert_eq!(error["type"], "App");
        assert!(
            error["data"]
                .as_str()
                .is_some_and(|message| !message.is_empty())
        );
    }
    assert_eq!(harness.invoke("step", json!({ "count": 0 })).await, 6);
    harness.finish().await;
}

#[tokio::test]
async fn replacement_subscription_survives_an_old_unsubscribe_and_can_reconnect() {
    let mut harness = Harness::new().await;
    let first = harness.subscribe(51).await;
    harness.document(51, 0).await;
    let second = harness.subscribe(52).await;
    assert_ne!(first, second);
    // Already delivered messages can remain in the test consumer's queue.
    // The replacement's initial document provides the handoff boundary.
    within(async {
        loop {
            let message = harness.next().await;
            if message.callback == 52
                && matches!(message.event, AppEvent::DocumentChanged { revision: 0, .. })
            {
                break;
            }
        }
    })
    .await;
    harness.invoke("unsubscribe", json!({ "id": first })).await;
    assert_eq!(
        harness
            .invoke(
                "play",
                json!({ "mv": { "type": "Pass", "data": { "color": "Black" } } })
            )
            .await,
        1
    );
    harness.document(52, 1).await;
    harness.invoke("unsubscribe", json!({ "id": second })).await;
    // Drain messages already sent before unsubscribe acknowledged completion.
    while let Ok(message) = harness.output.try_recv() {
        let index = harness.indices.entry(message.callback).or_default();
        assert_eq!(message.index, *index);
        *index += 1;
    }
    assert_eq!(
        harness
            .invoke(
                "play",
                json!({ "mv": { "type": "Pass", "data": { "color": "White" } } })
            )
            .await,
        2
    );
    let third = harness.subscribe(53).await;
    assert_ne!(second, third);
    let document = harness.document(53, 2).await;
    assert_eq!(document.board.unwrap().move_number, 2);
    harness.invoke("unsubscribe", json!({ "id": third })).await;
    harness.finish().await;
}

#[tokio::test]
async fn capability_denies_other_windows_remote_origins_and_unneeded_plugins() {
    let harness = Harness::new().await;
    assert!(harness.invoke("settings", json!({})).await.is_object());
    let other = tauri::WebviewWindowBuilder::new(&harness.app, "other", Default::default())
        .build()
        .unwrap();
    for (window, command, url) in [
        (&other, "settings", local_url()),
        (&harness.main, "settings", "https://example.com/"),
        (&other, "detect_engines", local_url()),
        (&harness.main, "detect_engines", "https://example.com/"),
        (&other, "auto_configure_engine", local_url()),
        (
            &harness.main,
            "auto_configure_engine",
            "https://example.com/",
        ),
        (&other, "variation_tree", local_url()),
        (&harness.main, "variation_tree", "https://example.com/"),
        (&other, "preview_variation", local_url()),
        (&harness.main, "preview_variation", "https://example.com/"),
        (&other, "try_configure_engine", local_url()),
        (
            &harness.main,
            "try_configure_engine",
            "https://example.com/",
        ),
        (&harness.main, "plugin:dialog|message", local_url()),
        (&harness.main, "plugin:shell|execute", local_url()),
        (&harness.main, "plugin:fs|read_file", local_url()),
    ] {
        let error = request(window, command, json!({}), url).await.unwrap_err();
        assert!(
            error
                .as_str()
                .is_some_and(|message| message.contains("not allowed")),
            "{command}: {error}"
        );
    }
    harness.finish().await;
}

#[tokio::test]
async fn setup_commands_detect_injected_roots_and_only_persist_ready_profiles() {
    let mut harness = Harness::new().await;
    let original = profile(harness.directory.path());
    let installation = harness.directory.path().join("katago");
    std::fs::create_dir_all(&installation).unwrap();
    let model = installation.join("kata1-b18c384nbt-s999-d999.bin.gz");
    std::fs::write(&model, b"fake model").unwrap();
    let discovery = harness.invoke("detect_engines", json!({})).await;
    assert_eq!(discovery["models"], json!([model]));
    assert_eq!(discovery["programs"], json!([]));
    let setup = app_core::EngineSetupProfile {
        program: original.program,
        model,
        config: None,
        extra_args: vec!["-override-config=numSearchThreads=1".into()],
    };
    harness.subscribe(61).await;
    harness.document(61, 0).await;
    harness
        .invoke("try_configure_engine", json!({ "profile": setup }))
        .await;
    let settings: Settings =
        serde_json::from_value(harness.invoke("settings", json!({})).await).unwrap();
    let saved = settings.engine.as_ref().unwrap();
    assert_eq!(saved.model, setup.model);
    assert_eq!(saved.config, harness.directory.path().join("analysis.cfg"));
    assert_eq!(
        std::fs::read_to_string(&saved.config).unwrap(),
        "reportAnalysisWinratesAs = BLACK\n"
    );
    within(async {
        loop {
            match harness.next().await.event {
                AppEvent::EngineStatusChanged(EngineStatus::Ready { version }) => {
                    assert_eq!(version, "1.16.5-fake");
                    break;
                }
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("{reason}")
                }
                _ => {}
            }
        }
    })
    .await;
    let before = std::fs::read(harness.directory.path().join("settings.json")).unwrap();
    let mut invalid = setup;
    invalid.model = harness.directory.path().join("missing model.bin.gz");
    let error = request(
        &harness.main,
        "try_configure_engine",
        json!({ "profile": invalid }),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(error["type"], "App");
    assert!(
        error["data"]
            .as_str()
            .unwrap()
            .contains("missing model.bin.gz")
    );
    assert_eq!(
        std::fs::read(harness.directory.path().join("settings.json")).unwrap(),
        before
    );
    assert_eq!(harness.core.settings().await.unwrap(), settings);
    // A new revision must still receive live analysis from the retained engine.
    harness
        .invoke("open_bytes", json!({ "bytes": b"(;SZ[9])" }))
        .await;
    harness.document(61, 1).await;
    within(async {
        loop {
            match harness.next().await.event {
                AppEvent::AnalysisUpdated(view) if view.revision == 1 => break,
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("{reason}")
                }
                _ => {}
            }
        }
    })
    .await;
    harness.finish().await;
}
