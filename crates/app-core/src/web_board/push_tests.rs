use super::*;

#[tokio::test]
async fn initial_state_and_host_navigation_use_host_views_and_history_stops_at_current_node() {
    let mut h = Harness::new().await;
    h.open(b"(;SZ[9]KM[6.5]RU[japanese]C[private comment];B[aa];W[bb](;B[cc])(;B[dd]))")
        .await;
    let revision = within(h.app.step(1)).await.unwrap();
    let custom_rank = crate::MoveRankSettings {
        winrate_thresholds: [10.0, 20.0, 30.0, 40.0, 50.0],
        ..crate::MoveRankSettings::default()
    };
    within(h.app.set_move_rank_settings(custom_rank))
        .await
        .unwrap();
    let host = host_view(&mut h.events, revision).await;
    let (mut socket, initial) = h.connect(None).await;
    assert_eq!(initial.hello.komi, 6.5);
    assert_eq!(initial.hello.rules, "japanese");
    assert!(initial.hello.allow_trial);
    assert!(!initial.hello.client_id.is_empty());
    assert_eq!(initial.hello.reconnect.len(), 22);
    assert_eq!(initial.full.board, host.board);
    assert_eq!(initial.full.node_id, host.tree.current);
    assert_eq!(initial.full.revision, revision);
    assert_eq!(
        initial.history.points,
        host.graph.points[..=host.graph.current_index]
    );
    assert!(initial.history.points.iter().all(|p| p.summary.is_none()));
    assert_eq!(initial.history.current_index, host.graph.current_index);
    assert!(initial.problems.entries.is_empty());
    assert_eq!(
        initial.problems.settings,
        crate::MoveRankSettings::default()
    );
    assert!(!initial.trial.active);
    let encoded = serde_json::to_string(&WebServerMessage::FullState(initial.full)).unwrap();
    assert!(!encoded.contains("private comment"));

    let revision = within(h.app.step(1)).await.unwrap();
    let host = host_view(&mut h.events, revision).await;
    let updated = full_where(&mut socket, |s| s.revision == revision).await;
    assert_eq!(updated.board, host.board);
    assert_eq!(updated.node_id, host.tree.current);
    let revision = within(
        h.app
            .open_bytes(b"(;SZ[5]KM[0.5]RU[chinese];B[cc])".to_vec()),
    )
    .await
    .unwrap();
    let host = host_view(&mut h.events, revision).await;
    let replacement = full_where(&mut socket, |s| s.revision == revision).await;
    assert_eq!(replacement.board, host.board);
    assert_eq!(replacement.board.unwrap().width, 5);
    assert!(replacement.analysis.is_none());
    let (_second, new_initial) = h.connect(None).await;
    assert_eq!(new_initial.hello.komi, 0.5);
    assert_eq!(new_initial.hello.rules, "chinese");
    h.shutdown().await;
}

#[tokio::test]
async fn real_fake_engine_analysis_is_black_perspective_throttled_and_cleared_on_engine_change() {
    let program = fake_katago::binary_path().unwrap();
    let mut config = configuration(TestConfig::default());
    config.analysis.live.max_visits = 500;
    config.analysis.live.report_interval = Duration::from_millis(10);
    let mut h = Harness::configured(config).await;
    let original = h.open(b"(;SZ[9]PL[W];W[aa])").await;
    let (mut socket, _) = h.connect(None).await;
    let profile = crate::EngineProfile {
        program: program.clone(),
        model: program.clone(),
        config: program,
        remote: None,
        human_model: None,
        canonical_model_name: None,
        extra_args: vec![
            "--visits-per-sec".into(),
            "1000".into(),
            "--default-perspective-to-play".into(),
        ],
    };
    within(h.app.configure_engine(Some(profile))).await.unwrap();
    let (frames, final_analysis) = within(async {
        let mut frames = Vec::new();
        let mut final_analysis = None;
        let mut history_done = false;
        loop {
            match receive(&mut socket).await {
                WebServerMessage::AnalysisUpdate(update) => {
                    assert_eq!(update.node_id, original.tree.current);
                    if let Some(analysis) = update.analysis {
                        assert_eq!(analysis.node_id, original.tree.current);
                        frames.push(analysis.visits);
                        if !analysis.is_during_search {
                            final_analysis = Some(analysis);
                        }
                    }
                }
                WebServerMessage::WinrateHistory(history) => {
                    history_done = history
                        .points
                        .first()
                        .and_then(|p| p.summary)
                        .is_some_and(|s| s.visits == 500);
                }
                _ => {}
            }
            if history_done && let Some(final_analysis) = final_analysis.take() {
                return (frames, final_analysis);
            }
        }
    })
    .await;
    assert!(
        (1..=7).contains(&frames.len()),
        "500 visits at 1000 visits/s exceeded the throttled frame budget: {frames:?}"
    );
    for pair in frames.windows(2) {
        assert!(pair[1] >= pair[0]);
    }
    let host_analysis = within(async {
        loop {
            if let AppEvent::AnalysisUpdated(view) = h.events.recv().await.unwrap()
                && view.node_id == original.tree.current
                && view.visits == 500
            {
                return view;
            }
        }
    })
    .await;
    assert_eq!(final_analysis, host_analysis);
    assert_eq!(final_analysis.visits, 500);
    assert_eq!(final_analysis.ownership.as_ref().unwrap().len(), 81);

    let revision = within(h.app.step(1)).await.unwrap();
    let changed = full_where(&mut socket, |s| s.revision == revision).await;
    assert_eq!(changed.board.as_ref().unwrap().move_number, 1);
    let next = message_where(&mut socket, |m| matches!(m, WebServerMessage::AnalysisUpdate(u) if u.analysis.as_ref().is_some_and(|a| !a.is_during_search))).await;
    if let WebServerMessage::AnalysisUpdate(update) = next {
        assert_eq!(update.revision, revision);
        assert_eq!(update.node_id, changed.node_id);
    }
    within(h.app.configure_engine(None)).await.unwrap();
    message_where(&mut socket, |m| match m {
        WebServerMessage::FullState(s) => s.revision == revision && s.analysis.is_none(),
        WebServerMessage::AnalysisUpdate(s) => s.revision == revision && s.analysis.is_none(),
        _ => false,
    })
    .await;
    h.shutdown().await;
}
