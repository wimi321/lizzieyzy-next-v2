#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::{ColorView, test_support::within};

#[tokio::test(start_paused = true)]
async fn autosave_waits_for_coach_preparation_and_startup_to_finish() {
    for preparing in [true, false] {
        let directory = tempfile::tempdir().unwrap();
        let (mut actor, _) = crate::actor::tests::actor();
        actor.directory = directory.path().to_owned();
        actor
            .document
            .apply(document::Command::SetComment("changed".into()))
            .unwrap();
        actor.coach.preparing = preparing;
        let (reply, _response) = oneshot::channel();
        if !preparing {
            actor.coach.starting = Some(Starting {
                setup: CoachSetup::default(),
                reply,
                status: None,
                deadline: Instant::now() + Duration::from_secs(120),
                phase: StartupPhase::Loading { restore: None },
                error: None,
            });
        }
        assert!(actor.game.is_none());
        assert!(actor.coach.is_starting());
        within(actor.autosave(false)).await;
        within(actor.autosave(true)).await;
        assert!(!directory.path().join("saves").exists());

        actor.coach.preparing = false;
        actor.coach.starting = None;
        within(actor.autosave(false)).await;
        let slots = within(crate::saves::SlotStore::new(directory.path().to_owned()).list()).await;
        assert_eq!(slots.len(), 1);
        assert!(slots[0].is_autosave);
        assert!(!slots[0].corrupt);
    }
}

#[tokio::test(start_paused = true)]
async fn active_coach_without_game_session_blocks_autosave_until_review_exits() {
    for mode in [CoachMode::ReviewAfter, CoachMode::LiveAnalysis] {
        let directory = tempfile::tempdir().unwrap();
        let (mut actor, _) = crate::actor::tests::actor();
        actor.directory = directory.path().to_owned();
        actor
            .document
            .apply(document::Command::SetComment("changed".into()))
            .unwrap();
        actor.coach.session = Some(Session {
            view: CoachView {
                active: true,
                setup: CoachSetup {
                    mode,
                    ..CoachSetup::default()
                },
                human_color: ColorView::Black,
                awaiting_decision: false,
                hints: 0,
                take_backs: 0,
                last_rating: None,
                summary: None,
            },
            active: true,
            ai: None,
            review: None,
            before: None,
            rating: None,
            ratings: Vec::new(),
            hints: HashMap::new(),
            low_winrates: 0,
        });
        assert!(
            actor.game.is_none(),
            "isolate the coach guard from the game guard"
        );
        assert!(actor.coach_view().unwrap().active);
        within(actor.autosave(false)).await;
        within(actor.autosave(true)).await;
        assert!(!directory.path().join("saves").exists());

        actor.coach.end();
        assert!(!actor.coach_view().unwrap().active);
        within(actor.autosave(false)).await;
        let slots = within(crate::saves::SlotStore::new(directory.path().to_owned()).list()).await;
        assert_eq!(slots.len(), 1);
        assert!(slots[0].is_autosave);
        assert!(!slots[0].corrupt);
    }
}
