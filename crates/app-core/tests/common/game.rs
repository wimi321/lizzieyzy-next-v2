#![allow(dead_code)]

use super::within;
use app_core::{
    AiPlayer, AppConfig, AppCore, AppEvent, AppSubscription, DocumentView, EngineProfile,
    EngineStatus, GamePhase, GameSetup, GameView, KataGoPlay, Player, Settings,
};
use serde_json::Value;
use std::{path::Path, time::Duration};

pub struct Harness {
    pub directory: tempfile::TempDir,
    pub app: AppCore,
    pub events: AppSubscription,
}

pub fn config(directory: &Path) -> AppConfig {
    let mut config = AppConfig::default();
    config.session.startup_timeout = Duration::from_secs(60);
    config.session.response_timeout = Duration::from_secs(60);
    config.session.write_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(60);
    config.analysis.live.report_interval = Duration::from_millis(10);
    config.games_directory = Some(directory.join("MyGames"));
    config
}

pub async fn harness(extra: Vec<String>) -> Harness {
    let directory = tempfile::tempdir().unwrap();
    let cfg = config(directory.path());
    harness_in(directory, cfg, extra).await
}

pub async fn harness_in(
    directory: tempfile::TempDir,
    config: AppConfig,
    extra: Vec<String>,
) -> Harness {
    // Compilation is preparation, never part of a five-second behavior deadline.
    let program = fake_katago::binary_path().unwrap();
    let model = directory.path().join("model.bin");
    let engine_config = directory.path().join("analysis.cfg");
    std::fs::write(&model, "fake model").unwrap();
    std::fs::write(&engine_config, "").unwrap();
    let settings = Settings {
        pondering: false,
        auto_quick_analyze_on_load: false,
        cache_enabled: false,
        ..Settings::default()
    };
    within(settings.save(directory.path())).await.unwrap();
    let (app, mut events) = within(app_core::spawn_with_config(directory.path(), config))
        .await
        .unwrap();
    let mut extra_args = vec![
        "--visits-per-sec".into(),
        "100000".into(),
        "--request-log".into(),
        directory
            .path()
            .join("requests.jsonl")
            .to_str()
            .unwrap()
            .into(),
    ];
    extra_args.extend(extra);
    within(app.configure_engine(Some(EngineProfile {
        remote: None,
        human_model: None,
        program,
        model,
        config: engine_config,
        extra_args,
        canonical_model_name: None,
    })))
    .await
    .unwrap();
    within(async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::EngineStatusChanged(EngineStatus::Ready { .. }) => break,
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("fake engine failed to start: {reason}");
                }
                _ => {}
            }
        }
    })
    .await;
    Harness {
        directory,
        app,
        events,
    }
}

pub fn ai(visits: u64) -> AiPlayer {
    AiPlayer::KataGo(KataGoPlay {
        seconds: None,
        visits: Some(visits),
        resign: None,
        ..KataGoPlay::default()
    })
}

pub fn setup() -> GameSetup {
    GameSetup {
        white: Player::Ai(ai(1)),
        ponder: false,
        auto_save: false,
        ..GameSetup::default()
    }
}

pub async fn game(app: &AppCore) -> GameView {
    within(app.game_view()).await.unwrap().unwrap()
}

pub async fn moves(app: &AppCore, expected: u32) -> GameView {
    within(async {
        loop {
            let view = app.game_view().await.unwrap().unwrap();
            if view.move_number >= expected {
                return view;
            }
            assert_ne!(
                view.phase,
                GamePhase::Finished,
                "game ended before move {expected}: {view:?}"
            );
            tokio::task::yield_now().await;
        }
    })
    .await
}

pub async fn document(events: &mut AppSubscription, current: u32) -> DocumentView {
    within(async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::DocumentChanged { view, .. } if view.tree.current == current => {
                    return view;
                }
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("fake engine failed: {reason}");
                }
                _ => {}
            }
        }
    })
    .await
}

pub fn requests(directory: &Path) -> Vec<Value> {
    std::fs::read_to_string(directory.join("requests.jsonl"))
        .unwrap_or_default()
        .lines()
        // The process can be in the middle of writing the last line.
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

pub async fn query(directory: &Path, predicate: impl Fn(&Value) -> bool) -> Value {
    within(async {
        loop {
            if let Some(request) = requests(directory).into_iter().find(&predicate) {
                return request;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
}

// A command reply only acknowledges enqueueing. For absence/count assertions,
// wait for a later request on the same engine's FIFO and inspect the complete
// prefix before that marker, excluding the marker itself.
pub async fn requests_before(directory: &Path, predicate: impl Fn(&Value) -> bool) -> Vec<Value> {
    within(async {
        loop {
            let mut recorded = requests(directory);
            if let Some(index) = recorded.iter().position(&predicate) {
                recorded.truncate(index);
                return recorded;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
}
