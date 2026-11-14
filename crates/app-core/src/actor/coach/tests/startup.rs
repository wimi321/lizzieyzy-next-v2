use super::*;
use engine_session::EngineStatus;

#[tokio::test]
async fn stopped_or_timed_out_human_model_start_restores_the_original_engine() {
    for stopped in [true, false] {
        let mut h = harness(json!({}), false).await;
        let gate = h.directory.path().join("startup-gate");
        std::fs::write(&gate, b"ready").unwrap();
        let mut original = h.actor.settings.engine.clone().unwrap();
        original.extra_args.extend([
            "--startup-gate-file".into(),
            gate.to_string_lossy().into_owned(),
        ]);
        within(h.actor.configure_engine(Some(original.clone())))
            .await
            .unwrap();
        ready(h.actor.running.as_ref().unwrap().engine.status()).await;
        // Close only the fake's startup gate, keeping the original profile intact.
        std::fs::remove_file(&gate).unwrap();
        let (reply, result) = oneshot::channel();
        within(
            h.actor
                .coach_command(Request::Start(setup(CoachMode::ReviewAfter), reply)),
        )
        .await;
        assert!(
            h.actor
                .settings
                .engine
                .as_ref()
                .unwrap()
                .human_model
                .is_some()
        );
        if stopped {
            // Let the fake consume EOF so shutdown needs no wall-clock grace.
            std::fs::write(&gate, b"ready").unwrap();
            within(h.actor.running.as_ref().unwrap().engine.clone().shutdown())
                .await
                .unwrap();
        } else {
            let deadline = h.actor.coach.starting.as_ref().unwrap().deadline;
            tokio::time::advance(
                deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
            )
            .await;
        }
        let failed = within(super::super::receive(&mut h.actor.coach)).await;
        assert!(matches!(failed, Event::Started(Err(_))));
        std::fs::write(&gate, b"ready").unwrap();
        within(h.actor.coach_signal(failed)).await;
        assert!(h.actor.coach.is_starting());
        let restored = within(super::super::receive(&mut h.actor.coach)).await;
        assert!(matches!(restored, Event::Started(Ok(()))));
        within(h.actor.coach_signal(restored)).await;
        let error = within(result).await.unwrap().unwrap_err();
        assert!(
            matches!(
                error,
                AppError::Coach(CoachError::HumanModelLoadFailed { ref reason })
                    if reason.contains(if stopped { "stopped before Ready" } else { "timed out" })
            ),
            "{error:?}"
        );
        assert!(!h.actor.coach.is_starting());
        assert_eq!(h.actor.settings.engine.as_ref(), Some(&original));
        assert_eq!(
            crate::Settings::load(h.directory.path())
                .await
                .unwrap()
                .engine,
            Some(original)
        );
        ready(h.actor.running.as_ref().unwrap().engine.status()).await;
        within(h.actor.set_pondering(true)).await.unwrap();
        let ordinary = query(h.directory.path(), |r| r["maxVisits"] == 20).await;
        assert!(ordinary["overrideSettings"].get("humanSLProfile").is_none());
        close(h).await;
    }
}

#[tokio::test]
async fn already_loaded_model_start_failure_keeps_existing_profile() {
    let mut h = harness(json!({}), true).await;
    let original = h.actor.settings.engine.clone();
    let (reply, result) = oneshot::channel();
    within(
        h.actor
            .coach_command(Request::Start(setup(CoachMode::ReviewAfter), reply)),
    )
    .await;
    // The model was loaded before coaching; this unrelated stop must not clear it.
    let mut status = h.actor.running.as_ref().unwrap().engine.status();
    within(h.actor.running.as_ref().unwrap().engine.clone().shutdown())
        .await
        .unwrap();
    within(async {
        while !matches!(*status.borrow_and_update(), EngineStatus::Stopped) {
            status.changed().await.unwrap();
        }
    })
    .await;
    let stopped = within(super::super::receive(&mut h.actor.coach)).await;
    within(h.actor.coach_signal(stopped)).await;
    assert!(matches!(
        within(result).await.unwrap(),
        Err(AppError::Game(_))
    ));
    assert!(!h.actor.coach.is_starting());
    assert_eq!(h.actor.settings.engine, original);
    assert_eq!(
        crate::Settings::load(h.directory.path())
            .await
            .unwrap()
            .engine,
        original
    );
    close(h).await;
}
