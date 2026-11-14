use super::*;

#[tokio::test]
async fn low_version_tensorrt_missing_and_corrupt_models_fail_without_restarting() {
    for case in ["old", "tensorrt", "missing", "corrupt"] {
        let mut h = harness(json!({}), true).await;
        let mut version_sender = None;
        match case {
            "old" => {
                let (sender, receiver) = watch::channel(engine_session::EngineStatus::Ready {
                    version: "1.14.9".into(),
                });
                version_sender = Some(sender);
                h.actor.running.as_mut().unwrap().engine_status = receiver;
            }
            "tensorrt" => {
                let profile = h.actor.settings.engine.as_mut().unwrap();
                let path = h
                    .directory
                    .path()
                    .join(format!("katago-tensorrt{}", std::env::consts::EXE_SUFFIX));
                std::fs::hard_link(&profile.program, &path).unwrap();
                profile.program = path;
            }
            "missing" => {
                h.actor.settings.engine.as_mut().unwrap().human_model =
                    Some(h.directory.path().join("missing.bin.gz"))
            }
            "corrupt" => {
                let path = h.directory.path().join("corrupt.bin.gz");
                std::fs::write(&path, b"wrong-model").unwrap();
                h.actor.settings.engine.as_mut().unwrap().human_model = Some(path);
            }
            _ => unreachable!(),
        }
        let saved = std::fs::read(h.directory.path().join("settings.json")).unwrap();
        let result = start(&mut h.actor, setup(CoachMode::ReviewAfter)).await;
        assert!(
            matches!(
                (&result, case),
                (
                    Err(AppError::Coach(CoachError::UnsupportedVersion(_))),
                    "old"
                ) | (
                    Err(AppError::Coach(CoachError::TensorRtUnsupported)),
                    "tensorrt"
                ) | (Err(AppError::Coach(CoachError::ModelMissing)), "missing")
                    | (Err(AppError::Coach(CoachError::ModelCorrupt)), "corrupt")
            ),
            "{case}: {result:?}"
        );
        assert!(h.actor.coach_view().is_none());
        assert!(h.actor.game_view().is_none());
        assert_eq!(
            std::fs::read(h.directory.path().join("settings.json")).unwrap(),
            saved
        );
        let prefix = requests_before(&h.actor, h.directory.path()).await;
        assert_eq!(
            prefix
                .iter()
                .filter(|r| r["action"] == "query_version")
                .count(),
            1
        );
        assert!(
            !prefix
                .iter()
                .any(|r| r["overrideSettings"]["humanSLProfile"].is_string())
        );
        drop(version_sender);
        close(h).await;
    }
}

#[tokio::test]
async fn unknown_backend_names_load_human_model_and_start_coaching() {
    for name in [
        format!("homebrew/bin/katago{}", std::env::consts::EXE_SUFFIX),
        "imported/katago.exe".into(),
        format!("custom/katago{}", std::env::consts::EXE_SUFFIX),
    ] {
        let mut h = harness(feedback_script(), false).await;
        let mut profile = h.actor.settings.engine.clone().unwrap();
        let path = h.directory.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::hard_link(&profile.program, &path)
            .or_else(|_| std::fs::copy(&profile.program, &path).map(|_| ()))
            .unwrap();
        profile.program = path;
        assert_eq!(
            crate::assets::human_model_backend(&profile).await,
            crate::assets::HumanModelBackend::Unknown
        );
        within(h.actor.configure_engine(Some(profile)))
            .await
            .unwrap();
        ready(h.actor.running.as_ref().unwrap().engine.status()).await;
        let view = start(&mut h.actor, setup(CoachMode::LiveFeedback))
            .await
            .unwrap();
        assert!(view.active);
        assert!(
            h.actor
                .settings
                .engine
                .as_ref()
                .unwrap()
                .human_model
                .is_some()
        );
        query(h.directory.path(), |r| {
            r["overrideSettings"]["humanSLProfile"] == "rank_3d"
        })
        .await;
        close(h).await;
    }
}

