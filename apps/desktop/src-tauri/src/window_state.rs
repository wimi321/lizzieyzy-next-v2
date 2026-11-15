//! Platform-independent placement and trailing-edge debounce policy.
use std::collections::BTreeMap;

use app_core::{WindowBounds, WindowKind, WindowPlacement, fit_window_bounds};
use tokio::time::{Duration, Instant};

pub(crate) const DEBOUNCE: Duration = Duration::from_millis(500);

pub(crate) fn label(kind: WindowKind) -> &'static str {
    match kind {
        WindowKind::Main => "main",
        WindowKind::FloatingMain => "float-main",
        WindowKind::FloatingSub => "float-sub",
    }
}

pub(crate) fn kind(label: &str) -> Option<WindowKind> {
    match label {
        "main" => Some(WindowKind::Main),
        "float-main" => Some(WindowKind::FloatingMain),
        "float-sub" => Some(WindowKind::FloatingSub),
        _ => None,
    }
}

pub(crate) fn minimum(kind: WindowKind) -> (u32, u32) {
    match kind {
        WindowKind::Main => (520, 400),
        _ => (100, 100),
    }
}

pub(crate) fn default_bounds(kind: WindowKind, areas: &[WindowBounds]) -> WindowBounds {
    let (width, height) = match kind {
        WindowKind::Main => (1280, 820),
        WindowKind::FloatingMain => (600, 600),
        WindowKind::FloatingSub => (300, 300),
    };
    let area = areas.first().copied().unwrap_or(WindowBounds {
        x: 0,
        y: 0,
        width: 1280,
        height: 820,
    });
    let width = width.min(area.width);
    let height = height.min(area.height);
    let x = match kind {
        WindowKind::Main => area.x + ((area.width - width) / 2) as i32,
        WindowKind::FloatingMain => area.x + (area.width - width) as i32,
        WindowKind::FloatingSub => area.x,
    };
    fit_window_bounds(
        WindowBounds {
            x,
            y: area.y + ((area.height - height) / 2) as i32,
            width,
            height,
        },
        areas,
        minimum(kind),
    )
}

/// Preserve normal bounds while maximized/minimized native resize events arrive.
#[derive(Clone, Default)]
pub(crate) struct Placements {
    normal: BTreeMap<&'static str, WindowBounds>,
    dirty: BTreeMap<&'static str, (WindowKind, Instant)>,
}

impl Placements {
    pub(crate) fn observe(
        &mut self,
        kind: WindowKind,
        mut value: WindowPlacement,
    ) -> WindowPlacement {
        if value.maximized {
            if let Some(bounds) = self.normal.get(label(kind)) {
                value.bounds = *bounds;
            }
        } else {
            self.normal.insert(label(kind), value.bounds);
        }
        value
    }

    pub(crate) fn seed(&mut self, kind: WindowKind, bounds: WindowBounds) {
        self.normal.insert(label(kind), bounds);
    }

    pub(crate) fn changed(&mut self, kind: WindowKind) {
        self.dirty
            .insert(label(kind), (kind, Instant::now() + DEBOUNCE));
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.dirty.values().map(|(_, deadline)| *deadline).min()
    }

    pub(crate) fn due(&mut self) -> Vec<WindowKind> {
        let now = Instant::now();
        let mut due = Vec::new();
        self.dirty.retain(|_, (kind, deadline)| {
            if *deadline <= now {
                due.push(*kind);
                false
            } else {
                true
            }
        });
        due
    }

    pub(crate) fn clear(&mut self) {
        self.dirty.clear();
    }
}

#[cfg(test)]
mod tests;
