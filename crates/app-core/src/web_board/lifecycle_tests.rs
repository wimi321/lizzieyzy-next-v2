use super::*;

#[tokio::test]
async fn reconnect_atomically_replaces_a_still_connected_owner_without_losing_trial() {
    let h = Harness::new().await;
    let (mut original, initial) = h.connect(None).await;
    send(&mut original, WebClientMessage::EnterTrial).await;
    trial_where(&mut original, |s| s.active).await;
    send(&mut original, WebClientMessage::TrialMove { x: 3, y: 3 }).await;
    trial_where(&mut original, |s| s.move_number == 1).await;

    // The old TCP connection remains alive: the credential must replace its
    // registration atomically, without waiting for a Disconnected notification.
    let (mut replacement, resumed) = h.connect(Some(&initial.hello.reconnect)).await;
    assert_eq!(resumed.hello.client_id, initial.hello.client_id);
    assert_eq!(resumed.hello.reconnect, initial.hello.reconnect);
    assert_eq!(resumed.trial.owner.as_ref(), Some(&initial.hello.client_id));
    assert_eq!(resumed.trial.move_number, 1);
    closed(&mut original).await;
    let status = within(h.app.web_board_status()).await.unwrap();
    assert_eq!(status.viewers, 1);
    assert_eq!(status.trial.unwrap().owner, initial.hello.client_id);

    send(&mut replacement, WebClientMessage::TrialMove { x: 4, y: 4 }).await;
    trial_where(&mut replacement, |s| s.move_number == 2).await;
    let status = within(h.app.web_board_status()).await.unwrap();
    assert_eq!(status.viewers, 1);
    assert_eq!(status.trial.unwrap().owner, initial.hello.client_id);
    h.shutdown().await;
}

#[tokio::test]
async fn reconnect_credential_recovers_owner_only_during_grace_and_expires_without_sleeps() {
    let config = TestConfig {
        reconnect_timeout: Duration::from_millis(500),
        ..TestConfig::default()
    };
    let mut h = Harness::configured(configuration(config)).await;
    let original = within(h.app.variation_tree()).await.unwrap();
    let (mut socket, initial) = h.connect(None).await;
    send(&mut socket, WebClientMessage::EnterTrial).await;
    trial_where(&mut socket, |s| s.active).await;
    send(&mut socket, WebClientMessage::TrialMove { x: 3, y: 3 }).await;
    trial_where(&mut socket, |s| s.move_number == 1).await;
    drop(socket);
    web_status(&mut h.events, |s| s.viewers == 0 && s.trial.is_some()).await;
    let (mut restored, resumed) = h.connect(Some(&initial.hello.reconnect)).await;
    assert_eq!(resumed.hello.client_id, initial.hello.client_id);
    assert_eq!(resumed.trial.owner.as_ref(), Some(&initial.hello.client_id));
    assert_eq!(resumed.trial.move_number, 1);
    send(&mut restored, WebClientMessage::TrialMove { x: 4, y: 4 }).await;
    trial_where(&mut restored, |s| s.move_number == 2).await;

    let (mut forged, fake) = h.connect(Some("AAAAAAAAAAAAAAAAAAAAAA")).await;
    assert_ne!(fake.hello.client_id, initial.hello.client_id);
    send(&mut forged, WebClientMessage::EnterTrial).await;
    assert_eq!(
        message_where(&mut forged, |m| matches!(
            m,
            WebServerMessage::TrialDenied { .. }
        ))
        .await,
        WebServerMessage::TrialDenied {
            reason: WebTrialDeniedReason::InUse
        }
    );
    drop(forged);
    drop(restored);
    web_status(&mut h.events, |s| s.viewers == 0 && s.trial.is_some()).await;
    web_status(&mut h.events, |s| s.viewers == 0 && s.trial.is_none()).await;
    assert_eq!(
        within(h.app.variation_tree()).await.unwrap().nodes,
        original.nodes
    );
    let (_expired, fresh) = h.connect(Some(&initial.hello.reconnect)).await;
    assert_ne!(fresh.hello.client_id, initial.hello.client_id);
    assert!(!fresh.trial.active);
    h.shutdown().await;
}

