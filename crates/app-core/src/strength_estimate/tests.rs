#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{
    AppConfig, AppEvent, EngineProfile, EngineStatus, Settings, WholeGameOptions, WholeGameStatus,
};
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
    // File creation and fake binary construction are setup, outside all safety
    // deadlines. This integration exercises engine delivery, not SQLite I/O.
    Settings {
        engine: Some(EngineProfile {
            remote: None,
            human_model: None,
            canonical_model_name: None,
            model: program.clone(),
            config: program.clone(),
            program,
            extra_args: Vec::new(),
        }),
        pondering: false,
        auto_quick_analyze_on_load: false,
        cache_enabled: false,
        ..Settings::default()
    }
    .save(directory.path())
    .await
    .unwrap();
    let mut config = AppConfig::default();
    config.session.write_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(60);
    let (app, mut events) = crate::spawn_with_config(directory.path(), config)
        .await
        .unwrap();
    // Thirty-six passes give six samples per color in each custom phase.
    // Full 60/150 default-boundary coverage lives in the synchronous actor test.
    let sgf = format!("(;SZ[9]{})", ";B[];W[]".repeat(18));
    let revision = within(app.open_bytes(sgf.into_bytes())).await.unwrap();
    let empty = within(app.strength_estimate(None)).await.unwrap();
    assert_eq!((empty.total_moves, empty.analyzed_moves), (36, 0));
    assert!(!empty.report.has_enough_data);
    assert_eq!(empty.phases.bounds, [60, 150]);
    for (index, phase) in empty.phases.phases.iter().enumerate() {
        assert_eq!(
            phase.played,
            index == 0,
            "played is independent of analysis coverage"
        );
        assert!(!phase.black_ranked && !phase.white_ranked);
        assert_eq!((phase.black.sample_count, phase.white.sample_count), (0, 0));
    }

    // A separate Ready barrier keeps process startup out of the completion
    // deadline; no engine fault or wall-clock performance is under test.
    within(async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::EngineStatusChanged(EngineStatus::Ready { .. }) => break,
                AppEvent::EngineStatusChanged(EngineStatus::Failed { reason, .. }) => {
                    panic!("{reason}")
                }
                _ => {}
            }
        }
    })
    .await;
    within(app.start_whole_game_analysis(WholeGameOptions {
        visits: 1,
        ..Default::default()
    }))
    .await
    .unwrap();
    within(async {
        loop {
            match events.recv().await.unwrap() {
                AppEvent::WholeGameProgress(progress)
                    if progress.status == WholeGameStatus::Completed =>
                {
                    assert_eq!(
                        (progress.visits, progress.completed, progress.total),
                        (1, 37, 37)
                    );
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
    assert_eq!((report.total_moves, report.analyzed_moves), (36, 36));
    assert_eq!(report.report.black.sample_count, 18);
    assert_eq!(report.report.white.sample_count, 18);
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
            .zip([(1, 36, 18), (61, 36, 0), (151, 36, 0)])
    {
        assert_eq!((phase.first, phase.last), (first, last));
        assert_eq!(phase.played, count != 0);
        assert_eq!(phase.black_ranked, count >= 6);
        assert_eq!(phase.white_ranked, count >= 6);
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
    let custom = within(app.strength_estimate(Some([12, 24]))).await.unwrap();
    assert_eq!(custom.phases.bounds, [12, 24]);
    assert_eq!(
        custom.report, report.report,
        "custom cuts must not alter the whole-game report"
    );
    assert_eq!(
        (custom.revision, custom.total_moves, custom.analyzed_moves),
        (revision, 36, 36)
    );
    for (phase, (first, last, count)) in
        custom
            .phases
            .phases
            .iter()
            .zip([(1, 12, 6), (13, 24, 6), (25, 36, 6)])
    {
        assert_eq!((phase.first, phase.last), (first, last));
        assert_eq!(
            (phase.black.sample_count, phase.white.sample_count),
            (count, count)
        );
        assert!(phase.played && phase.black_ranked && phase.white_ranked);
    }
    let normalized = within(app.strength_estimate(Some([18, 17]))).await.unwrap();
    assert_eq!(normalized.phases.bounds, [18, 28]);
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
        (36, 0)
    );
    assert!(!invalidated.report.has_enough_data);
    for (index, phase) in invalidated.phases.phases.iter().enumerate() {
        assert_eq!(phase.played, index == 0);
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
