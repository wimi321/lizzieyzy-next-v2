#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use app_core::{AppEvent, CaptureCell, CaptureCells, CaptureCorner, CaptureOutcome};
use std::{
    collections::BTreeSet,
    future::{Future, poll_fn},
    io::Cursor,
    sync::Mutex,
    task::Poll,
};

async fn within<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("capture test deadline")
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}
fn display() -> Display {
    Display {
        id: 7,
        bounds: rect(-4.0, 0.0, 4.0, 3.0),
    }
}
fn pixels() -> CaptureImage {
    CaptureImage {
        width: 8,
        height: 6,
        rgba: [24, 48, 96, 255].repeat(48),
        scale_factor: 2.0,
    }
}

#[test]
fn selects_by_main_center_with_negative_origins_and_display_boundaries() {
    let displays = [
        Display {
            id: 1,
            bounds: rect(0.0, 0.0, 1920.0, 1080.0),
        },
        Display {
            id: 2,
            bounds: rect(-1600.0, -200.0, 1600.0, 1200.0),
        },
    ];
    assert_eq!(
        geometry::select_display(rect(-800.0, 50.0, 100.0, 500.0), &displays)
            .unwrap()
            .id,
        2
    );
    assert_eq!(
        geometry::select_display(rect(-100.0, 0.0, 200.0, 200.0), &displays)
            .unwrap()
            .id,
        1
    );
    assert_eq!(
        geometry::select_display(rect(5000.0, 0.0, 10.0, 10.0), &displays),
        Err(ScreenCaptureError::DisplayUnavailable)
    );
    assert!(geometry::select_display(rect(f64::NAN, 0.0, 1.0, 1.0), &displays).is_err());
    assert!(geometry::select_display(rect(0.0, 0.0, 1.0, 1.0), &[]).is_err());
}

#[test]
fn pixel_conversion_uses_actual_dimensions_at_fractional_and_retina_scales() {
    let bounds = rect(-2000.0, 100.0, 1000.0, 800.0);
    for (width, height, expected) in [
        (2000, 1600, (200.0, 100.0)),
        (1250, 1000, (125.0, 62.5)),
        (1500, 800, (150.0, 50.0)),
    ] {
        assert_eq!(
            geometry::logical_to_pixel(100.0, 50.0, bounds, width, height).unwrap(),
            expected
        );
    }
    assert_eq!(geometry::image_scale(bounds, 2000, 1600).unwrap(), 2.0);
    for (x, y) in [(-1.0, 0.0), (0.0, f64::INFINITY), (1001.0, 0.0)] {
        assert!(geometry::logical_to_pixel(x, y, bounds, 2000, 1600).is_err());
    }
    assert!(geometry::image_scale(bounds, 0, 1600).is_err());
}

#[test]
fn recovery_order_closes_capture_shows_original_hosts_then_focuses_main() {
    let plan = RestorePlan {
        visible: vec!["main", "float-sub"],
    };
    assert_eq!(
        plan.actions(),
        vec![
            RestoreAction::CloseCapture,
            RestoreAction::Show("main"),
            RestoreAction::Show("float-sub"),
            RestoreAction::FocusMain
        ]
    );
    assert_eq!(
        native::capture_url(17),
        tauri::WebviewUrl::App("index.html?window=capture&session=17".into())
    );
}

#[test]
fn png_encoding_round_trips_generated_rgba_without_files_or_display_access() {
    let image = pixels();
    let bytes = png_bytes(&image).unwrap();
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    let mut output = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut output).unwrap();
    assert_eq!((info.width, info.height), (image.width, image.height));
    assert_eq!(output, image.rgba);
}

