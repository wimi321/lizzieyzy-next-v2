#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
use common::within;
mod commentary_session {
    pub mod support;
}
use app_core::*;
use commentary_session::support::*;

#[tokio::test]
async fn streaming_modes_write_correct_nodes_and_undo_with_saved_analysis_only() {
    for (mode, start, target, evidence_node) in [
        (CommentaryMode::NextMove, 1, 1, 1),
        (CommentaryMode::Range { from: 2, to: 2 }, 0, 2, 1),
        (CommentaryMode::WholeGame, 2, 0, 0),
    ] {
        let mut server = Server::new().await;
        let (_dir, app, mut events) = app(&server.base).await;
        within(app.go_to(start)).await.unwrap();
        let initial = within(app.start_commentary(req(mode))).await.unwrap();
        assert_eq!(initial.phase, CommentaryPhase::Preparing);
        assert_eq!(initial.positions[0].node_id, evidence_node);
        let request = within(server.requests.recv()).await.unwrap();
        assert!(
            request["messages"][1]["content"]
                .as_str()
                .unwrap()
                .contains(&initial.evidence)
        );
        assert!(!request.to_string().contains("manual comment"));
        within(server.steps.send(Step::Text("C3 ".into())))
            .await
            .unwrap();
        let partial = progress(&mut events, CommentaryPhase::Generating).await;
        // A generating snapshot may precede the first network chunk.
        let partial = if partial.text.is_empty() {
            within(async {
                loop {
                    let v = progress(&mut events, CommentaryPhase::Generating).await;
                    if !v.text.is_empty() {
                        break v;
                    }
                }
            })
            .await
        } else {
            partial
        };
        assert_eq!(partial.text, "C3 ");
        // Actor responds while the server is waiting to finish the stream.
        within(app.go_to(start)).await.unwrap();
        server.answer("connects the nearby stones.").await;
        let completed = progress(&mut events, CommentaryPhase::Completed).await;
        assert_eq!(completed.text, "C3 connects the nearby stones.");
        assert!(completed.issues.is_empty());
        within(app.go_to(if target == 0 { 1 } else { 0 }))
            .await
            .unwrap();
        within(app.go_to(target)).await.unwrap();
        let view = document(&mut events).await;
        assert!(
            view.comment
                .as_deref()
                .unwrap_or_default()
                .contains(&completed.text)
        );
        assert!(view.can_undo);
        if target == 0 {
            assert!(
                view.comment
                    .as_deref()
                    .unwrap_or_default()
                    .starts_with("manual comment")
            );
        }
        within(app.undo()).await.unwrap();
        let undone = document(&mut events).await;
        assert!(
            !undone
                .comment
                .as_deref()
                .unwrap_or_default()
                .contains("LizzieYzy AI Commentary BEGIN")
        );
        within(app.shutdown()).await.unwrap();
    }
}

