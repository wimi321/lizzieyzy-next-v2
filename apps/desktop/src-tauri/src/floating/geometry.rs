//! Windows converts logical window positions using the current window DPI.
//! Select the destination display before converting a saved logical placement.
use app_core::WindowBounds;
use tauri::{LogicalPosition, LogicalSize, PhysicalPosition, PhysicalSize};

pub(super) fn target_geometry(
    bounds: WindowBounds,
    frame: (u32, u32),
    screens: &[(WindowBounds, f64)],
    fallback_scale: f64,
) -> (PhysicalPosition<i32>, PhysicalSize<u32>) {
    let mut selected = None;
    let mut best_overlap = -1.0;
    let mut best_distance = f64::INFINITY;
    for &(area, scale) in screens {
        if area.width == 0 || area.height == 0 || !scale.is_finite() || scale <= 0.0 {
            continue;
        }
        let width = (f64::from(bounds.x) + f64::from(bounds.width))
            .min(f64::from(area.x) + f64::from(area.width))
            - f64::from(bounds.x.max(area.x));
        let height = (f64::from(bounds.y) + f64::from(bounds.height))
            .min(f64::from(area.y) + f64::from(area.height))
            - f64::from(bounds.y.max(area.y));
        let overlap = width.max(0.0) * height.max(0.0);
        let dx = f64::from(bounds.x) - f64::from(area.x)
            + (f64::from(bounds.width) - f64::from(area.width)) / 2.0;
        let dy = f64::from(bounds.y) - f64::from(area.y)
            + (f64::from(bounds.height) - f64::from(area.height)) / 2.0;
        let distance = dx * dx + dy * dy;
        if overlap > best_overlap
            || (overlap > 0.0 && overlap == best_overlap && distance < best_distance)
        {
            selected = Some(scale);
            best_overlap = overlap;
            best_distance = distance;
        }
    }
    let scale = selected.unwrap_or(fallback_scale);
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let position = LogicalPosition::new(bounds.x, bounds.y).to_physical(scale);
    let size = LogicalSize::new(
        bounds.width.saturating_sub(frame.0).max(1),
        bounds.height.saturating_sub(frame.1).max(1),
    )
    .to_physical(scale);
    (position, size)
}

#[cfg(test)]
#[path = "geometry/tests.rs"]
mod tests;
