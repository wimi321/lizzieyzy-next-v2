use super::remember_state;
use app_core::{AppEvent, CoachSummary, CoachView, ColorView};

#[test]
fn new_coach_session_clears_old_summary_and_keeps_current_view_in_publication_order() {
    let summary = CoachSummary::from_ratings(None, None, 32, 2, 1, &[]);
    let mut replay = vec![];
    remember_state(&mut replay, &AppEvent::CoachSummary(summary.clone()));
    let mut view = CoachView {
        active: true,
        setup: Default::default(),
        human_color: ColorView::Black,
        awaiting_decision: false,
        hints: 2,
        take_backs: 1,
        last_rating: None,
        summary: Some(summary),
    };
    remember_state(&mut replay, &AppEvent::CoachUpdated(view.clone()));
    assert_eq!(replay.len(), 2);
    assert!(matches!(replay[0], AppEvent::CoachSummary(_)));
    view.summary = None;
    view.hints = 0;
    view.take_backs = 0;
    remember_state(&mut replay, &AppEvent::CoachUpdated(view.clone()));
    assert_eq!(replay.len(), 1);
    assert!(matches!(&replay[0], AppEvent::CoachUpdated(current) if current == &view));
}
