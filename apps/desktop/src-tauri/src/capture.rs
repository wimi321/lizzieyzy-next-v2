//! Serialized native capture lifecycle. AppCore alone owns the image/session.
mod geometry;
#[cfg(target_os = "macos")]
mod macos;
mod native;
#[cfg(test)]
mod tests;
#[cfg(target_os = "windows")]
mod xcap_backend;

use std::{sync::Arc, time::Duration};

use app_core::{AppCore, CaptureImage, CaptureResult, CaptureSession};
use serde::Serialize;
use tauri::Runtime;
use tokio::sync::{mpsc, oneshot};
use ts_rs::TS;

use crate::{bindings::Revision, error::CommandError};
use geometry::{Display, Rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum CapturePermission {
    Granted,
    Missing,
    NotNeeded,
}

#[derive(Clone, Debug, Serialize, TS)]
pub struct ScreenCaptureStarted {
    pub session: CaptureSession,
    pub logical_width: f64,
    pub logical_height: f64,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize, TS)]
#[serde(tag = "type", content = "data")]
pub enum ScreenCaptureError {
    #[error("screen recording permission is missing")]
    PermissionMissing,
    #[error("screen capture is unsupported on this platform")]
    Unsupported,
    #[error("screen capture is already in progress")]
    Busy,
    #[error("the main window center does not belong to an available display")]
    DisplayUnavailable,
    #[error("screen capture failed: {reason}")]
    Failed { reason: String },
}

fn failure(reason: impl ToString) -> ScreenCaptureError {
    ScreenCaptureError::Failed {
        reason: reason.to_string(),
    }
}

trait Platform: Send + Sync + 'static {
    fn permission(&self) -> CapturePermission;
    fn supported(&self) -> Result<(), ScreenCaptureError>;
    fn displays(&self) -> Result<Vec<Display>, ScreenCaptureError>;
    fn main_bounds(&self) -> Result<Rect, ScreenCaptureError>;
    fn logical_bounds(&self, display: Display) -> Result<Rect, ScreenCaptureError> {
        Ok(display.bounds)
    }
    fn visible(&self, label: &str) -> Result<bool, ScreenCaptureError>;
    fn hide(&self, label: &str) -> Result<(), ScreenCaptureError>;
    fn show(&self, label: &str) -> Result<(), ScreenCaptureError>;
    fn focus(&self, label: &str) -> Result<(), ScreenCaptureError>;
    fn close_capture(&self) -> Result<(), ScreenCaptureError>;
    fn open_capture(
        &self,
        display: Display,
        session_id: u32,
        on_close: Box<dyn Fn() + Send + Sync>,
    ) -> Result<(), ScreenCaptureError>;
    fn capture(&self, display: Display) -> Result<CaptureImage, ScreenCaptureError>;
}

const HOST_WINDOWS: [&str; 3] = ["main", "float-main", "float-sub"];
// Visibility can turn false while the window manager still displays a fade-out
// frame. Give that frame time to leave the compositor before freezing pixels.
const WINDOW_HIDE_SETTLE_DELAY: Duration = Duration::from_millis(150);

