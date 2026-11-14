use super::*;

#[test]
fn verification_cooldown_expires_exactly_at_deadline_and_rounds_up_remaining_seconds() {
    let start = Instant::now();
    for seconds in [7, 60, 600] {
        let mut timing = AccountTiming::default();
        timing.remember_code("account".into(), seconds, start);
        assert!(timing.check_code("another-account", start).is_ok());
        let first = timing.check_code("account", start).unwrap_err();
        assert_eq!(first.kind, ZhiziErrorKind::TooFrequent);
        assert_eq!(first.retry_after_seconds, seconds);
        let deadline = start + Duration::from_secs(u64::from(seconds));
        assert_eq!(
            timing
                .check_code("account", deadline - Duration::from_millis(1))
                .unwrap_err()
                .retry_after_seconds,
            1
        );
        assert!(timing.check_code("account", deadline).is_ok());
    }
}

#[test]
fn expired_cooldowns_release_capacity_for_another_account() {
    let start = Instant::now();
    let mut timing = AccountTiming::default();
    for i in 0..128 {
        timing.remember_code(i.to_string(), 60, start);
    }
    assert_eq!(
        timing
            .check_code("next", start)
            .unwrap_err()
            .retry_after_seconds,
        600
    );
    assert!(
        timing
            .check_code("next", start + Duration::from_secs(60))
            .is_ok()
    );
}

#[test]
fn overview_cache_expires_at_45_seconds_and_force_or_logout_bypasses_it() {
    let start = Instant::now();
    let mut timing = AccountTiming::default();
    assert!(timing.cached_overview(false, start).is_none());
    let value = ZhiziOverview::default();
    timing.cache_overview(value.clone(), start);
    assert_eq!(
        timing.cached_overview(false, start + Duration::from_millis(44999)),
        Some(value)
    );
    assert!(timing.cached_overview(true, start).is_none());
    assert!(
        timing
            .cached_overview(false, start + Duration::from_secs(45))
            .is_none()
    );
    timing.clear_overview();
    assert!(timing.cached_overview(false, start).is_none());
}
