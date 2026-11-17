use super::*;
use tauri::ipc::Origin;

const COMMANDS: [&str; 10] = [
    "capture_permission",
    "request_capture_permission",
    "open_capture_permission_settings",
    "start_screen_capture",
    "capture_image",
    "recognize_capture",
    "finish_capture",
    "cancel_capture",
    "capture_settings",
    "set_capture_settings",
];

#[test]
fn capture_capabilities_grant_only_exact_local_window_commands() {
    let mut context = crate::context::<MockRuntime>();
    let authority = context.runtime_authority_mut();
    for label in ["main", "capture", "float-main", "float-sub", "other"] {
        for command in COMMANDS {
            let expected = match label {
                "main" => {
                    !["capture_image", "recognize_capture", "finish_capture"].contains(&command)
                }
                "capture" => [
                    "capture_image",
                    "recognize_capture",
                    "finish_capture",
                    "cancel_capture",
                    "capture_settings",
                ]
                .contains(&command),
                _ => false,
            };
            assert_eq!(
                authority
                    .resolve_access(command, label, label, &Origin::Local)
                    .is_some(),
                expected,
                "{label}: {command}"
            );
            assert!(
                authority
                    .resolve_access(
                        command,
                        label,
                        label,
                        &Origin::Remote {
                            url: "https://example.com".parse().unwrap()
                        }
                    )
                    .is_none()
            );
        }
    }
    for command in [
        "settings",
        "subscribe",
        "unsubscribe",
        "open_path",
        "save",
        "play",
        "start_asset_download",
        "plugin:shell|execute",
        "plugin:window|close",
        "plugin:opener|open_url",
    ] {
        assert!(
            authority
                .resolve_access(command, "capture", "capture", &Origin::Local)
                .is_none(),
            "capture unexpectedly has {command}"
        );
    }
}

#[tokio::test]
async fn capture_image_returns_raw_png_for_injected_pixels_and_stale_ids_stay_typed() {
    let harness = Harness::new().await;
    let window = tauri::WebviewWindowBuilder::new(&harness.app, "capture", Default::default())
        .build()
        .unwrap();
    let session = within(harness.core.begin_capture(app_core::CaptureImage {
        width: 2,
        height: 2,
        rgba: [10, 20, 30, 255].repeat(4),
        scale_factor: 1.0,
    }))
    .await
    .unwrap();
    let (reply, response) = oneshot::channel();
    window.clone().on_message(
        InvokeRequest {
            cmd: "capture_image".into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: local_url().parse().unwrap(),
            body: InvokeBody::Json(json!({"id": session.id})),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.into(),
        },
        Box::new(move |_, _, response, _, _| {
            let _ = reply.send(response);
        }),
    );
    let response = within(response).await.unwrap();
    let InvokeResponse::Ok(tauri::ipc::InvokeResponseBody::Raw(bytes)) = response else {
        panic!("capture_image must return raw binary IPC")
    };
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    let error = request(
        &window,
        "capture_image",
        json!({"id":session.id+1}),
        local_url(),
    )
    .await
    .unwrap_err();
    assert_eq!(
        error,
        json!({"type":"Capture","data":{"type":"StaleSession"}})
    );
    within(harness.core.cancel_capture(session.id))
        .await
        .unwrap();
    harness.finish().await;
}
