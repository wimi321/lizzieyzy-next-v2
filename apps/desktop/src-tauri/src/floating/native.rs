//! Thin Tauri adapter. All geometry is logical; monitor APIs return physical pixels.
use app_core::{FloatingBoardKind, WindowBounds, WindowKind, WindowPlacement};
#[cfg(not(target_os = "windows"))]
use tauri::{LogicalPosition, LogicalSize};
use tauri::{Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

#[cfg(any(target_os = "windows", test))]
#[path = "geometry.rs"]
mod geometry;

use super::{Action, FloatingWindows, WindowPlatform, window_kind};
use crate::window_state;

pub(crate) struct NativeWindows<R: Runtime>(pub tauri::AppHandle<R>);

impl<R: Runtime> NativeWindows<R> {
    fn window(&self, kind: WindowKind) -> Result<tauri::WebviewWindow<R>, String> {
        self.0
            .get_webview_window(window_state::label(kind))
            .ok_or_else(|| "window is unavailable".into())
    }

    fn monitors(&self) -> Result<Vec<tauri::Monitor>, String> {
        let primary = self.0.primary_monitor().map_err(|e| e.to_string())?;
        let mut monitors = self.0.available_monitors().map_err(|e| e.to_string())?;
        if let Some(primary) = primary {
            monitors.retain(|monitor| monitor.position() != primary.position());
            monitors.insert(0, primary);
        }
        Ok(monitors)
    }
}

fn monitor_bounds(monitor: &tauri::Monitor) -> Option<WindowBounds> {
    let scale = monitor.scale_factor();
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let area = monitor.work_area();
    let position = area.position.to_logical::<f64>(scale);
    let size = area.size.to_logical::<f64>(scale);
    // Round inward to stay inside the physical work area at fractional DPI.
    let right = (position.x + size.width).floor();
    let bottom = (position.y + size.height).floor();
    let x = position.x.ceil();
    let y = position.y.ceil();
    let width = (right - x).max(0.0) as u32;
    let height = (bottom - y).max(0.0) as u32;
    (width > 0 && height > 0).then_some(WindowBounds {
        x: x as i32,
        y: y as i32,
        width,
        height,
    })
}

impl<R: Runtime> WindowPlatform for NativeWindows<R> {
    fn work_areas(&self) -> Result<Vec<WindowBounds>, String> {
        // The first area is explicitly the primary display for offscreen fallback.
        Ok(self.monitors()?.iter().filter_map(monitor_bounds).collect())
    }

    fn minimum(&self, kind: WindowKind) -> (u32, u32) {
        let base = window_state::minimum(kind);
        let frame = self
            .window(kind)
            .ok()
            .and_then(|window| {
                let scale = window.scale_factor().ok()?;
                let outer = window.outer_size().ok()?.to_logical::<u32>(scale);
                let inner = window.inner_size().ok()?.to_logical::<u32>(scale);
                Some((
                    outer.width.saturating_sub(inner.width),
                    outer.height.saturating_sub(inner.height),
                ))
            })
            .unwrap_or_default();
        // Tauri's configured minimum is client size; fitting uses outer bounds.
        (
            base.0.saturating_add(frame.0),
            base.1.saturating_add(frame.1),
        )
    }

    fn exists(&self, kind: WindowKind) -> bool {
        self.0
            .get_webview_window(window_state::label(kind))
            .is_some()
    }

    fn create(
        &mut self,
        kind: FloatingBoardKind,
        bounds: WindowBounds,
        on_top: bool,
        locked: bool,
    ) -> Result<(), String> {
        let label = window_state::label(window_kind(kind));
        WebviewWindowBuilder::new(&self.0, label, floating_url(kind))
            .title("LizzieYzy Next 2")
            .decorations(false)
            .inner_size(f64::from(bounds.width), f64::from(bounds.height))
            .position(f64::from(bounds.x), f64::from(bounds.y))
            .min_inner_size(100.0, 100.0)
            .always_on_top(on_top)
            .resizable(!locked)
            .build()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn destroy(&mut self, kind: WindowKind) -> Result<(), String> {
        self.window(kind)?.destroy().map_err(|e| e.to_string())
    }

    fn placement(&self, kind: WindowKind) -> Result<Option<WindowPlacement>, String> {
        if !self.exists(kind) {
            return Ok(None);
        }
        let window = self.window(kind)?;
        // Minimized dimensions are not restorable placements.
        if window.is_minimized().map_err(|e| e.to_string())? {
            return Ok(None);
        }
        let scale = window.scale_factor().map_err(|e| e.to_string())?;
        let position = window
            .outer_position()
            .map_err(|e| e.to_string())?
            .to_logical::<i32>(scale);
        let size = window
            .outer_size()
            .map_err(|e| e.to_string())?
            .to_logical::<u32>(scale);
        Ok(Some(WindowPlacement {
            bounds: WindowBounds {
                x: position.x,
                y: position.y,
                width: size.width,
                height: size.height,
            },
            maximized: window.is_maximized().map_err(|e| e.to_string())?,
        }))
    }

    fn place(&mut self, kind: WindowKind, placement: WindowPlacement) -> Result<(), String> {
        let window = self.window(kind)?;
        window.unmaximize().map_err(|e| e.to_string())?;
        let scale = window.scale_factor().map_err(|e| e.to_string())?;
        let outer = window
            .outer_size()
            .map_err(|e| e.to_string())?
            .to_logical::<u32>(scale);
        let inner = window
            .inner_size()
            .map_err(|e| e.to_string())?
            .to_logical::<u32>(scale);
        // Saved bounds include decorations; Tauri set_size accepts client size.
        let frame = (
            outer.width.saturating_sub(inner.width),
            outer.height.saturating_sub(inner.height),
        );
        #[cfg(target_os = "windows")]
        {
            let screens = self
                .monitors()
                .unwrap_or_default()
                .iter()
                .filter_map(|monitor| {
                    monitor_bounds(monitor).map(|bounds| (bounds, monitor.scale_factor()))
                })
                .collect::<Vec<_>>();
            let (position, size) =
                geometry::target_geometry(placement.bounds, frame, &screens, scale);
            // Handle WM_DPICHANGED before the final size; convert with target DPI.
            window.set_position(position).map_err(|e| e.to_string())?;
            window.set_size(size).map_err(|e| e.to_string())?;
        }
        #[cfg(not(target_os = "windows"))]
        {
            let width = placement.bounds.width.saturating_sub(frame.0).max(1);
            let height = placement.bounds.height.saturating_sub(frame.1).max(1);
            window
                .set_size(LogicalSize::new(width, height))
                .map_err(|e| e.to_string())?;
            window
                .set_position(LogicalPosition::new(placement.bounds.x, placement.bounds.y))
                .map_err(|e| e.to_string())?;
        }
        if placement.maximized {
            window.maximize().map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn options(
        &mut self,
        kind: FloatingBoardKind,
        on_top: bool,
        locked: bool,
    ) -> Result<(), String> {
        let window = self.window(window_kind(kind))?;
        window
            .set_always_on_top(on_top)
            .map_err(|e| e.to_string())?;
        window.set_resizable(!locked).map_err(|e| e.to_string())
    }
}

pub(crate) fn floating_url(kind: FloatingBoardKind) -> WebviewUrl {
    // Tauri 2.12 joins App paths (including their query) against devUrl or the
    // packaged application URL. No external URL or initialization global needed.
    WebviewUrl::App(
        format!(
            "index.html?window={}",
            window_state::label(window_kind(kind))
        )
        .into(),
    )
}

pub(super) fn close_action(kind: WindowKind) -> Action {
    match kind {
        WindowKind::Main => Action::Flush,
        WindowKind::FloatingMain => Action::Open(FloatingBoardKind::Main, false),
        WindowKind::FloatingSub => Action::Open(FloatingBoardKind::Sub, false),
    }
}

pub(crate) fn on_window_event<R: Runtime>(window: &tauri::Window<R>, event: &tauri::WindowEvent) {
    let Some(kind) = window_state::kind(window.label()) else {
        return;
    };
    let Some(service) = window.app_handle().try_state::<FloatingWindows>() else {
        return;
    };
    match event {
        tauri::WindowEvent::Moved(_)
        | tauri::WindowEvent::Resized(_)
        | tauri::WindowEvent::ScaleFactorChanged { .. } => service.changed(kind),
        tauri::WindowEvent::CloseRequested { api, .. } => {
            // Capture/save before any native window disappears; destroy() bypasses
            // CloseRequested, so programmatic closure cannot recurse here.
            api.prevent_close();
            let service = service.inner().clone();
            let app = window.app_handle().clone();
            tauri::async_runtime::spawn(async move {
                let action = close_action(kind);
                if let Err(error) = service.apply(action).await {
                    eprintln!("close window: {error}");
                }
                if matches!(kind, WindowKind::Main) {
                    app.exit(0);
                }
            });
        }
        _ => {}
    }
}
