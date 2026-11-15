#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[test]
fn acceleration_reservation_rejects_coach_start_without_cancelling_its_transaction() {
    let (mut actor, _) = crate::actor::tests::actor();
    actor.acceleration.backend = Some(AccelerationBackend::OpenCl);
    let settings = actor.settings.clone();
    let (reply, mut response) = oneshot::channel();
    let command = Command::Coach(crate::coach_api::Request::Start(Default::default(), reply));
    assert!(actor.acceleration_guard(command).is_none());
    assert!(matches!(
        response.try_recv().unwrap(),
        Err(AppError::Acceleration(AccelerationError::Busy))
    ));
    assert_eq!(actor.settings, settings);
    assert_eq!(
        actor.acceleration.backend,
        Some(AccelerationBackend::OpenCl)
    );
    assert!(!actor.coach.is_starting());

    // Releasing the reservation restores the ordinary coach command path.
    actor.acceleration.backend = None;
    let (reply, _) = oneshot::channel();
    let command = Command::Coach(crate::coach_api::Request::Start(Default::default(), reply));
    assert!(matches!(
        actor.acceleration_guard(command),
        Some(Command::Coach(_))
    ));
}

#[tokio::test]
async fn typed_startup_failure_keeps_only_last_twenty_lines_without_repeating_them_in_reason() {
    let (mut actor, _) = crate::actor::tests::actor();
    actor.acceleration.backend = Some(AccelerationBackend::Cuda);
    actor.acceleration.output = vec!["earlier logfile line".into()];
    let error = crate::EngineSetupError::Failed {
        reason: "startup failed".into(),
        stderr_tail: (0..200).map(|i| format!("stderr line {i}")).collect(),
    };
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        actor.acceleration_failed(error.into(), false),
    )
    .await
    .unwrap();
    let AppError::Acceleration(AccelerationError::Failed {
        reason,
        stderr_tail,
    }) = outcome
    else {
        panic!("expected typed acceleration failure");
    };
    assert_eq!(reason, "startup failed");
    assert_eq!(stderr_tail.len(), 20);
    assert_eq!(stderr_tail[0], "stderr line 180");
    assert_eq!(stderr_tail[19], "stderr line 199");
    assert!(actor.acceleration.backend.is_none());
}
