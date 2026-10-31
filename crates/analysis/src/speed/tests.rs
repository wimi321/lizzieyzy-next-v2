use super::*;
use tokio::time::advance;

#[tokio::test(start_paused = true)]
async fn uniform_partials_measure_visits_per_second_after_half_a_second() {
    let mut speed = SearchSpeed::default();
    assert_eq!(speed.visits_per_second(), None);
    speed.record(100);
    assert_eq!(speed.visits_per_second(), None);
    advance(Duration::from_millis(499)).await;
    speed.record(599);
    assert_eq!(speed.visits_per_second(), None);
    advance(Duration::from_millis(1)).await;
    speed.record(600);
    assert_eq!(speed.visits_per_second(), Some(1000.0));
    for visits in [1100, 1600, 2100, 2600, 3100, 3600, 4100] {
        advance(Duration::from_millis(500)).await;
        speed.record(visits);
        assert_eq!(speed.visits_per_second(), Some(1000.0));
    }
}

#[tokio::test(start_paused = true)]
async fn window_uses_only_first_and_last_samples_within_three_seconds() {
    let mut speed = SearchSpeed::default();
    speed.record(100);
    advance(Duration::from_secs(1)).await;
    speed.record(200);
    advance(Duration::from_secs(2)).await;
    speed.record(700);
    // The sample exactly three seconds old is included.
    assert_eq!(speed.visits_per_second(), Some(200.0));
    advance(Duration::from_secs(1)).await;
    speed.record(1100);
    assert_eq!(speed.visits_per_second(), Some(300.0));
    advance(Duration::from_secs(4)).await;
    // No new Partial means the last speed is frozen, even beyond the window.
    assert_eq!(speed.visits_per_second(), Some(300.0));
    speed.record(1200);
    assert_eq!(speed.visits_per_second(), None);
}

#[tokio::test(start_paused = true)]
async fn repeated_timestamps_and_decreasing_visits_do_not_create_invalid_rates() {
    let mut speed = SearchSpeed::default();
    speed.record(100);
    speed.record(200);
    assert_eq!(speed.visits_per_second(), None);
    advance(Duration::from_secs(1)).await;
    speed.record(1100);
    assert_eq!(speed.visits_per_second(), Some(1000.0));
    speed.record(10);
    assert_eq!(speed.visits_per_second(), None);
    advance(Duration::from_secs(1)).await;
    speed.record(10);
    assert_eq!(speed.visits_per_second(), Some(0.0));
}
