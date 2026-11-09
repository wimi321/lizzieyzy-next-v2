use super::*;

#[tokio::test]
async fn native_drop_opens_through_core_and_failure_is_not_replayed() {
    let mut harness = Harness::new().await;
    harness.subscribe(201).await;
    harness.document(201, 0).await;
    let path = harness.directory.path().join("native.SGF");
    std::fs::write(&path, "(;SZ[9]C[native])").unwrap();
    let event = tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop {
        paths: vec![path],
        position: tauri::PhysicalPosition::new(10.0, 10.0),
    });
    crate::drag_drop::on_window_event(&harness.main.as_ref().window(), &event);
    let view = harness.document(201, 1).await;
    assert_eq!(view.file_name.as_deref(), Some("native.SGF"));
    assert_eq!(view.comment.as_deref(), Some("native"));
    let missing = harness.directory.path().join("missing.sgf");
    let error = request(
        &harness.main,
        "open_path",
        json!({"path": missing}),
        local_url(),
    )
    .await;
    assert!(error.is_err());
    within(async {
        loop {
            if let AppEvent::OpenFailed { path, reason } = harness.next().await.event {
                assert_eq!(path, missing.to_string_lossy());
                assert!(!reason.is_empty());
                break;
            }
        }
    })
    .await;
    harness.subscribe(202).await;
    harness
        .invoke("open_bytes", json!({"bytes": b"(;C[barrier])".to_vec()}))
        .await;
    within(async {
        loop {
            let message = harness.next().await;
            assert_eq!(message.callback, 202);
            assert!(!matches!(message.event, AppEvent::OpenFailed { .. }));
            if matches!(message.event, AppEvent::DocumentChanged { revision: 2, .. }) {
                break;
            }
        }
    })
    .await;
    harness.finish().await;
}

#[tokio::test]
async fn engine_display_name_command_returns_tagged_names_and_rejects_remote_page() {
    let harness = Harness::new().await;
    let profile = json!({
        "program": "katago", "model": "default.bin.gz", "config": "analysis.cfg", "extra_args": [],
        "canonical_model_name": "kata1-tf3-b11c768-s12002M-d6304M.bin.gz"
    });
    assert_eq!(
        harness
            .invoke("engine_display_name", json!({"profile": profile}))
            .await,
        json!({"type": "Text", "data": "Transformer B11 · 2026-09-25"})
    );
    assert!(
        request(
            &harness.main,
            "engine_display_name",
            json!({"profile": profile}),
            "https://example.com"
        )
        .await
        .is_err()
    );
    harness.finish().await;
}