#[derive(Clone, Debug, Default)]
struct RestorePlan {
    visible: Vec<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RestoreAction {
    CloseCapture,
    Show(&'static str),
    FocusMain,
}

impl RestorePlan {
    fn actions(&self) -> Vec<RestoreAction> {
        let mut actions = vec![RestoreAction::CloseCapture, RestoreAction::Show("main")];
        actions.extend(
            self.visible
                .iter()
                .filter(|label| **label != "main")
                .map(|label| RestoreAction::Show(label)),
        );
        actions.push(RestoreAction::FocusMain);
        actions
    }
}

fn restore(platform: &dyn Platform, plan: &RestorePlan) -> Result<(), ScreenCaptureError> {
    let mut errors = Vec::new();
    // Try every step, even if an earlier one fails, so a failed destroy cannot
    // strand the main window and a failed floating window cannot prevent focus.
    for action in plan.actions() {
        let result = match action {
            RestoreAction::CloseCapture => platform.close_capture(),
            RestoreAction::Show(label) => platform.show(label),
            RestoreAction::FocusMain => platform.focus("main"),
        };
        if let Err(error) = result {
            errors.push(error.to_string());
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(failure(errors.join("; ")))
    }
}

struct Session {
    id: u32,
    restore: RestorePlan,
}

enum Request {
    Start(oneshot::Sender<Result<ScreenCaptureStarted, CommandError>>),
    Finish(
        u32,
        CaptureResult,
        oneshot::Sender<Result<Revision, CommandError>>,
    ),
    Cancel(u32, oneshot::Sender<Result<(), CommandError>>),
    Close(u32),
}

#[derive(Clone)]
pub struct ScreenCapture {
    requests: mpsc::Sender<Request>,
}

impl ScreenCapture {
    fn new(core: AppCore, platform: Arc<dyn Platform>) -> Self {
        let (requests, incoming) = mpsc::channel(16);
        let weak_requests = requests.downgrade();
        tauri::async_runtime::spawn(serve(core, platform, incoming, weak_requests));
        Self { requests }
    }

    pub(crate) fn for_app<R: Runtime>(app: tauri::AppHandle<R>, core: AppCore) -> Self {
        Self::new(core, Arc::new(native::Native(app)))
    }

    pub(crate) async fn start(&self) -> Result<ScreenCaptureStarted, CommandError> {
        let (reply, response) = oneshot::channel();
        self.requests
            .send(Request::Start(reply))
            .await
            .map_err(|_| CommandError::StreamClosed)?;
        response.await.map_err(|_| CommandError::StreamClosed)?
    }

    pub(crate) async fn finish(
        &self,
        id: u32,
        result: CaptureResult,
    ) -> Result<Revision, CommandError> {
        let (reply, response) = oneshot::channel();
        self.requests
            .send(Request::Finish(id, result, reply))
            .await
            .map_err(|_| CommandError::StreamClosed)?;
        response.await.map_err(|_| CommandError::StreamClosed)?
    }

    pub(crate) async fn cancel(&self, id: u32) -> Result<(), CommandError> {
        let (reply, response) = oneshot::channel();
        self.requests
            .send(Request::Cancel(id, reply))
            .await
            .map_err(|_| CommandError::StreamClosed)?;
        response.await.map_err(|_| CommandError::StreamClosed)?
    }
}

async fn start(
    core: &AppCore,
    platform: &Arc<dyn Platform>,
    requests: &mpsc::WeakSender<Request>,
) -> Result<(ScreenCaptureStarted, Session), CommandError> {
    if platform.permission() == CapturePermission::Missing {
        return Err(ScreenCaptureError::PermissionMissing.into());
    }
    platform.supported()?;
    let display = geometry::select_display(platform.main_bounds()?, &platform.displays()?)?;
    let logical_bounds = platform.logical_bounds(display)?;
    let mut plan = RestorePlan::default();
    for label in HOST_WINDOWS {
        if platform.visible(label)? {
            plan.visible.push(label);
        }
    }
    let mut id = None;
    let result: Result<ScreenCaptureStarted, CommandError> = async {
        for label in &plan.visible {
            platform.hide(label)?;
        }
        tokio::time::timeout(Duration::from_millis(500), async {
            loop {
                let hidden = plan
                    .visible
                    .iter()
                    .map(|label| platform.visible(label))
                    .collect::<Result<Vec<_>, _>>()?;
                if hidden.iter().all(|visible| !visible) {
                    return Ok::<_, ScreenCaptureError>(());
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .map_err(|_| failure("host windows did not hide before capture"))??;
        tokio::time::sleep(WINDOW_HIDE_SETTLE_DELAY).await;
        let capture_platform = Arc::clone(platform);
        let mut image = tokio::task::spawn_blocking(move || capture_platform.capture(display))
            .await
            .map_err(failure)??;
        image.scale_factor = geometry::image_scale(logical_bounds, image.width, image.height)?;
        let session = core.begin_capture(image).await?;
        id = Some(session.id);
        let requests = requests.clone();
        let session_id = session.id;
        platform.open_capture(
            display,
            session_id,
            Box::new(move || {
                if let Some(requests) = requests.upgrade() {
                    tauri::async_runtime::spawn(async move {
                        let _ = requests.send(Request::Close(session_id)).await;
                    });
                }
            }),
        )?;
        Ok(ScreenCaptureStarted {
            session,
            logical_width: logical_bounds.width,
            logical_height: logical_bounds.height,
        })
    }
    .await;
    match result {
        Ok(started) => {
            let session = Session {
                id: started.session.id,
                restore: plan,
            };
            Ok((started, session))
        }
        Err(error) => {
            let restoration = restore(platform.as_ref(), &plan);
            let error = match restoration {
                Ok(()) => error,
                Err(restore_error) => failure(format!("{error}; restore: {restore_error}")).into(),
            };
            let reason = error.to_string();
            if let Some(id) = id {
                let _ = core.fail_capture(id, reason.clone()).await;
            } else {
                let _ = core.report_capture_failure(reason.clone()).await;
            }
            Err(error)
        }
    }
}

async fn end_windows(
    core: &AppCore,
    platform: &dyn Platform,
    session: &mut Option<Session>,
) -> Result<(), CommandError> {
    if let Some(active) = session.take()
        && let Err(error) = restore(platform, &active.restore)
    {
        let _ = core.report_capture_failure(error.to_string()).await;
        return Err(error.into());
    }
    Ok(())
}

async fn serve(
    core: AppCore,
    platform: Arc<dyn Platform>,
    mut requests: mpsc::Receiver<Request>,
    weak_requests: mpsc::WeakSender<Request>,
) {
    let mut session: Option<Session> = None;
    while let Some(request) = requests.recv().await {
        match request {
            Request::Start(reply) => {
                let result = if session.is_some() {
                    let _ = platform.focus("capture");
                    Err(ScreenCaptureError::Busy.into())
                } else {
                    match start(&core, &platform, &weak_requests).await {
                        Ok((started, active)) => {
                            session = Some(active);
                            Ok(started)
                        }
                        Err(error) => Err(error),
                    }
                };
                let _ = reply.send(result);
            }
            Request::Finish(id, result, reply) => {
                let result = match core.finish_capture(id, result).await {
                    Ok(revision) => end_windows(&core, platform.as_ref(), &mut session)
                        .await
                        .map(|()| Revision(revision)),
                    Err(error) => Err(error.into()),
                };
                let _ = reply.send(result);
            }
            Request::Cancel(id, reply) => {
                let result = match core.cancel_capture(id).await {
                    Ok(()) => end_windows(&core, platform.as_ref(), &mut session).await,
                    Err(error) => Err(error.into()),
                };
                let _ = reply.send(result);
            }
            Request::Close(id) => {
                if session.as_ref().is_some_and(|active| active.id == id) {
                    let _ = core.cancel_capture(id).await;
                    let _ = end_windows(&core, platform.as_ref(), &mut session).await;
                }
            }
        }
    }
    if let Some(active) = &session {
        let _ = core.cancel_capture(active.id).await;
    }
    let _ = end_windows(&core, platform.as_ref(), &mut session).await;
}

pub(crate) use native::{open_permission_settings, permission, request_permission};

pub(crate) fn png_bytes(image: &CaptureImage) -> Result<Vec<u8>, ScreenCaptureError> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, image.width, image.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder.write_header().map_err(failure)?;
        writer.write_image_data(&image.rgba).map_err(failure)?;
    }
    Ok(bytes)
}
