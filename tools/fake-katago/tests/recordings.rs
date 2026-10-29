#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use common::Engine;
use serde_json::Value;

#[derive(Debug)]
struct Event {
    input: bool,
    value: Value,
}

fn read_trace(path: &Path) -> Vec<Event> {
    let mut events = Vec::<Event>::new();
    for line in std::fs::read_to_string(path).unwrap().lines() {
        let record: Value = serde_json::from_str(line).unwrap();
        let Some(direction) = record["dir"].as_str() else {
            continue;
        };
        if !matches!(direction, "in" | "out") {
            continue;
        }
        let value: Value = serde_json::from_str(record["line"].as_str().unwrap()).unwrap();
        let input = direction == "in";
        // Sampling frequency depends on the neural backend. Preserve the
        // partial-before-final transition while collapsing repeated samples.
        if !input
            && value["isDuringSearch"] == true
            && events.last().is_some_and(|event| {
                !event.input
                    && event.value["isDuringSearch"] == true
                    && event.value["id"] == value["id"]
                    && event.value["turnNumber"] == value["turnNumber"]
            })
        {
            continue;
        }
        events.push(Event { input, value });
    }
    events
}

fn keys(value: &Value) -> BTreeSet<&str> {
    value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect()
}

fn assert_shape(actual: &Value, recorded: &Value) {
    assert_eq!(keys(actual), keys(recorded), "top-level fields");
    for field in [
        "id",
        "action",
        "terminateId",
        "turnNumber",
        "isDuringSearch",
        "noResults",
        "field",
    ] {
        assert_eq!(actual.get(field), recorded.get(field), "field {field}");
    }
    if let Some(root) = recorded.get("rootInfo") {
        assert_eq!(keys(&actual["rootInfo"]), keys(root), "root fields");
        assert_eq!(actual["rootInfo"]["currentPlayer"], root["currentPlayer"]);
        let rate = actual["rootInfo"]["winrate"].as_f64().unwrap();
        assert!((0.0..=1.0).contains(&rate));
        assert!(actual["rootInfo"]["visits"].as_u64().unwrap() > 0);
        assert!(matches!(
            actual["rootInfo"]["currentPlayer"].as_str(),
            Some("B" | "W")
        ));
    }
    if let Some(recorded_moves) = recorded.get("moveInfos") {
        let candidates = actual["moveInfos"].as_array().unwrap();
        assert!(!candidates.is_empty());
        let mut base_keys = keys(&recorded_moves[0]);
        // Symmetry pruning is conditional on the searched position; it is
        // the only optional move field present in these recordings.
        base_keys.remove("isSymmetryOf");
        for (index, candidate) in candidates.iter().enumerate() {
            let mut candidate_keys = keys(candidate);
            candidate_keys.remove("isSymmetryOf");
            assert_eq!(candidate_keys, base_keys, "candidate fields");
            assert_eq!(candidate["order"], index);
            assert!(candidate["move"].is_string());
            assert!(
                candidate["pv"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(Value::is_string)
            );
            let rate = candidate["winrate"].as_f64().unwrap();
            assert!((0.0..=1.0).contains(&rate));
        }
    }
    for field in ["ownership", "policy"] {
        if let Some(recorded_array) = recorded.get(field) {
            let actual_array = actual[field].as_array().unwrap();
            assert_eq!(actual_array.len(), recorded_array.as_array().unwrap().len());
            assert!(actual_array.iter().all(|number| {
                number
                    .as_f64()
                    .is_some_and(|value| (-1.0..=1.0).contains(&value))
            }));
        }
    }
}

fn identity(value: &Value) -> (String, u64) {
    (
        value["id"].as_str().unwrap().to_owned(),
        value["turnNumber"].as_u64().unwrap(),
    )
}

fn observe(
    actual: &Value,
    visits: &mut BTreeMap<(String, u64), u64>,
    finished: &mut BTreeSet<(String, u64)>,
) {
    if actual.get("isDuringSearch").is_none() {
        return;
    }
    let key = identity(actual);
    assert!(
        !finished.contains(&key),
        "output after a final result: {actual}"
    );
    if let Some(count) = actual["rootInfo"]["visits"].as_u64() {
        let previous = visits.insert(key.clone(), count).unwrap_or_default();
        assert!(count >= previous, "visits regressed");
    }
    if actual["isDuringSearch"] == false {
        finished.insert(key);
    }
}

#[test]
fn every_recorded_scenario_matches_fields_transitions_and_cancellation_order() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/katago");
    let mut paths = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .collect::<Vec<_>>();
    paths.sort();
    assert_eq!(paths.len(), 11, "all P0-03 scenarios must be exercised");
    for path in paths {
        eprintln!("replaying {}", path.display());
        let trace = read_trace(&path);
        let mut engine = Engine::spawn(&["--visits-per-sec", "100", "--threads", "1"]);
        let mut visits = BTreeMap::new();
        let mut finished = BTreeSet::new();
        let mut partial_shapes = BTreeMap::new();
        for event in trace {
            if event.input {
                engine.send(&event.value);
                continue;
            }
            let actual = loop {
                let actual = engine.recv();
                observe(&actual, &mut visits, &mut finished);
                if actual["isDuringSearch"] == true && event.value["isDuringSearch"] != true {
                    let prior = partial_shapes
                        .get(&identity(&actual))
                        .expect("unsolicited partial or partial after final");
                    assert_shape(&actual, prior);
                    continue;
                }
                break actual;
            };
            assert_shape(&actual, &event.value);
            if actual["isDuringSearch"] == true {
                partial_shapes.insert(identity(&actual), event.value);
            }
        }
        engine.finish();
    }
}
