use super::*;
use tauri::ipc::Origin;

const COMMANDS: &[&str] = &[
    "comparison_state",
    "select_comparison_engine",
    "restart_comparison_engine",
    "close_comparison_engine",
    "close_other_engines",
    "close_all_engines",
];

#[test]
fn comparison_capability_is_exclusive_to_the_local_main_window() {
    let mut context = crate::context::<MockRuntime>();
    let authority = context.runtime_authority_mut();
    for command in COMMANDS {
        assert!(
            authority
                .resolve_access(command, "main", "main", &Origin::Local)
                .is_some(),
            "{command}"
        );
        for label in ["float-main", "float-sub", "other"] {
            assert!(
                authority
                    .resolve_access(command, label, label, &Origin::Local)
                    .is_none(),
                "{command}: {label}"
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
                    },
                )
                .is_none(),
            "{command}"
        );
    }
}

#[tokio::test]
async fn comparison_ipc_selects_and_closes_without_starting_outside_double_layout() {
    let harness = Harness::new().await;
    let profile = profile(harness.directory.path());
    assert_eq!(
        harness.invoke("comparison_state", json!({})).await,
        json!({"engine_id":null,"status":{"type":"Stopped"},"analysis":null})
    );
    let added = harness
        .invoke(
            "add_engine",
            json!({"name":"Comparison", "profile":profile}),
        )
        .await;
    let id = added["engines"][0]["id"].as_str().unwrap();
    let selected = harness
        .invoke("select_comparison_engine", json!({"id":id}))
        .await;
    assert_eq!(selected["comparison_engine_id"], id);
    assert!(selected["engine"].is_null());
    let view = harness.invoke("comparison_state", json!({})).await;
    assert_eq!(view["engine_id"], id);
    assert_eq!(view["status"]["type"], "Stopped");
    assert_eq!(
        request(
            &harness.main,
            "select_main_engine",
            json!({"id":id}),
            local_url()
        )
        .await
        .unwrap_err()["type"],
        "SameAsComparison"
    );
    assert_eq!(
        harness.invoke("restart_comparison_engine", json!({})).await,
        selected
    );
    for command in [
        "close_comparison_engine",
        "close_other_engines",
        "close_all_engines",
    ] {
        let settings = harness.invoke(command, json!({})).await;
        assert!(settings["comparison_engine_id"].is_null());
        assert_eq!(settings["engines"], selected["engines"]);
        assert_eq!(
            harness.invoke("comparison_state", json!({})).await["status"]["type"],
            "Stopped"
        );
        if command != "close_all_engines" {
            harness
                .invoke("select_comparison_engine", json!({"id":id}))
                .await;
        }
    }
    let main = harness.invoke("select_main_engine", json!({"id":id})).await;
    assert_eq!(main["main_engine_id"], id);
    assert_eq!(
        request(
            &harness.main,
            "select_comparison_engine",
            json!({"id":id}),
            local_url()
        )
        .await
        .unwrap_err()["type"],
        "SameAsMain"
    );
    let closed = harness
        .invoke("select_comparison_engine", json!({"id":null}))
        .await;
    assert!(closed["comparison_engine_id"].is_null());
    assert_eq!(closed["main_engine_id"], id);
    assert_eq!(
        json!(
            within(Settings::load(harness.directory.path()))
                .await
                .unwrap()
        ),
        closed
    );
    harness.finish().await;
}
