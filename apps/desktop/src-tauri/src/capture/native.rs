//! The only screen/permission APIs; never called by automated tests.
use app_core::CaptureImage;
use tauri::{Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

use super::{CapturePermission, Display, Platform, Rect, ScreenCaptureError, failure};

pub(super) struct Native<R: Runtime>(pub tauri::AppHandle<R>);

pub(crate) fn permission() -> CapturePermission {
    #[cfg(target_os = "macos")]
    {
        if core_graphics::access::ScreenCaptureAccess.preflight() {
            CapturePermission::Granted
        } else {
            CapturePermission::Missing
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        CapturePermission::NotNeeded
    }
}

pub(crate) fn request_permission() -> CapturePermission {
    #[cfg(target_os = "macos")]
    {
        if core_graphics::access::ScreenCaptureAccess.request() {
            CapturePermission::Granted
        } else {
            CapturePermission::Missing
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        CapturePermission::NotNeeded
    }
}

pub(crate) fn open_permission_settings() -> Result<(), ScreenCaptureError> {
    #[cfg(target_os = "macos")]
    {
        tauri_plugin_opener::open_url(
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
            None::<&str>,
        )
        .map_err(failure)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(ScreenCaptureError::Unsupported)
    }
}

pub(super) fn capture_url(session_id: u32) -> WebviewUrl {
    WebviewUrl::App(format!("index.html?window=capture&session={session_id}").into())
}

impl<R: Runtime> Native<R> {
    fn window(&self, label: &str) -> Result<tauri::WebviewWindow<R>, ScreenCaptureError> {
        self.0
            .get_webview_window(label)
            .ok_or_else(|| failure(format!("window {label} is unavailable")))
    }
}

impl<R: Runtime> Platform for Native<R> {
    fn permission(&self) -> CapturePermission {
        permission()
    }

    fn supported(&self) -> Result<(), ScreenCaptureError> {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            Ok(())
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            Err(ScreenCaptureError::Unsupported)
        }
    }

    fn displays(&self) -> Result<Vec<Display>, ScreenCaptureError> {
        #[cfg(target_os = "macos")]
        {
            use core_graphics::display::CGDisplay;
            CGDisplay::active_displays()
                .map_err(|error| failure(format!("display enumeration: {error}")))?
                .into_iter()
                .map(|id| {
                    let bounds = CGDisplay::new(id).bounds();
                    Ok(Display {
                        id,
                        bounds: Rect {
                            x: bounds.origin.x,
                            y: bounds.origin.y,
                            width: bounds.size.width,
                            height: bounds.size.height,
                        },
                    })
                })
                .collect()
        }
        #[cfg(target_os = "windows")]
        {
            super::xcap_backend::displays().map(|displays| {
                displays
                    .into_iter()
                    .map(|display| Display {
                        id: display.id,
                        bounds: display.physical_bounds,
                    })
                    .collect()
            })
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            Err(ScreenCaptureError::Unsupported)
        }
    }

    fn main_bounds(&self) -> Result<Rect, ScreenCaptureError> {
        let window = self.window("main")?;
        #[cfg(target_os = "windows")]
        {
            // Compare the main-window center with all monitor rectangles in
            // physical desktop coordinates. Per-monitor logical origins are
            // not a common coordinate space on mixed-DPI Windows desktops.
            let position = window.outer_position().map_err(failure)?;
            let size = window.outer_size().map_err(failure)?;
            Ok(Rect {
                x: f64::from(position.x),
                y: f64::from(position.y),
                width: f64::from(size.width),
                height: f64::from(size.height),
            })
        }
        #[cfg(not(target_os = "windows"))]
        {
            let scale = window.scale_factor().map_err(failure)?;
            if !scale.is_finite() || scale <= 0.0 {
                return Err(ScreenCaptureError::DisplayUnavailable);
            }
            let position = window
                .outer_position()
                .map_err(failure)?
                .to_logical::<f64>(scale);
            let size = window
                .outer_size()
                .map_err(failure)?
                .to_logical::<f64>(scale);
            Ok(Rect {
                x: position.x,
                y: position.y,
                width: size.width,
                height: size.height,
            })
        }
    }

    fn logical_bounds(&self, display: Display) -> Result<Rect, ScreenCaptureError> {
        #[cfg(target_os = "windows")]
        {
            super::xcap_backend::logical_bounds(&super::xcap_backend::display(display.id)?)
        }
        #[cfg(not(target_os = "windows"))]
        {
            Ok(display.bounds)
        }
    }

    fn visible(&self, label: &str) -> Result<bool, ScreenCaptureError> {
        self.0
            .get_webview_window(label)
            .map_or(Ok(false), |window| window.is_visible().map_err(failure))
    }

    fn hide(&self, label: &str) -> Result<(), ScreenCaptureError> {
        self.window(label)?.hide().map_err(failure)
    }

    fn show(&self, label: &str) -> Result<(), ScreenCaptureError> {
        let window = self.window(label)?;
        window.show().map_err(failure)?;
        if label == "main" {
            window.unminimize().map_err(failure)?;
        }
        Ok(())
    }

    fn focus(&self, label: &str) -> Result<(), ScreenCaptureError> {
        self.window(label)?.set_focus().map_err(failure)
    }

    fn close_capture(&self) -> Result<(), ScreenCaptureError> {
        self.0
            .get_webview_window("capture")
            .map_or(Ok(()), |window| window.destroy().map_err(failure))
    }

    fn open_capture(
        &self,
        display: Display,
        session_id: u32,
        on_close: Box<dyn Fn() + Send + Sync>,
    ) -> Result<(), ScreenCaptureError> {
        let bounds = display.bounds;
        let window = WebviewWindowBuilder::new(&self.0, "capture", capture_url(session_id))
            .title("LizzieYzy Next 2")
            .decorations(false)
            .always_on_top(true)
            .resizable(false)
            .maximizable(false)
            .minimizable(false)
            .visible(false)
            .inner_size(bounds.width, bounds.height)
            .position(bounds.x, bounds.y)
            .build()
            .map_err(failure)?;
        #[cfg(target_os = "windows")]
        {
            window
                .set_position(tauri::PhysicalPosition::new(
                    bounds.x as i32,
                    bounds.y as i32,
                ))
                .map_err(failure)?;
            window
                .set_size(tauri::PhysicalSize::new(
                    bounds.width as u32,
                    bounds.height as u32,
                ))
                .map_err(failure)?;
        }
        // The callback belongs to this native window and captures its session
        // identity. A late close/destroy cannot cancel a replacement window.
        window.on_window_event(move |event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                on_close();
            }
            tauri::WindowEvent::Destroyed => on_close(),
            _ => {}
        });
        // On macOS, simple fullscreen covers the monitor without changing
        // Spaces; the window was first positioned on the chosen display.
        window.set_simple_fullscreen(true).map_err(failure)?;
        window.show().map_err(failure)?;
        self.focus("capture")
    }

    fn capture(&self, display: Display) -> Result<CaptureImage, ScreenCaptureError> {
        capture_display(display)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn capture_display(_display: Display) -> Result<CaptureImage, ScreenCaptureError> {
    Err(ScreenCaptureError::Unsupported)
}

#[cfg(target_os = "macos")]
fn capture_display(display: Display) -> Result<CaptureImage, ScreenCaptureError> {
    super::macos::capture(display)
}

#[cfg(target_os = "windows")]
fn capture_display(display: Display) -> Result<CaptureImage, ScreenCaptureError> {
    super::xcap_backend::capture(display.id)
}
