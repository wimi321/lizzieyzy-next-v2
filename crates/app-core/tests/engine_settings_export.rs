#![allow(clippy::unwrap_used, clippy::expect_used)]

use app_core::{AnalysisSource, AppConfig, AppEvent, EngineProfile, EngineRules, EngineSettings};
use std::{future::Future, time::Duration};

const JAVA: &[u8] = include_bytes!("../../../fixtures/lz/java-analysis.sgf");

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("safety deadline")
}

#[tokio::test]
async fn overridden_results_are_not_exported_as_original_game_lz_or_revived_after_reopen() {
    let binary = fake_katago::binary_path().unwrap();
    for options in [
        EngineSettings {
            rules: EngineRules::Japanese,
            ..EngineSettings::default()
        },
        EngineSettings {
            read_komi: false,
            default_komi: 6.5,
            ..EngineSettings::default()
        },
    ] {
        let directory = tempfile::tempdir().unwrap();
        let mut config = AppConfig::default();
        config.analysis.live.max_visits = 1;
        config.session.shutdown_timeout = Duration::from_secs(60);
        config.session.write_timeout = Duration::from_secs(60);
        let (core, mut output) = within(app_core::spawn_with_config(directory.path(), config))
            .await
            .unwrap();
        within(core.set_auto_quick_analyze_on_load(false))
            .await
            .unwrap();
        within(core.open_bytes(JAVA.to_vec())).await.unwrap();
        let original_root = within(core.whole_game_analysis(0)).await.unwrap().unwrap();
        let untouched_child = within(core.whole_game_analysis(1)).await.unwrap().unwrap();
        assert!(original_root.saved.is_some());
        within(core.set_engine_settings(options)).await.unwrap();
        // Overrides alone must not discard unmodified file analysis.
        let original_path = directory.path().join("original-only.sgf");
        within(core.save(&original_path)).await.unwrap();
        let original = sgf::parse(JAVA).unwrap();
        assert_eq!(
            sgf::parse(&tokio::fs::read(original_path).await.unwrap())
                .unwrap()
                .games,
            original.games
        );
        within(core.configure_engine(Some(EngineProfile {
            program: binary.clone(),
            model: binary.clone(),
            config: binary.clone(),
            ..EngineProfile::default()
        })))
        .await
        .unwrap();
        within(async {
            loop {
                match output.recv().await.unwrap() {
                    AppEvent::AnalysisUpdated(view)
                        if view.source == AnalysisSource::Live && !view.is_during_search =>
                    {
                        assert_eq!(view.node_id, 0);
                        break;
                    }
                    AppEvent::EngineStatusChanged(app_core::EngineStatus::Failed {
                        reason,
                        ..
                    }) => panic!("{reason}"),
                    _ => {}
                }
            }
        })
        .await;
        let analyzed = within(core.whole_game_analysis(0)).await.unwrap().unwrap();
        assert!(analyzed.saved.is_none());
        assert_eq!(analyzed.response.root_info.visits, 1);
        let path = directory.path().join("overridden.sgf");
        within(core.save(&path)).await.unwrap();
        let collection = sgf::parse(&tokio::fs::read(&path).await.unwrap()).unwrap();
        let restored = document::GameDocument::from_sgf(&collection.games[0]).unwrap();
        // Neither the override result nor the replaced deep original primary
        // slot may be reintroduced under the unchanged SGF metadata.
        assert!(
            restored
                .saved_analysis(document::NodeId(0))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            restored.saved_analysis(document::NodeId(1)).unwrap(),
            untouched_child.saved
        );
        assert_eq!(restored.game_info().unwrap().komi, 7.5);
        assert_eq!(restored.game_info().unwrap().rules_preset, "chinese");
        let secondary = |tree: &sgf::GameTree| {
            tree.sequence[0]
                .properties
                .iter()
                .filter(|p| p.identifier == "LZ2")
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(
            secondary(&collection.games[0]),
            secondary(&original.games[0])
        );
        within(core.shutdown()).await.unwrap();
        let (reader, _events) = within(app_core::spawn(directory.path().join("reader")))
            .await
            .unwrap();
        within(reader.open_path(path)).await.unwrap();
        assert_eq!(
            within(reader.settings()).await.unwrap().engine_settings,
            EngineSettings::default()
        );
        assert!(
            within(reader.whole_game_analysis(0))
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            within(reader.whole_game_analysis(1))
                .await
                .unwrap()
                .unwrap()
                .saved
                .is_some()
        );
        within(reader.shutdown()).await.unwrap();
    }
}
