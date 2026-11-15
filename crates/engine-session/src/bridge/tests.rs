#![allow(clippy::unwrap_used)]
use super::*;
fn query(id: &str) -> String {
    json!({"id":id,"moves":[],"rules":"chinese","boardXSize":2,"boardYSize":2,"maxVisits":1000})
        .to_string()
}
fn values(state: &mut State) -> Vec<Value> {
    state
        .outputs
        .drain(..)
        .filter_map(|line| match line {
            Output::Stdout(line) => Some(serde_json::from_str(&line).unwrap()),
            _ => None,
        })
        .collect()
}
#[test]
fn queued_cancel_no_results_precedes_confirmation() {
    let mut state = State::new();
    state.input(&query("q"));
    state.input(r#"{"id":"cancel","action":"terminate","terminateId":"q"}"#);
    let values = values(&mut state);
    assert_eq!(values.len(), 2);
    assert_eq!(values[0]["noResults"], true);
    assert_eq!(values[1]["action"], "terminate");
    assert!(state.queue.is_empty());
}
#[tokio::test]
async fn running_cancel_final_with_visits_precedes_confirmation() {
    let mut state = State::new();
    state.input(&query("q"));
    state.pump();
    while let Some((id, _)) = state.pending.as_ref() {
        let id = *id;
        state.reply(Response {
            id: Some(id),
            success: true,
            body: String::new(),
        });
        state.pump();
    }
    state.analysis("info move A1 visits 20 winrate 0.6 scoreLead 1 prior 0.3 order 0 pv A1 rootInfo visits 40 winrate 0.6 scoreLead 1 ownership 0 0 0 0");
    values(&mut state);
    state.input(r#"{"id":"cancel","action":"terminate","terminateId":"q"}"#);
    let stop = state.pending.as_ref().unwrap().0;
    assert!(state.writes.back().unwrap().ends_with(" stop"));
    state.reply(Response {
        id: Some(stop),
        success: true,
        body: String::new(),
    });
    let values = values(&mut state);
    assert_eq!(values.len(), 2);
    assert_eq!(values[0]["rootInfo"]["visits"], 40);
    assert_eq!(values[0]["isDuringSearch"], false);
    assert_eq!(values[1]["action"], "terminate");
}

#[tokio::test]
async fn cancel_during_replay_sends_stop_immediately_and_drops_late_old_report() {
    let mut state = State::new();
    state.input(&query("old"));
    state.pump();
    let setup_id = state.pending.as_ref().unwrap().0;
    state.input(r#"{"id":"cancel","action":"terminate","terminateId":"old"}"#);
    let stop_id = state.pending.as_ref().unwrap().0;
    assert_ne!(setup_id, stop_id);
    assert!(state.writes.back().unwrap().ends_with(" stop"));
    state.reply(Response {
        id: Some(setup_id),
        success: true,
        body: String::new(),
    });
    state.reply(Response {
        id: Some(stop_id),
        success: true,
        body: String::new(),
    });
    let cancelled = values(&mut state);
    assert_eq!(cancelled[0]["noResults"], true);
    assert_eq!(cancelled[1]["action"], "terminate");
    state.input(&query("next"));
    state.pump();
    state.analysis("info move A1 visits 99 winrate 0.9 scoreLead 9 prior 0.3 order 0 pv A1 rootInfo visits 100 winrate 0.9 scoreLead 9");
    assert!(values(&mut state).is_empty());
    assert!(state.active.as_ref().unwrap().report.root.is_none());
}

#[test]
fn invalid_time_is_typed_and_absent_override_keeps_default() {
    let mut state = State::new();
    let mut input: Value = serde_json::from_str(&query("zero")).unwrap();
    input["overrideSettings"] = json!({"maxTime":0});
    state.input(&input.to_string());
    assert_eq!(values(&mut state)[0]["field"], "zhiziBridge.InvalidQuery");
    state.input(&query("default"));
    assert_eq!(state.queue.len(), 1);
    assert_eq!(query::max_time(&state.queue[0].query).unwrap(), None);
}
