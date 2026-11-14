use super::*;
use crate::{AppSubscription, EngineProfile, assets::AssetCatalog};
use serde_json::{Value, json};
use std::{future::Future, path::Path};

pub(super) async fn within<T>(future: impl Future<Output = T>) -> T {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut future = Box::pin(future);
    loop {
        // A runnable yield prevents Tokio's paused clock from auto-advancing
        // while the external fake process is still answering over stdio.
        tokio::select! {
            biased;
            result = &mut future => return result,
            () = tokio::task::yield_now() => {
                assert!(std::time::Instant::now() < deadline, "coach safety deadline");
            }
        }
    }
}

pub(super) struct Harness {
    pub actor: Box<Actor>,
    pub output: AppSubscription,
    pub directory: tempfile::TempDir,
}

pub(super) fn policy(values: &[(usize, f64)], area: usize) -> Vec<f64> {
    let mut result = vec![0.0; area + 1];
    for (index, value) in values {
        result[*index] = *value;
    }
    result
}

pub(super) fn feedback_script() -> Value {
    json!({"profiles":{"rank_3d":{"turns":{
        "0":{"humanPolicy":policy(&[(0,0.05),(1,0.80),(2,0.15)],81),"humanScoreStdev":12,
             "candidates":[{"move":"A9","visitShare":0.6,"scoreLead":0},
                           {"move":"B9","visitShare":0.3,"scoreLead":-15},
                           {"move":"C9","visitShare":0.1,"scoreLead":-25}]},
        "1":{"humanPolicy":policy(&[(9,1.0)],81),"candidates":[{"move":"A8","visitShare":1.0}]},
        "2":{"humanPolicy":policy(&[(3,1.0)],81),"candidates":[{"move":"D9","visitShare":1.0}]}
    }}}})
}

pub(super) async fn harness(script: Value, human_loaded: bool) -> Harness {
    let directory = tempfile::tempdir().unwrap();
    let source = fake_katago::binary_path().unwrap();
    let program = directory
        .path()
        .join(format!("katago-eigen{}", std::env::consts::EXE_SUFFIX));
    std::fs::hard_link(&source, &program)
        .or_else(|_| std::fs::copy(&source, &program).map(|_| ()))
        .unwrap();
    let config = directory.path().join("analysis.cfg");
    let model = directory.path().join("main.bin");
    let human = directory.path().join("weights/human-sl.bin.gz");
    std::fs::create_dir_all(human.parent().unwrap()).unwrap();
    std::fs::write(&config, "reportAnalysisWinratesAs = BLACK\n").unwrap();
    std::fs::write(&model, b"main-model").unwrap();
    std::fs::write(&human, b"human-model").unwrap();
    std::fs::write(
        directory.path().join("script.json"),
        serde_json::to_vec(&script).unwrap(),
    )
    .unwrap();
    let (actor, output) = super::super::super::tests::actor();
    let mut actor = Box::new(actor);
    actor.directory = directory.path().to_owned();
    actor.settings.pondering = false;
    actor.settings.auto_quick_analyze_on_load = false;
    actor.settings.cache_enabled = false;
    actor.settings.games_directory = Some(directory.path().join("MyGames"));
    actor.config.asset_catalog = Some(AssetCatalog::test_weight(
        "human-sl",
        "http://127.0.0.1/unused",
        b"human-model",
    ));
    actor.config.game_random_seed = Some(1234);
    actor.config.analysis.live.max_visits = 20;
    actor.config.session.startup_timeout = Duration::from_secs(60);
    actor.config.session.response_timeout = Duration::from_secs(60);
    actor.config.session.write_timeout = Duration::from_secs(60);
    actor.config.session.shutdown_timeout = Duration::from_secs(60);
    actor.config.session.max_restarts = 0;
    let args = vec![
        "--request-log".into(),
        directory
            .path()
            .join("requests.jsonl")
            .to_string_lossy()
            .into_owned(),
        "--launch-log".into(),
        directory
            .path()
            .join("launch.jsonl")
            .to_string_lossy()
            .into_owned(),
        "--play-script".into(),
        directory
            .path()
            .join("script.json")
            .to_string_lossy()
            .into_owned(),
        "--visits-per-sec".into(),
        "1000000".into(),
    ];
    within(actor.configure_engine(Some(EngineProfile {
        program,
        model,
        config,
        extra_args: args,
        remote: None,
        human_model: human_loaded.then_some(human),
        canonical_model_name: None,
    })))
    .await
    .unwrap();
    ready(actor.running.as_ref().unwrap().engine.status()).await;
    tokio::time::pause();
    Harness {
        actor,
        output,
        directory,
    }
}

