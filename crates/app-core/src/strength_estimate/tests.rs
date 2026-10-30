#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{AppConfig, AppEvent, EngineProfile, WholeGameOptions, WholeGameStatus};
use std::time::Duration;

async fn within<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("strength integration deadline")
}

#[tokio::test]
async fn whole_game_completion_exposes_both_players_and_invalidates_on_new_game_or_engine() {
    let program = fake_katago::binary_path().unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut config = AppConfig::default();
    config.session.write_timeout = Duration::from_secs(60);
    config.session.shutdown_timeout = Duration::from_secs(60);
    let (app, mut events) = crate::spawn_with_config(directory.path(), config)
        .await
        .unwrap();
    within(app.set_pondering(false)).await.unwrap();
    let sgf = b"(;SZ[9];B[aa];W[ba];B[ca];W[da];B[ea];W[fa];B[ga];W[ha];B[ab];W[bb];B[cb];W[db];B[eb];W[fb];B[gb];W[hb])";
    let revision = within(app.open_bytes(sgf.to_vec())).await.unwrap();
    let empty = within(app.strength_estimate()).await.unwrap();
    assert_eq!((empty.total_moves, empty.analyzed_moves), (16, 0));
    assert!(!empty.report.has_enough_data);

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
    let report = within(app.strength_estimate()).await.unwrap();
    assert_eq!(report.revision, revision);
    assert_eq!((report.total_moves, report.analyzed_moves), (16, 16));
    assert_eq!(report.report.black.sample_count, 8);
    assert_eq!(report.report.white.sample_count, 8);
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
    assert_eq!(within(app.strength_estimate()).await.unwrap(), report);
    assert!(within(app.open_bytes(b"not sgf".to_vec())).await.is_err());
    assert_eq!(within(app.strength_estimate()).await.unwrap(), report);

    within(app.configure_engine(None)).await.unwrap();
    let invalidated = within(app.strength_estimate()).await.unwrap();
    assert_eq!(
        (invalidated.total_moves, invalidated.analyzed_moves),
        (16, 0)
    );
    assert!(!invalidated.report.has_enough_data);
    within(app.open_bytes(b"(;SZ[9];B[])".to_vec()))
        .await
        .unwrap();
    let new_game = within(app.strength_estimate()).await.unwrap();
    assert_eq!((new_game.total_moves, new_game.analyzed_moves), (1, 0));
    assert!(new_game.revision > revision);
    within(app.shutdown()).await.unwrap();
}
