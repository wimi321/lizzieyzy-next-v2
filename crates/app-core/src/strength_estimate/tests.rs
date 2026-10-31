#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{AppConfig, AppEvent, EngineProfile, WholeGameOptions, WholeGameStatus};
use std::time::Duration;

async fn within<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("strength integration deadline")
}

#[tokio::test]
async fn whole_game_completion_exposes_default_and_custom_phases_and_invalidates_results() {
    let program = fake_katago::binary_path().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut config = AppConfig::default();
    config.session.write_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(60);
    let (app, mut events) = crate::spawn_with_config(directory.path(), config)
        .await
        .unwrap();
    within(app.set_pondering(false)).await.unwrap();
    // Passes keep the fixture legal through all three default phases without
    // relying on a synthetic engine's move choices or a real GPU.
    let sgf = format!("(;SZ[9]{})", ";B[];W[]".repeat(84));
    let revision = within(app.open_bytes(sgf.into_bytes())).await.unwrap();
    let empty = within(app.strength_estimate(None)).await.unwrap();
    assert_eq!((empty.total_moves, empty.analyzed_moves), (168, 0));
    assert!(!empty.report.has_enough_data);
    assert_eq!(empty.phases.bounds, [60, 150]);
    for phase in &empty.phases.phases {
        assert!(phase.played, "played is independent of analysis coverage");
        assert!(!phase.black_ranked && !phase.white_ranked);
        assert_eq!((phase.black.sample_count, phase.white.sample_count), (0, 0));
    }

    within(app.configure_engine(Some(EngineProfile {
        model: program.clone(),
        config: program.clone(),
        program,
        extra_args: vec!["--visits-per-sec".into(), "1000000".into()],
    })))
    .await
    .unwrap();
    within(app.start_whole_game_analysis(WholeGameOptions { visits: 500 }))
        .await
        .unwrap();
    within(async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::WholeGameProgress(progress)
                    if progress.status == WholeGameStatus::Completed =>
                {
                    break;
                }
                AppEvent::WholeGameProgress(progress)
                    if matches!(progress.status, WholeGameStatus::Error { .. }) =>
                {
                    panic!("{progress:?}")
                }
                _ => {}
            }
        }
    })
    .await;
    let report = within(app.strength_estimate(None)).await.unwrap();
    assert_eq!(report.revision, revision);
    assert_eq!((report.total_moves, report.analyzed_moves), (168, 168));
    assert_eq!(report.report.black.sample_count, 84);
    assert_eq!(report.report.white.sample_count, 84);
    assert!(report.report.has_enough_data);
    assert!(report.report.black.rank_value.is_some());
    assert!(report.report.white.rank_value.is_some());
    assert!(
        report
            .report
            .black
            .features
            .iter()
            .all(|feature| feature.is_finite())
    );
    assert!(
        report
            .report
            .white
            .features
            .iter()
            .all(|feature| feature.is_finite())
    );
    assert_eq!(report.phases.bounds, [60, 150]);
    let estimator = strength::Estimator::bundled().unwrap();
    for (phase, (first, last, count)) in
        report
            .phases
            .phases
            .iter()
            .zip([(1, 60, 30), (61, 150, 45), (151, 168, 9)])
    {
        assert_eq!((phase.first, phase.last), (first, last));
        assert!(phase.played && phase.black_ranked && phase.white_ranked);
        assert_eq!(
            (phase.black.sample_count, phase.white.sample_count),
            (count, count)
        );
        for (phase_side, whole_side) in [
            (&phase.black, &report.report.black),
            (&phase.white, &report.report.white),
        ] {
            let samples: Vec<_> = whole_side
                .samples
                .iter()
                .filter(|sample| (first..=last).contains(&sample.move_number))
                .cloned()
                .collect();
            assert_eq!(*phase_side, estimator.summarize(&samples));
        }
    }
    let tree = within(app.variation_tree()).await.unwrap();
    let progress = within(app.whole_game_progress()).await.unwrap();
    let custom = within(app.strength_estimate(Some([24, 100])))
        .await
        .unwrap();
    assert_eq!(custom.phases.bounds, [24, 100]);
    assert_eq!(
        custom.report, report.report,
        "custom cuts must not alter the whole-game report"
    );
    assert_eq!(
        (custom.revision, custom.total_moves, custom.analyzed_moves),
        (revision, 168, 168)
    );
    for (phase, (first, last, count)) in
        custom
            .phases
            .phases
            .iter()
            .zip([(1, 24, 12), (25, 100, 38), (101, 168, 34)])
    {
        assert_eq!((phase.first, phase.last), (first, last));
        assert_eq!(
            (phase.black.sample_count, phase.white.sample_count),
            (count, count)
        );
        assert!(phase.played && phase.black_ranked && phase.white_ranked);
    }
    let normalized = within(app.strength_estimate(Some([80, 75]))).await.unwrap();
    assert_eq!(normalized.phases.bounds, [80, 90]);
    assert_eq!(normalized.report, report.report);
    assert_eq!(within(app.variation_tree()).await.unwrap(), tree);
    assert_eq!(within(app.whole_game_progress()).await.unwrap(), progress);
    assert_eq!(within(app.strength_estimate(None)).await.unwrap(), report);
    assert!(within(app.open_bytes(b"not sgf".to_vec())).await.is_err());
    assert_eq!(within(app.strength_estimate(None)).await.unwrap(), report);

    within(app.configure_engine(None)).await.unwrap();
    let invalidated = within(app.strength_estimate(None)).await.unwrap();
    assert_eq!(
        (invalidated.total_moves, invalidated.analyzed_moves),
        (168, 0)
    );
    assert!(!invalidated.report.has_enough_data);
    for phase in &invalidated.phases.phases {
        assert!(phase.played);
        assert!(!phase.black_ranked && !phase.white_ranked);
        assert_eq!((phase.black.sample_count, phase.white.sample_count), (0, 0));
    }
    assert_eq!(invalidated.phases.black.best, None);
    assert_eq!(invalidated.phases.white.worst, None);
    assert!(!invalidated.phases.black.worth_reviewing);
    assert!(!invalidated.phases.white.worth_reviewing);
    within(app.open_bytes(b"(;SZ[9];B[])".to_vec()))
        .await
        .unwrap();
    let new_game = within(app.strength_estimate(None)).await.unwrap();
    assert_eq!((new_game.total_moves, new_game.analyzed_moves), (1, 0));
    assert_eq!(new_game.phases.bounds, [60, 150]);
    assert_eq!(
        (
            new_game.phases.phases[0].first,
            new_game.phases.phases[0].last
        ),
        (1, 1)
    );
    assert!(new_game.phases.phases[0].played);
    assert!(!new_game.phases.phases[1].played);
    assert!(!new_game.phases.phases[2].played);
    assert!(new_game.revision > revision);
    within(app.shutdown()).await.unwrap();
}