struct Fake {
    permission: CapturePermission,
    logical_bounds: Option<Rect>,
    state: Mutex<FakeState>,
}
struct FakeState {
    visible: BTreeSet<String>,
    operations: Vec<String>,
    visibility_checks: Vec<(String, bool)>,
    captured_displays: Vec<Display>,
    opened_urls: Vec<tauri::WebviewUrl>,
    fail: Option<String>,
}
impl Fake {
    fn new(permission: CapturePermission, fail: Option<&str>) -> Self {
        Self {
            permission,
            logical_bounds: None,
            state: Mutex::new(FakeState {
                visible: ["main".into(), "float-sub".into()].into(),
                operations: vec![],
                visibility_checks: vec![],
                captured_displays: vec![],
                opened_urls: vec![],
                fail: fail.map(str::to_owned),
            }),
        }
    }
    fn operation(&self, operation: &str) -> Result<(), ScreenCaptureError> {
        let mut state = self.state.lock().unwrap();
        state.operations.push(operation.into());
        if state.fail.as_deref() == Some(operation) {
            Err(failure(operation))
        } else {
            Ok(())
        }
    }
    fn operations(&self) -> Vec<String> {
        self.state.lock().unwrap().operations.clone()
    }
}
impl Platform for Fake {
    fn permission(&self) -> CapturePermission {
        self.permission
    }
    fn supported(&self) -> Result<(), ScreenCaptureError> {
        Ok(())
    }
    fn displays(&self) -> Result<Vec<Display>, ScreenCaptureError> {
        Ok(vec![display()])
    }
    fn main_bounds(&self) -> Result<Rect, ScreenCaptureError> {
        Ok(rect(-3.0, 0.0, 1.0, 1.0))
    }
    fn logical_bounds(&self, display: Display) -> Result<Rect, ScreenCaptureError> {
        Ok(self.logical_bounds.unwrap_or(display.bounds))
    }
    fn visible(&self, label: &str) -> Result<bool, ScreenCaptureError> {
        let mut state = self.state.lock().unwrap();
        let visible = state.visible.contains(label);
        state.visibility_checks.push((label.into(), visible));
        Ok(visible)
    }
    fn hide(&self, label: &str) -> Result<(), ScreenCaptureError> {
        self.operation(&format!("hide:{label}"))?;
        self.state.lock().unwrap().visible.remove(label);
        Ok(())
    }
    fn show(&self, label: &str) -> Result<(), ScreenCaptureError> {
        self.operation(&format!("show:{label}"))?;
        self.state.lock().unwrap().visible.insert(label.into());
        Ok(())
    }
    fn focus(&self, label: &str) -> Result<(), ScreenCaptureError> {
        self.operation(&format!("focus:{label}"))
    }
    fn close_capture(&self) -> Result<(), ScreenCaptureError> {
        self.operation("close")?;
        self.state.lock().unwrap().visible.remove("capture");
        Ok(())
    }
    fn open_capture(
        &self,
        _display: Display,
        session_id: u32,
        _on_close: Box<dyn Fn() + Send + Sync>,
    ) -> Result<(), ScreenCaptureError> {
        {
            let mut state = self.state.lock().unwrap();
            state.visible.insert("capture".into());
            state.opened_urls.push(native::capture_url(session_id));
        }
        self.operation("open")
    }
    fn capture(&self, display: Display) -> Result<CaptureImage, ScreenCaptureError> {
        assert!(
            self.state.lock().unwrap().visible.is_empty(),
            "capture must follow confirmed hiding"
        );
        self.operation("capture")?;
        self.state.lock().unwrap().captured_displays.push(display);
        Ok(pixels())
    }
}

async fn next_end(subscription: &mut app_core::AppSubscription) -> CaptureOutcome {
    within(async {
        loop {
            if let AppEvent::CaptureEnded { outcome } = subscription
                .recv()
                .await
                .expect("capture event stream closed")
            {
                return outcome;
            }
        }
    })
    .await
}

