use super::*;
use go_core::{Color, Move, Point};

const BRANCHED: &[u8] = b"(;SZ[9]KM[6.5]RU[japanese]PB[original];B[aa];W[bb](;B[cc])(;B[dd]))";

async fn denied(socket: &mut Socket, reason: WebTrialDeniedReason) {
    assert_eq!(
        message_where(socket, |m| matches!(
            m,
            WebServerMessage::TrialDenied { .. }
        ))
        .await,
        WebServerMessage::TrialDenied { reason }
    );
}

#[tokio::test]
async fn spectator_trial_branches_navigation_and_exit_restore_original_document_and_history() {
    let mut h = Harness::new().await;
    h.open(BRANCHED).await;
    within(h.app.step(2)).await.unwrap();
    within(h.app.set_comment("kept comment".into()))
        .await
        .unwrap();
    let path = h.directory.path().join("original.sgf");
    within(h.app.save(&path)).await.unwrap();
    within(h.app.play(Move::Play(Color::Black, Point { x: 4, y: 4 })))
        .await
        .unwrap();
    let revision = within(h.app.undo()).await.unwrap();
    let original = host_view(&mut h.events, revision).await;
    let original_tree = within(h.app.variation_tree()).await.unwrap();
    let original_bytes = std::fs::read(&path).unwrap();
    assert!(original.can_undo && original.can_redo);

    let (mut owner, initial) = h.connect(None).await;
    let (mut other, other_initial) = h.connect(None).await;
    assert_ne!(initial.hello.client_id, other_initial.hello.client_id);
    send(&mut owner, WebClientMessage::EnterTrial).await;
    let state = trial_where(&mut owner, |s| s.active).await;
    assert_eq!(state.owner.as_ref(), Some(&initial.hello.client_id));
    assert_eq!((state.anchor_move, state.move_number), (2, 2));
    assert!(!state.can_back && !state.can_forward);
    let status = within(h.app.web_board_status()).await.unwrap();
    assert_eq!(
        status.trial.as_ref().unwrap().owner,
        initial.hello.client_id
    );
    assert_eq!(status.viewers, 2);
    send(&mut other, WebClientMessage::EnterTrial).await;
    denied(&mut other, WebTrialDeniedReason::InUse).await;
    within(other.send(Message::text(serde_json::json!({
        "type": "TrialMove", "data": { "x": 6, "y": 6, "client_id": initial.hello.client_id }
    }).to_string()))).await.unwrap();
    denied(&mut other, WebTrialDeniedReason::InUse).await;

    // Every host mutation goes through the same actor guard. Queries, display
    // preferences and the pondering switch remain useful during the trial.
    for result in [
        within(h.app.step(-1)).await,
        within(h.app.go_to(0)).await,
        within(h.app.play(Move::Play(Color::Black, Point { x: 8, y: 8 }))).await,
        within(h.app.set_comment("must not overwrite".into())).await,
        within(h.app.undo()).await,
        within(h.app.redo()).await,
        within(h.app.start_trial()).await,
        within(h.app.end_trial()).await,
        within(h.app.open_bytes(b"(;SZ[5])".to_vec())).await,
        within(h.app.new_game(crate::NewGameOptions::default())).await,
        within(
            h.app
                .set_setup_stone(Point { x: 0, y: 0 }, Some(Color::Black)),
        )
        .await,
    ] {
        assert!(
            matches!(result, Err(crate::AppError::WebBoard(_))),
            "{result:?}"
        );
    }
    assert!(within(h.app.start_scoring()).await.is_err());
    assert!(within(h.app.save(&path)).await.is_err());
    assert!(within(h.app.configure_engine(None)).await.is_err());
    let settings = within(h.app.settings()).await.unwrap();
    within(h.app.set_display_settings(settings.display))
        .await
        .unwrap();
    within(h.app.set_pondering(false)).await.unwrap();
    assert!(within(h.app.variation_tree()).await.is_ok());

    send(&mut owner, WebClientMessage::TrialMove { x: 2, y: 2 }).await;
    trial_where(&mut owner, |s| s.move_number == 3).await;
    send(&mut owner, WebClientMessage::TrialBack).await;
    trial_where(&mut owner, |s| s.move_number == 2 && s.branches.len() == 1).await;
    send(&mut owner, WebClientMessage::TrialMove { x: 3, y: 3 }).await;
    trial_where(&mut owner, |s| s.move_number == 3).await;
    send(&mut owner, WebClientMessage::TrialBack).await;
    let branching = trial_where(&mut owner, |s| s.move_number == 2 && s.branches.len() == 2).await;
    assert_eq!(
        branching
            .branches
            .iter()
            .map(|b| (b.x, b.y, b.index))
            .collect::<Vec<_>>(),
        vec![(Some(2), Some(2), 0), (Some(3), Some(3), 1)]
    );
    assert!(branching.branches.iter().all(|b| !b.label.is_empty()));
    send(&mut owner, WebClientMessage::TrialChild { index: 1 }).await;
    trial_where(&mut owner, |s| s.move_number == 3).await;
    let selected = within(h.app.variation_tree()).await.unwrap();
    assert!(
        selected
            .nodes
            .iter()
            .any(|n| n.mv == Some(Move::Play(Color::Black, Point { x: 3, y: 3 }).into()))
    );
    send(&mut owner, WebClientMessage::TrialToAnchor).await;
    trial_where(&mut owner, |s| s.move_number == 2).await;
    send(&mut owner, WebClientMessage::TrialForward).await;
    trial_where(&mut owner, |s| s.move_number == 3).await;
    send(&mut owner, WebClientMessage::ExitTrial).await;
    trial_where(&mut owner, |s| !s.active).await;
    let revision = within(h.app.step(0)).await.unwrap();
    let restored = host_view(&mut h.events, revision).await;
    assert_eq!(restored.board, original.board);
    assert_eq!(restored.tree.current, original.tree.current);
    assert_eq!(restored.comment, original.comment);
    assert_eq!(restored.file_path, original.file_path);
    assert_eq!(restored.file_name, original.file_name);
    assert_eq!(restored.modified, original.modified);
    assert_eq!((restored.can_undo, restored.can_redo), (true, true));
    assert!(!restored.trial);
    assert_eq!(
        within(h.app.variation_tree()).await.unwrap().nodes,
        original_tree.nodes
    );
    assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
    let revision = within(h.app.redo()).await.unwrap();
    assert_eq!(
        host_view(&mut h.events, revision)
            .await
            .board
            .unwrap()
            .stones[40],
        Some(crate::ColorView::Black)
    );
    h.shutdown().await;
}