#[tokio::test]
async fn idle_timeout_force_end_stop_and_app_shutdown_restore_trial_and_close_sockets() {
    let config = TestConfig {
        idle_timeout: Duration::from_millis(300),
        ..TestConfig::default()
    };
    let mut h = Harness::configured(configuration(config)).await;
    let original = h.open(b"(;SZ[9];B[aa];W[bb])").await;
    let original_tree = within(h.app.variation_tree()).await.unwrap();
    let (mut socket, _) = h.connect(None).await;
    send(&mut socket, WebClientMessage::EnterTrial).await;
    trial_where(&mut socket, |s| s.active).await;
    web_status(&mut h.events, |s| s.trial.is_some()).await;
    send(&mut socket, WebClientMessage::TrialMove { x: 3, y: 3 }).await;
    trial_where(&mut socket, |s| s.move_number == 1).await;
    trial_where(&mut socket, |s| !s.active).await;
    assert_eq!(
        within(h.app.variation_tree()).await.unwrap().nodes,
        original_tree.nodes
    );

    send(&mut socket, WebClientMessage::EnterTrial).await;
    trial_where(&mut socket, |s| s.active).await;
    within(h.app.force_end_web_trial()).await.unwrap();
    trial_where(&mut socket, |s| !s.active).await;
    assert!(
        within(h.app.web_board_status())
            .await
            .unwrap()
            .trial
            .is_none()
    );
    within(h.app.force_end_web_trial()).await.unwrap();

    send(&mut socket, WebClientMessage::EnterTrial).await;
    trial_where(&mut socket, |s| s.active).await;
    send(&mut socket, WebClientMessage::TrialMove { x: 4, y: 4 }).await;
    trial_where(&mut socket, |s| s.move_number == 1).await;
    within(h.app.stop_web_board()).await.unwrap();
    closed(&mut socket).await;
    assert_eq!(
        within(h.app.web_board_status()).await.unwrap(),
        WebBoardStatus::default()
    );
    assert_eq!(
        within(h.app.variation_tree()).await.unwrap().nodes,
        original_tree.nodes
    );
    let revision = within(h.app.step(0)).await.unwrap();
    let view = host_view(&mut h.events, revision).await;
    assert_eq!(view.board, original.board);
    assert!(!view.trial);
    assert!(
        within(TcpStream::connect(("127.0.0.1", h.status.port.unwrap())))
            .await
            .is_err()
    );

    h.status = within(h.app.start_web_board()).await.unwrap();
    let (mut socket, _) = h.connect(None).await;
    send(&mut socket, WebClientMessage::EnterTrial).await;
    trial_where(&mut socket, |s| s.active).await;
    within(h.app.clone().shutdown()).await.unwrap();
    closed(&mut socket).await;
    // The last document event proves shutdown restored the original before
    // closing the event stream, rather than merely dropping a trial tree.
    let restored = within(async {
        let mut latest = None;
        while let Some(event) = h.events.recv().await {
            if let AppEvent::DocumentChanged { view, .. } = event {
                latest = Some(view);
            }
        }
        latest.unwrap()
    })
    .await;
    assert!(!restored.trial);
    assert_eq!(restored.board, original.board);
}

#[tokio::test]
async fn twenty_clients_are_counted_by_actor_and_the_twenty_first_closes_with_1013() {
    let mut h = Harness::new().await;
    let mut clients = Vec::new();
    for _ in 0..20 {
        clients.push(h.connect(None).await.0);
    }
    assert_eq!(within(h.app.web_board_status()).await.unwrap().viewers, 20);
    let mut overflow = h.connect_socket(None).await;
    let closed = within(overflow.next()).await.unwrap().unwrap();
    match closed {
        Message::Close(Some(frame)) => assert_eq!(u16::from(frame.code), 1013),
        other => panic!("expected overload close, got {other:?}"),
    }
    assert_eq!(within(h.app.web_board_status()).await.unwrap().viewers, 20);
    drop(clients.pop());
    web_status(&mut h.events, |s| s.viewers == 19).await;
    let (replacement, _) = h.connect(None).await;
    clients.push(replacement);
    assert_eq!(within(h.app.web_board_status()).await.unwrap().viewers, 20);
    within(h.app.stop_web_board()).await.unwrap();
    for mut socket in clients {
        super::closed(&mut socket).await;
    }
    h.shutdown().await;
}