#[tokio::test]
async fn follow_up_reuses_frozen_evidence_and_appends_without_overwriting_initial_answer() {
    let mut server = Server::new().await;
    let (_dir, app, mut events) = app(&server.base).await;
    let first = within(app.start_commentary(req(CommentaryMode::NextMove)))
        .await
        .unwrap();
    within(server.requests.recv()).await.unwrap();
    server.answer("C3 connects stones.").await;
    progress(&mut events, CommentaryPhase::Completed).await;
    within(app.go_to(2)).await.unwrap();
    let second = within(app.start_commentary(CommentaryRequest {
        mode: CommentaryMode::WholeGame,
        follow_up: Some("Why?".into()),
    }))
    .await
    .unwrap();
    assert_eq!(first.evidence, second.evidence);
    assert_eq!(first.positions, second.positions);
    let request = within(server.requests.recv()).await.unwrap();
    assert_eq!(request["messages"].as_array().unwrap().len(), 4);
    assert_eq!(request["messages"][2]["content"], "C3 connects stones.");
    server.answer("C3 keeps the group connected.").await;
    progress(&mut events, CommentaryPhase::Completed).await;
    within(app.go_to(0)).await.unwrap();
    let view = document(&mut events).await;
    assert!(
        view.comment
            .as_deref()
            .unwrap_or_default()
            .contains("C3 connects stones.")
    );
    assert!(
        view.comment
            .as_deref()
            .unwrap_or_default()
            .contains("追问：Why?")
    );
    assert!(
        view.comment
            .as_deref()
            .unwrap_or_default()
            .contains("C3 keeps the group connected.")
    );
    within(app.undo()).await.unwrap();
    let old = document(&mut events).await;
    assert!(
        old.comment
            .as_deref()
            .unwrap_or_default()
            .contains("C3 connects stones.")
    );
    assert!(!old.comment.as_deref().unwrap_or_default().contains("追问"));
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn corrects_once_and_failed_verification_never_writes_even_via_follow_up() {
    let mut server = Server::new().await;
    let (_dir, app, mut events) = app(&server.base).await;
    within(app.start_commentary(req(CommentaryMode::NextMove)))
        .await
        .unwrap();
    within(server.requests.recv()).await.unwrap();
    server.answer("Z25 has 101% winrate.").await;
    let correction = within(server.requests.recv()).await.unwrap();
    assert_eq!(correction["messages"][2]["role"], "assistant");
    assert!(
        correction["messages"][3]["content"]
            .as_str()
            .unwrap()
            .contains("Z25")
    );
    server.answer("Z24 is better.").await;
    let completed = progress(&mut events, CommentaryPhase::Completed).await;
    assert!(!completed.issues.is_empty());
    assert!(completed.replacement);
    assert_eq!(completed.text, "Z24 is better.");
    within(app.start_commentary(CommentaryRequest {
        follow_up: Some("explain".into()),
        ..Default::default()
    }))
    .await
    .unwrap();
    within(server.requests.recv()).await.unwrap();
    server.answer("C3 connects stones.").await;
    assert!(
        !progress(&mut events, CommentaryPhase::Completed)
            .await
            .issues
            .is_empty()
    );
    within(app.go_to(1)).await.unwrap();
    document(&mut events).await;
    within(app.go_to(0)).await.unwrap();
    assert_eq!(
        document(&mut events).await.comment.as_deref(),
        Some("manual comment")
    );
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn successful_correction_replaces_draft_and_is_the_only_saved_answer() {
    let mut server = Server::new().await;
    let (_dir, app, mut events) = app(&server.base).await;
    within(app.start_commentary(req(CommentaryMode::NextMove)))
        .await
        .unwrap();
    within(server.requests.recv()).await.unwrap();
    server.answer("Z25 wins.").await;
    within(server.requests.recv()).await.unwrap();
    server.answer("C3 connects stones.").await;
    let done = progress(&mut events, CommentaryPhase::Completed).await;
    assert!(done.issues.is_empty());
    assert!(done.replacement);
    within(app.go_to(1)).await.unwrap();
    document(&mut events).await;
    within(app.go_to(0)).await.unwrap();
    let view = document(&mut events).await;
    assert!(
        view.comment
            .as_deref()
            .unwrap_or_default()
            .contains("C3 connects stones.")
    );
    assert!(!view.comment.as_deref().unwrap_or_default().contains("Z25"));
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn cancellation_busy_document_replace_and_shutdown_close_local_http() {
    for action in ["stop", "replace", "shutdown"] {
        let mut server = Server::new().await;
        let (_dir, app, mut events) = app(&server.base).await;
        within(app.start_commentary(req(CommentaryMode::NextMove)))
            .await
            .unwrap();
        within(server.requests.recv()).await.unwrap();
        assert!(matches!(
            within(app.start_commentary(Default::default())).await,
            Err(AppError::Commentary(CommentaryError::Busy))
        ));
        within(server.steps.send(Step::Text("C3".into())))
            .await
            .unwrap();
        within(async {
            loop {
                if !progress(&mut events, CommentaryPhase::Generating)
                    .await
                    .text
                    .is_empty()
                {
                    break;
                }
            }
        })
        .await;
        match action {
            "stop" => {
                assert_eq!(
                    within(app.stop_commentary()).await.unwrap().phase,
                    CommentaryPhase::Stopped
                );
            }
            "replace" => {
                within(app.open_bytes(b"(;SZ[9])".to_vec())).await.unwrap();
            }
            _ => {
                within(app.clone().shutdown()).await.unwrap();
            }
        }
        within(server.closed.recv()).await.unwrap();
        if action != "shutdown" {
            within(app.shutdown()).await.unwrap();
        }
    }
}

#[tokio::test]
async fn no_analysis_invalid_settings_and_file_contains_no_key() {
    let server = Server::new().await;
    let (dir, app, _events) = app(&server.base).await;
    let before = within(app.settings()).await.unwrap();
    let disk = tokio::fs::read(dir.path().join("settings.json"))
        .await
        .unwrap();
    assert!(!String::from_utf8_lossy(&disk).contains("test-only-secret"));
    assert!(
        within(app.set_commentary_key("bad key".into(), true))
            .await
            .is_err()
    );
    assert_eq!(within(app.settings()).await.unwrap(), before);
    for rank in ["19k", "10d", "0k"] {
        let mut invalid = before.commentary.clone();
        invalid.rank = rank.into();
        assert!(within(app.set_commentary_settings(invalid)).await.is_err());
    }
    assert_eq!(
        tokio::fs::read(dir.path().join("settings.json"))
            .await
            .unwrap(),
        disk
    );
    within(app.open_bytes(b"(;SZ[9])".to_vec())).await.unwrap();
    assert!(matches!(
        within(app.start_commentary(Default::default())).await,
        Err(AppError::Commentary(CommentaryError::NoAnalysis))
    ));
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn interruption_retains_partial_text_without_writing_and_replacement_clears_old_evidence() {
    let mut server = Server::new().await;
    let (_dir, app, mut events) = app(&server.base).await;
    within(app.start_commentary(Default::default()))
        .await
        .unwrap();
    within(server.requests.recv()).await.unwrap();
    within(server.steps.send(Step::Text("C3 partial".into())))
        .await
        .unwrap();
    within(server.steps.send(Step::End)).await.unwrap();
    let failed = progress(&mut events, CommentaryPhase::Failed).await;
    assert_eq!(failed.text, "C3 partial");
    assert!(matches!(
        failed.error,
        Some(CommentaryError::Client(
            commentary_client::CommentaryClientError::Interrupted
        ))
    ));
    within(app.open_bytes(b"(;SZ[9])".to_vec())).await.unwrap();
    let cleared = within(app.commentary_status()).await.unwrap();
    assert!(cleared.evidence.is_empty());
    assert!(cleared.positions.is_empty());
    assert!(cleared.text.is_empty());
    assert_eq!(cleared.phase, CommentaryPhase::Stopped);
    within(app.shutdown()).await.unwrap();
}

#[tokio::test]
async fn edits_during_generation_prevent_stale_write_but_disabled_save_keeps_follow_up_chain() {
    let mut server = Server::new().await;
    let (_dir, app, mut events) = app(&server.base).await;
    within(app.start_commentary(Default::default()))
        .await
        .unwrap();
    within(server.requests.recv()).await.unwrap();
    within(app.set_comment("new user comment".into()))
        .await
        .unwrap();
    server.answer("C3 connects.").await;
    let failed = progress(&mut events, CommentaryPhase::Failed).await;
    assert_eq!(failed.error, Some(CommentaryError::PositionChanged));
    let mut options = within(app.settings()).await.unwrap().commentary;
    options.write_to_comment = false;
    within(app.set_commentary_settings(options.clone()))
        .await
        .unwrap();
    within(app.start_commentary(Default::default()))
        .await
        .unwrap();
    within(server.requests.recv()).await.unwrap();
    server.answer("C3 connects.").await;
    progress(&mut events, CommentaryPhase::Completed).await;
    options.write_to_comment = true;
    within(app.set_commentary_settings(options)).await.unwrap();
    within(app.start_commentary(CommentaryRequest {
        follow_up: Some("more".into()),
        ..Default::default()
    }))
    .await
    .unwrap();
    within(server.requests.recv()).await.unwrap();
    server.answer("C3 protects stones.").await;
    progress(&mut events, CommentaryPhase::Completed).await;
    within(app.go_to(1)).await.unwrap();
    document(&mut events).await;
    within(app.go_to(0)).await.unwrap();
    let view = document(&mut events).await;
    assert!(
        view.comment
            .as_deref()
            .unwrap_or_default()
            .starts_with("new user comment")
    );
    assert!(
        view.comment
            .as_deref()
            .unwrap_or_default()
            .contains("C3 connects.")
    );
    assert!(
        view.comment
            .as_deref()
            .unwrap_or_default()
            .contains("C3 protects stones.")
    );
    within(app.shutdown()).await.unwrap();
}