#[tokio::test]
async fn denied_permission_does_not_touch_windows_or_supply_image() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = within(app_core::spawn(directory.path())).await.unwrap();
    let platform = Arc::new(Fake::new(CapturePermission::Missing, None));
    let service = ScreenCapture::new(core.clone(), platform.clone());
    assert!(matches!(
        within(service.start()).await,
        Err(CommandError::ScreenCapture(
            ScreenCaptureError::PermissionMissing
        ))
    ));
    assert!(platform.operations().is_empty());
    assert!(within(core.capture_image(1)).await.is_err());
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn capture_and_open_failures_restore_all_windows_and_publish_failure() {
    for fail in ["hide:main", "hide:float-sub", "capture", "open"] {
        let directory = tempfile::tempdir().unwrap();
        let (core, mut subscription) = within(app_core::spawn(directory.path())).await.unwrap();
        let platform = Arc::new(Fake::new(CapturePermission::Granted, Some(fail)));
        let service = ScreenCapture::new(core.clone(), platform.clone());
        assert!(within(service.start()).await.is_err());
        assert!(
            matches!(next_end(&mut subscription).await, CaptureOutcome::Failed { reason } if reason.contains(fail))
        );
        let operations = platform.operations();
        assert!(operations.ends_with(&[
            "close".into(),
            "show:main".into(),
            "show:float-sub".into(),
            "focus:main".into()
        ]));
        assert!(within(core.capture_image(1)).await.is_err());
        within(core.shutdown()).await.unwrap();
    }
}

#[tokio::test]
async fn concurrent_start_is_busy_then_cancel_releases_image_and_restores_visible_windows() {
    let directory = tempfile::tempdir().unwrap();
    let (core, mut subscription) = within(app_core::spawn(directory.path())).await.unwrap();
    let platform = Arc::new(Fake::new(CapturePermission::Granted, None));
    let service = ScreenCapture::new(core.clone(), platform.clone());
    let (first, second) = within(async { tokio::join!(service.start(), service.start()) }).await;
    let session = first.unwrap().session;
    assert!(matches!(
        second,
        Err(CommandError::ScreenCapture(ScreenCaptureError::Busy))
    ));
    assert_eq!(
        (session.width, session.height, session.scale_factor),
        (8, 6, 2.0)
    );
    assert_eq!(
        platform
            .operations()
            .iter()
            .filter(|op| op.as_str() == "capture")
            .count(),
        1
    );
    assert!(platform.operations().ends_with(&["focus:capture".into()]));
    assert!(within(service.cancel(session.id + 1)).await.is_err());
    assert!(within(core.capture_image(session.id)).await.is_ok());
    within(service.cancel(session.id)).await.unwrap();
    assert_eq!(next_end(&mut subscription).await, CaptureOutcome::Cancelled);
    assert!(within(core.capture_image(session.id)).await.is_err());
    assert_eq!(
        platform.state.lock().unwrap().visible,
        BTreeSet::from(["main".into(), "float-sub".into()])
    );
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn invalid_corrections_preserve_capture_and_valid_finish_restores_after_commit() {
    let directory = tempfile::tempdir().unwrap();
    let (core, mut subscription) = within(app_core::spawn(directory.path())).await.unwrap();
    let platform = Arc::new(Fake::new(CapturePermission::Granted, None));
    let service = ScreenCapture::new(core.clone(), platform.clone());
    let session = within(service.start()).await.unwrap().session;
    let mut result = CaptureResult {
        corner: CaptureCorner::TopLeft,
        board_size: 9,
        cells: CaptureCells {
            rows: 1,
            columns: 1,
            cells: vec![],
        },
    };
    assert!(
        within(service.finish(session.id, result.clone()))
            .await
            .is_err()
    );
    assert!(within(core.capture_image(session.id)).await.is_ok());
    assert!(!platform.operations().contains(&"close".into()));
    result.cells.cells.push(CaptureCell::Black);
    within(service.finish(session.id, result)).await.unwrap();
    assert_eq!(next_end(&mut subscription).await, CaptureOutcome::Finished);
    assert!(platform.operations().ends_with(&[
        "close".into(),
        "show:main".into(),
        "show:float-sub".into(),
        "focus:main".into()
    ]));
    assert!(within(core.capture_image(session.id)).await.is_err());
    within(core.shutdown()).await.unwrap();
}

#[test]
fn restoration_attempts_every_step_even_when_capture_close_fails() {
    let platform = Fake::new(CapturePermission::Granted, Some("close"));
    let plan = RestorePlan {
        visible: vec!["main", "float-main", "float-sub"],
    };
    assert!(restore(&platform, &plan).is_err());
    assert_eq!(
        platform.operations(),
        [
            "close",
            "show:main",
            "show:float-main",
            "show:float-sub",
            "focus:main"
        ]
    );
}

#[cfg(target_os = "macos")]
#[test]
fn screenshot_manager_normalization_preserves_generated_pixel_corners_and_channels() {
    use apple_cf::cg::CGContext;
    // Draw into a 2x2 memory bitmap: no display, screenshot or permission APIs.
    let context = CGContext::new_rgba8(2, 2).unwrap();
    let colors = [
        (255_u8, 0_u8, 0_u8),
        (0, 255, 0),
        (0, 0, 255),
        (180, 90, 30),
    ];
    for (index, (r, g, b)) in colors.iter().copied().enumerate() {
        context.set_rgb_fill_color(
            f64::from(r) / 255.0,
            f64::from(g) / 255.0,
            f64::from(b) / 255.0,
            1.0,
        );
        // Quartz drawing coordinates start at bottom-left; our expected RGBA
        // buffer starts with the top-left pixel, as screenshots do.
        context.fill_rect((index % 2) as f64, (1 - index / 2) as f64, 1.0, 1.0);
    }
    let image = context.snapshot_to_image().unwrap();
    let normalized = macos::image_rgba(&image).unwrap();
    assert_eq!((normalized.width, normalized.height), (2, 2));
    assert_eq!(
        normalized.rgba,
        vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 180, 90, 30, 255
        ]
    );
}