#[tokio::test]
async fn invalid_current_start_and_failed_save_do_not_remember_the_requested_setup() {
    let mut h = harness(json!({}), true).await;
    let mut options = setup(CoachMode::ReviewAfter);
    options.from_current_position = true;
    options.rank = CoachRank::Kyu(8);
    let previous = h.actor.settings.coach_setup.clone();
    assert!(matches!(
        start(&mut h.actor, options.clone()).await,
        Err(AppError::Coach(CoachError::InvalidSetup(_)))
    ));
    assert_eq!(h.actor.settings.coach_setup, previous);
    options.from_current_position = false;
    let blocked = h.directory.path().join("not-a-directory");
    std::fs::write(&blocked, b"blocked").unwrap();
    h.actor.directory = blocked;
    assert!(start(&mut h.actor, options).await.is_err());
    assert_eq!(h.actor.settings.coach_setup, previous);
    assert_eq!(
        crate::Settings::load(h.directory.path())
            .await
            .unwrap()
            .coach_setup,
        previous
    );
    close(h).await;
}

#[tokio::test]
async fn sparse_user_candidate_requests_bounded_post_move_review() {
    let mut script = feedback_script();
    script["profiles"]["rank_3d"]["turns"]["0"]["candidates"][2]["visitShare"] = json!(0.001);
    script["profiles"]["rank_3d"]["turns"]["1"]["scoreLead"] = json!(-25.0);
    let mut h = harness(script, true).await;
    start(&mut h.actor, setup(CoachMode::LiveFeedback))
        .await
        .unwrap();
    cached(&mut h.actor).await;
    h.actor
        .apply(crate::actor::Input::Edit(document::Command::Play(
            Move::Play(Color::Black, go_core::Point { x: 2, y: 0 }),
        )))
        .await
        .unwrap();
    drive(&mut h.actor, |actor| {
        actor.coach_view().unwrap().last_rating.is_some()
    })
    .await;
    let request = query(h.directory.path(), |r| r["maxVisits"] == 200).await;
    assert_eq!(request["overrideSettings"]["maxTime"], 0.8);
    assert_eq!(request["overrideSettings"]["humanSLProfile"], "rank_3d");
    assert_eq!(request["moves"].as_array().unwrap().len(), 1);
    let rating = h.actor.coach_view().unwrap().last_rating.unwrap();
    assert_eq!(rating.points_lost, 25.0);
    assert_eq!(rating.verdict, CoachVerdict::Blunder);
    assert!(h.actor.coach_view().unwrap().awaiting_decision);
    close(h).await;
}

#[tokio::test(start_paused = true)]
async fn gated_rating_never_holds_a_prepared_reply_beyond_four_seconds() {
    tokio::time::resume();
    let mut script = feedback_script();
    // A nonexistent gate remains closed throughout the test. Only the before
    // query is gated; the AI policy at turn 1 is independent and can complete.
    script["profiles"]["rank_3d"]["turns"]["0"]["hang"] = json!(true);
    let mut h = harness(script, true).await;
    start(&mut h.actor, setup(CoachMode::LiveFeedback))
        .await
        .unwrap();
    h.actor
        .apply(crate::actor::Input::Edit(document::Command::Play(
            Move::Play(Color::Black, go_core::Point { x: 2, y: 0 }),
        )))
        .await
        .unwrap();
    selected(&mut h.actor).await;
    assert!(h.actor.coach.session.as_ref().unwrap().rating.is_some());
    let deadline = h
        .actor
        .coach
        .session
        .as_ref()
        .unwrap()
        .rating
        .as_ref()
        .unwrap()
        .deadline;
    tokio::time::advance(
        deadline.saturating_duration_since(Instant::now()) - Duration::from_millis(1),
    )
    .await;
    h.actor.coach_tick().await.unwrap();
    assert_eq!(h.actor.game_view().unwrap().move_number, 1);
    tokio::time::advance(Duration::from_millis(1)).await;
    h.actor.coach_tick().await.unwrap();
    assert_eq!(h.actor.game_view().unwrap().move_number, 2);
    assert!(!h.actor.coach_view().unwrap().awaiting_decision);
    assert!(h.actor.coach_view().unwrap().last_rating.is_none());
    close(h).await;
}
