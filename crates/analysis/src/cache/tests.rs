#![allow(clippy::unwrap_used)]

use super::*;
use katago_protocol::{EngineLine, parse_line};

fn response(visits: u64) -> AnalysisResponse {
    let line = format!(
        r#"{{"id":"query-{visits}","turnNumber":0,"isDuringSearch":true,"moveInfos":[],
        "rootInfo":{{"visits":{visits},"winrate":0.7,"scoreLead":3.5,
        "utility":0.4,"currentPlayer":"B"}}}}"#
    );
    let EngineLine::Analysis(response) = parse_line(&line).unwrap() else {
        panic!("expected analysis response");
    };
    *response
}

#[test]
fn deepest_result_survives_shallower_refresh_and_equal_visits_update() {
    let mut cache = Cache::new(NonZeroUsize::new(2).unwrap());
    let key = AnalysisKey([1; 32]);
    cache.insert(key, "engine-a", response(100));
    cache.insert(key, "engine-a", response(10));
    assert_eq!(cache.get(key, "engine-a"), Some(response(100)));
    cache.insert(key, "engine-a", response(200));
    assert_eq!(cache.get(key, "engine-a"), Some(response(200)));

    let mut final_response = response(200);
    final_response.is_during_search = false;
    cache.insert(key, "engine-a", final_response.clone());
    assert_eq!(cache.get(key, "engine-a"), Some(final_response));
}

#[test]
fn fingerprints_and_position_keys_are_independent() {
    let mut cache = Cache::new(NonZeroUsize::new(3).unwrap());
    let first = AnalysisKey([1; 32]);
    let second = AnalysisKey([2; 32]);
    cache.insert(first, "engine-a", response(100));
    cache.insert(first, "engine-b", response(20));
    cache.insert(second, "engine-a", response(30));
    assert_eq!(cache.get(first, "engine-a"), Some(response(100)));
    assert_eq!(cache.get(first, "engine-b"), Some(response(20)));
    assert_eq!(cache.get(second, "engine-a"), Some(response(30)));
    assert_eq!(cache.get(second, "engine-b"), None);
}

#[test]
fn access_and_shallow_refresh_both_update_lru_recency() {
    let mut cache = Cache::new(NonZeroUsize::new(2).unwrap());
    let first = AnalysisKey([1; 32]);
    let second = AnalysisKey([2; 32]);
    let third = AnalysisKey([3; 32]);
    cache.insert(first, "engine", response(100));
    cache.insert(second, "engine", response(200));
    assert!(cache.get(first, "engine").is_some());
    cache.insert(third, "engine", response(300));
    assert!(cache.get(second, "engine").is_none());
    cache.insert(first, "engine", response(10));
    cache.insert(second, "engine", response(200));
    assert!(cache.get(third, "engine").is_none());
    assert_eq!(cache.get(first, "engine"), Some(response(100)));
    assert_eq!(cache.entries.len(), 2);
}

#[test]
fn capacity_one_replaces_entries_and_missing_reads_do_not_touch_recency() {
    let mut cache = Cache::new(NonZeroUsize::new(1).unwrap());
    let key = AnalysisKey([1; 32]);
    cache.insert(key, "engine-a", response(100));
    assert!(cache.get(key, "missing-engine").is_none());
    cache.insert(key, "engine-b", response(20));
    assert!(cache.get(key, "engine-a").is_none());
    assert_eq!(cache.get(key, "engine-b"), Some(response(20)));
    assert_eq!(cache.entries.len(), 1);
}