pub(super) async fn ready(mut status: watch::Receiver<engine_session::EngineStatus>) {
    within(async {
        loop {
            match status.borrow_and_update().clone() {
                engine_session::EngineStatus::Ready { .. } => break,
                engine_session::EngineStatus::Failed { reason, .. } => {
                    panic!("fake failed: {reason}")
                }
                _ => {}
            }
            status.changed().await.unwrap();
        }
    })
    .await;
}

pub(super) fn setup(mode: CoachMode) -> CoachSetup {
    CoachSetup {
        width: 9,
        height: 9,
        color: crate::CoachColor::Black,
        mode,
        auto_save: false,
        pace: crate::CoachPace::Fast,
        ..Default::default()
    }
}

pub(super) async fn start(actor: &mut Actor, setup: CoachSetup) -> Result<CoachView, AppError> {
    let (reply, receive) = oneshot::channel();
    within(actor.coach_command(Request::Start(setup, reply))).await;
    if actor.coach.starting.is_some() {
        let event = within(super::super::receive(&mut actor.coach)).await;
        within(actor.coach_signal(event)).await;
    }
    within(receive).await.unwrap()
}

pub(super) async fn drive(actor: &mut Actor, done: impl Fn(&Actor) -> bool) {
    within(async {
        while !done(actor) {
            tokio::select! {
                event=super::super::receive(&mut actor.coach)=>{
                    if matches!(&event, Event::Ai(event) if matches!(event.as_ref(), Some(QueryEvent::Final(_)))) {
                        // Fixed virtual search cost exercises throughput-based
                        // deepening without letting host load consume the pace.
                        tokio::time::advance(Duration::from_millis(1)).await;
                    }
                    actor.coach_signal(event).await;
                },
                signal=super::super::super::next_signal(&mut actor.running)=>actor.signal(signal),
            }
            if let Some(view) = actor.game_view() {
                assert!(view.error.is_none(), "game error: {:?}", view.error);
            }
        }
    })
    .await;
}

pub(super) async fn cached(actor: &mut Actor) {
    drive(actor, |actor| {
        actor
            .coach
            .session
            .as_ref()
            .is_some_and(|s| s.before.is_some() && s.review.is_none())
    })
    .await;
}

pub(super) async fn selected(actor: &mut Actor) {
    drive(actor, |actor| {
        actor
            .coach
            .session
            .as_ref()
            .and_then(|s| s.ai.as_ref())
            .is_some_and(|ai| ai.query.is_none() && ai.ready_at.is_some())
    })
    .await;
}

pub(super) async fn deliver(actor: &mut Actor) {
    selected(actor).await;
    let at = actor
        .coach
        .session
        .as_ref()
        .unwrap()
        .ai
        .as_ref()
        .unwrap()
        .ready_at
        .unwrap();
    tokio::time::advance(at.saturating_duration_since(Instant::now())).await;
    within(actor.maybe_coach_move()).await.unwrap();
}

pub(super) fn requests(directory: &Path) -> Vec<Value> {
    std::fs::read_to_string(directory.join("requests.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

pub(super) async fn query(directory: &Path, predicate: impl Fn(&Value) -> bool) -> Value {
    within(async {
        loop {
            if let Some(value) = requests(directory).into_iter().find(&predicate) {
                return value;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
}

// Same FIFO barrier as integration tests/common/game.rs: submit a later query
// and wait until fake-katago has written it before asserting absence/counts.
pub(super) async fn requests_before(actor: &Actor, directory: &Path) -> Vec<Value> {
    let position = actor
        .document
        .position_at(actor.document.current())
        .unwrap();
    let request = analysis::play_request(
        &position,
        &actor.config.analysis.live,
        Some(37),
        None,
        false,
    )
    .unwrap();
    let subscription = within(actor.running.as_ref().unwrap().engine.submit(request))
        .await
        .unwrap();
    let id = subscription.id().to_owned();
    query(directory, |r| r["id"] == id).await;
    requests(directory)
        .into_iter()
        .take_while(|r| r["id"] != id)
        .collect()
}

pub(super) async fn close(mut harness: Harness) {
    within(harness.actor.cancel_coach_queries()).await;
    within(harness.actor.stop_engine()).await.unwrap();
    within(harness.actor.assets.shutdown()).await.unwrap();
    tokio::time::resume();
}
