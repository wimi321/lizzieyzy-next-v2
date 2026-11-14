#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{collections::BTreeMap, ffi::OsString, time::Duration};

use analysis::{AnalysisConfig, Focus, LiveSettings, SnapshotSource};
use document::GameDocument;
use engine_session::{QueryEvent, SessionConfig, TransportSpec};
use serde_json::{Value, json};
use tokio::time::timeout;

#[tokio::test]
async fn loaded_human_model_does_not_add_profile_to_live_analysis_requests() {
    let directory = tempfile::tempdir().unwrap();
    let log = directory.path().join("requests.jsonl");
    let mut args: Vec<OsString> = [
        "analysis",
        "-config",
        "unused.cfg",
        "-model",
        "unused.bin.gz",
        "-human-model",
        "unused-human.bin.gz",
        "--request-log",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    args.push(log.as_os_str().to_owned());
    let engine = engine_session::spawn(
        TransportSpec::LocalProcess {
            program: fake_katago::binary_path().unwrap(),
            args,
            cwd: None,
            env: BTreeMap::new(),
        },
        SessionConfig::default(),
    );
    let live = LiveSettings {
        max_visits: 1,
        include_policy: true,
        ..LiveSettings::default()
    };
    let (handle, mut output) = analysis::spawn(
        engine.clone(),
        AnalysisConfig {
            live: live.clone(),
            ..AnalysisConfig::default()
        },
    )
    .unwrap();
    let document = GameDocument::new();
    let position = document.position_at(document.root()).unwrap();
    handle
        .focus(Focus {
            revision: 1,
            node_id: document.root(),
            key: position.analysis_key(),
            position: position.clone(),
        })
        .await
        .unwrap();
    let normal = timeout(Duration::from_secs(5), async {
        loop {
            let snapshot = output.recv().await.unwrap();
            if snapshot.source == SnapshotSource::Live && !snapshot.response.is_during_search {
                break snapshot.response;
            }
        }
    })
    .await
    .unwrap();
    assert!(normal.human_policy.is_none());
    handle.set_pondering(false).await.unwrap();

    let mut human = analysis::play_request(&position, &live, Some(1), Some(10.0), false).unwrap();
    human
        .override_settings
        .as_mut()
        .unwrap()
        .insert("humanSLProfile".into(), json!("rank_3d"));
    human.extensions.insert(
        "allowMoves".into(),
        json!([{"player":"B","moves":["D4","Q16"],"untilDepth":1}]),
    );
    let mut subscription = engine.submit(human).await.unwrap();
    let human = timeout(Duration::from_secs(5), async {
        loop {
            match subscription.recv().await.unwrap() {
                QueryEvent::Final(response) => break response,
                QueryEvent::Partial(_) => {}
                event => panic!("unexpected HumanSL event: {event:?}"),
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(human.human_policy.as_ref().unwrap().len(), 362);
    assert!(human.root_info.human_score_stdev.is_some());
    assert!(
        human
            .move_infos
            .iter()
            .all(|info| ["D4", "Q16"].contains(&info.coordinate.as_str()))
    );

    // The final response above is a subprocess barrier: all earlier requests
    // have been logged, so absence/count assertions cannot race command return.
    let requests: Vec<Value> = std::fs::read_to_string(log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|value: &Value| value.get("moves").is_some())
        .collect();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].get("humanSLProfile").is_none());
    assert!(
        requests[0]["overrideSettings"]
            .get("humanSLProfile")
            .is_none()
    );
    assert_eq!(requests[1]["overrideSettings"]["humanSLProfile"], "rank_3d");
    assert_eq!(requests[1]["allowMoves"][0]["moves"], json!(["D4", "Q16"]));
    handle.shutdown().await.unwrap();
    engine.shutdown().await.unwrap();
}