#[tokio::test]
async fn old_native_window_close_cannot_end_a_replacement_capture() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = within(app_core::spawn(directory.path())).await.unwrap();
    let platform = Arc::new(Fake::new(CapturePermission::Granted, None));
    let service = ScreenCapture::new(core.clone(), platform.clone());
    let old = within(service.start()).await.unwrap().session.id;
    within(service.cancel(old)).await.unwrap();
    let new = within(service.start()).await.unwrap().session.id;
    assert_ne!(old, new);
    // Reproduce an OS callback queued after the new window has been created.
    within(service.requests.send(Request::Close(old)))
        .await
        .unwrap();
    // FIFO service response is the barrier, with no sleep or timing assumption.
    assert!(matches!(
        within(service.start()).await,
        Err(CommandError::ScreenCapture(ScreenCaptureError::Busy))
    ));
    assert!(within(core.capture_image(new)).await.is_ok());
    assert!(platform.state.lock().unwrap().visible.contains("capture"));
    within(service.cancel(new)).await.unwrap();
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn capture_waits_for_compositor_after_all_hosts_confirm_hidden() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = within(app_core::spawn(directory.path())).await.unwrap();
    let fake = Arc::new(Fake::new(CapturePermission::Granted, None));
    let platform: Arc<dyn Platform> = fake.clone();
    let (requests, _incoming) = mpsc::channel(1);
    let weak_requests = requests.downgrade();

    // Poll this future on the paused test runtime, rather than the separate
    // Tauri service runtime. File I/O and blocking workers use real time below.
    tokio::time::pause();
    let began = tokio::time::Instant::now();
    let mut pending = Box::pin(start(&core, &platform, &weak_requests));
    assert!(
        within(poll_fn(|cx| Poll::Ready(pending.as_mut().poll(cx))))
            .await
            .is_pending()
    );
    {
        let state = fake.state.lock().unwrap();
        assert!(
            state.visible.is_empty(),
            "settling starts only after all hosts are hidden"
        );
        assert!(
            state
                .visibility_checks
                .ends_with(&[("main".into(), false), ("float-sub".into(), false)])
        );
        assert!(!state.operations.contains(&"capture".into()));
    }
    within(tokio::time::advance(
        WINDOW_HIDE_SETTLE_DELAY - Duration::from_millis(1),
    ))
    .await;
    assert!(
        within(poll_fn(|cx| Poll::Ready(pending.as_mut().poll(cx))))
            .await
            .is_pending()
    );
    assert!(!fake.operations().contains(&"capture".into()));
    assert_eq!(
        tokio::time::Instant::now() - began,
        WINDOW_HIDE_SETTLE_DELAY - Duration::from_millis(1)
    );

    within(tokio::time::advance(Duration::from_millis(1))).await;
    tokio::time::resume();
    let (started, active) = within(pending).await.unwrap();
    let operations = fake.operations();
    assert_eq!(
        operations
            .iter()
            .filter(|operation| operation.as_str() == "capture")
            .count(),
        1
    );
    assert!(operations.ends_with(&["capture".into(), "open".into()]));
    within(core.cancel_capture(started.session.id))
        .await
        .unwrap();
    restore(platform.as_ref(), &active.restore).unwrap();
    within(core.shutdown()).await.unwrap();
}