#[tokio::test]
async fn illegal_trial_moves_including_ko_keep_connection_and_document_intact() {
    let mut h = Harness::new().await;
    h.open(b"(;SZ[5]AB[ab][ba][cb]AW[bb][ac][cc][bd]PL[B];B[bc])")
        .await;
    within(h.app.step(1)).await.unwrap();
    let original = within(h.app.variation_tree()).await.unwrap();
    let (mut socket, _) = h.connect(None).await;
    send(&mut socket, WebClientMessage::EnterTrial).await;
    trial_where(&mut socket, |s| s.active).await;
    let trial = within(h.app.variation_tree()).await.unwrap();
    // Recapture ko, occupied, outside board, and single-stone suicide.
    for (x, y) in [(1, 1), (1, 2), (5, 5), (0, 0)] {
        send(&mut socket, WebClientMessage::TrialMove { x, y }).await;
        message_where(&mut socket, |m| {
            matches!(m, WebServerMessage::TrialRejected { .. })
        })
        .await;
        assert_eq!(
            within(h.app.variation_tree()).await.unwrap().nodes,
            trial.nodes
        );
    }
    send(
        &mut socket,
        WebClientMessage::TrialChild { index: usize::MAX },
    )
    .await;
    message_where(&mut socket, |m| {
        matches!(m, WebServerMessage::TrialRejected { .. })
    })
    .await;
    assert_eq!(
        within(h.app.variation_tree()).await.unwrap().nodes,
        trial.nodes
    );
    send(&mut socket, WebClientMessage::TrialMove { x: 4, y: 4 }).await;
    trial_where(&mut socket, |s| s.move_number == 2).await;
    send(&mut socket, WebClientMessage::ExitTrial).await;
    trial_where(&mut socket, |s| !s.active).await;
    assert_eq!(
        within(h.app.variation_tree()).await.unwrap().nodes,
        original.nodes
    );
    h.shutdown().await;
}

#[tokio::test]
async fn trial_denied_when_disabled_or_host_trial_or_manual_scoring_is_active() {
    let mut h = Harness::new().await;
    h.open(BRANCHED).await;
    let (mut socket, _) = h.connect(None).await;
    within(
        h.app
            .set_web_board_options(WebBoardOptions { allow_trial: false }),
    )
    .await
    .unwrap();
    send(&mut socket, WebClientMessage::EnterTrial).await;
    denied(&mut socket, WebTrialDeniedReason::Disabled).await;
    within(
        h.app
            .set_web_board_options(WebBoardOptions { allow_trial: true }),
    )
    .await
    .unwrap();
    within(h.app.start_trial()).await.unwrap();
    send(&mut socket, WebClientMessage::EnterTrial).await;
    denied(&mut socket, WebTrialDeniedReason::HostBusy).await;
    within(h.app.end_trial()).await.unwrap();
    within(h.app.start_scoring()).await.unwrap();
    send(&mut socket, WebClientMessage::EnterTrial).await;
    denied(&mut socket, WebTrialDeniedReason::HostBusy).await;
    within(h.app.cancel_scoring()).await.unwrap();
    send(&mut socket, WebClientMessage::EnterTrial).await;
    trial_where(&mut socket, |s| s.active).await;
    within(
        h.app
            .set_web_board_options(WebBoardOptions { allow_trial: false }),
    )
    .await
    .unwrap();
    trial_where(&mut socket, |s| !s.active).await;
    assert!(
        within(h.app.web_board_status())
            .await
            .unwrap()
            .trial
            .is_none()
    );
    h.shutdown().await;
}

#[tokio::test]
async fn playing_and_paused_host_games_reject_spectator_trials_until_stopped() {
    let h = Harness::new().await;
    let (mut socket, _) = h.connect(None).await;
    let setup = crate::GameSetup {
        black: crate::Player::Human,
        white: crate::Player::Human,
        start: crate::GameStart::CurrentPosition,
        human_clock: None,
        ponder: false,
        auto_save: false,
    };
    assert_eq!(
        within(h.app.start_game(setup)).await.unwrap().phase,
        crate::GamePhase::Playing
    );
    send(&mut socket, WebClientMessage::EnterTrial).await;
    denied(&mut socket, WebTrialDeniedReason::HostBusy).await;
    assert_eq!(
        within(h.app.pause_game()).await.unwrap().phase,
        crate::GamePhase::Paused
    );
    send(&mut socket, WebClientMessage::EnterTrial).await;
    denied(&mut socket, WebTrialDeniedReason::HostBusy).await;
    within(h.app.stop_game()).await.unwrap();
    send(&mut socket, WebClientMessage::EnterTrial).await;
    trial_where(&mut socket, |s| s.active).await;
    h.shutdown().await;
}
