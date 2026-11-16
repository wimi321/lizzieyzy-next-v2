use super::*;
use tauri::ipc::Origin;

const COMMANDS: &[&str] = &[
    "start_web_board",
    "stop_web_board",
    "web_board_status",
    "set_web_board_options",
    "force_end_web_trial",
];

#[test]
fn web_board_capability_is_exclusive_to_the_local_main_window() {
    let mut context = crate::context::<MockRuntime>();
    let authority = context.runtime_authority_mut();
    for command in COMMANDS {
        assert!(
            authority
                .resolve_access(command, "main", "main", &Origin::Local)
                .is_some()
        );
        for label in ["float-main", "float-sub", "other"] {
            assert!(
                authority
                    .resolve_access(command, label, label, &Origin::Local)
                    .is_none()
            );
        }
        assert!(
            authority
                .resolve_access(
                    command,
                    "main",
                    "main",
                    &Origin::Remote {
                        url: "http://127.0.0.1:9998/token/".parse().unwrap(),
                    }
                )
                .is_none()
        );
    }
}

#[tokio::test]
async fn web_board_ipc_forwards_options_and_idle_controls_without_binding() {
    let harness = Harness::new().await;
    assert_eq!(
        harness.invoke("web_board_status", json!({})).await["running"],
        false
    );
    assert_eq!(
        harness.invoke("settings", json!({})).await["web_board"]["allow_trial"],
        true
    );
    harness
        .invoke(
            "set_web_board_options",
            json!({"options":{"allow_trial":false}}),
        )
        .await;
    assert_eq!(
        harness.invoke("settings", json!({})).await["web_board"]["allow_trial"],
        false
    );
    let saved: Value = serde_json::from_slice(
        &std::fs::read(harness.directory.path().join("settings.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(saved["version"], 1);
    assert_eq!(saved["web_board"]["allow_trial"], false);
    assert!(
        harness
            .invoke("force_end_web_trial", json!({}))
            .await
            .is_null()
    );
    assert!(harness.invoke("stop_web_board", json!({})).await.is_null());
    harness.finish().await;
}