#[test]
fn fast_png_keeps_nonuniform_generated_rgba_lossless_and_compressed() {
    let (width, height) = (256_u32, 192_u32);
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            rgba.extend_from_slice(&[x as u8, y as u8, (x ^ y) as u8, 255]);
        }
    }
    let image = CaptureImage {
        width,
        height,
        rgba,
        scale_factor: 1.0,
    };
    let bytes = png_bytes(&image).unwrap();
    assert!(
        bytes.len() < image.rgba.len(),
        "the screenshot remains compressed for IPC"
    );
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    let mut output = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut output).unwrap();
    assert_eq!((info.width, info.height), (width, height));
    assert_eq!(output, image.rgba);
}

#[tokio::test]
async fn logical_bounds_control_returned_size_and_scale_without_changing_display_selection() {
    let directory = tempfile::tempdir().unwrap();
    let (core, mut events) = within(app_core::spawn(directory.path())).await.unwrap();
    let mut fake = Fake::new(CapturePermission::Granted, None);
    // The main center (-2.5, 0.5) is inside the physical display but outside
    // these logical bounds; display selection must keep the physical space.
    fake.logical_bounds = Some(rect(-2.0, 0.0, 2.0, 1.5));
    let platform = Arc::new(fake);
    let service = ScreenCapture::new(core.clone(), platform.clone());
    let started = within(service.start()).await.unwrap();
    assert_eq!((started.logical_width, started.logical_height), (2.0, 1.5));
    assert_eq!((started.session.width, started.session.height), (8, 6));
    assert_eq!(started.session.scale_factor, 4.0);
    let image = within(core.capture_image(started.session.id))
        .await
        .unwrap();
    assert_eq!(image.scale_factor, 4.0);
    assert_eq!(image.rgba, pixels().rgba);
    assert_eq!(
        platform.state.lock().unwrap().captured_displays,
        vec![display()]
    );
    drop(image);

    within(service.cancel(started.session.id)).await.unwrap();
    assert_eq!(next_end(&mut events).await, CaptureOutcome::Cancelled);
    assert!(
        within(core.capture_image(started.session.id))
            .await
            .is_err()
    );
    assert_eq!(
        platform.state.lock().unwrap().visible,
        BTreeSet::from(["main".into(), "float-sub".into()])
    );
    within(core.shutdown()).await.unwrap();
}

#[tokio::test]
async fn each_capture_window_url_uses_its_returned_session_id_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let (core, _) = within(app_core::spawn(directory.path())).await.unwrap();
    let platform = Arc::new(Fake::new(CapturePermission::Granted, None));
    let service = ScreenCapture::new(core.clone(), platform.clone());

    let old_id = within(service.start()).await.unwrap().session.id;
    within(service.cancel(old_id)).await.unwrap();
    let new_id = within(service.start()).await.unwrap().session.id;
    assert_ne!(old_id, new_id);
    {
        let state = platform.state.lock().unwrap();
        assert_eq!(state.opened_urls.len(), 2);
        assert_eq!(
            state.opened_urls[0],
            tauri::WebviewUrl::App(format!("index.html?window=capture&session={old_id}").into())
        );
        assert_eq!(
            state.opened_urls[1],
            tauri::WebviewUrl::App(format!("index.html?window=capture&session={new_id}").into())
        );
        assert_ne!(state.opened_urls[0], state.opened_urls[1]);
    }

    within(service.cancel(new_id)).await.unwrap();
    within(core.shutdown()).await.unwrap();
}
