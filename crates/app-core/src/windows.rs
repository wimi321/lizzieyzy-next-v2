//! Layout snapshots and logical-pixel geometry, independent of a window runtime.
use crate::DisplaySettingsError;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

mod loading;
pub(crate) use loading::recover_positions;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum LayoutMode {
    #[default]
    Normal,
    Min,
    Thinking,
    MultiView,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum FloatingBoardKind {
    Main,
    Sub,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowKind {
    Main,
    FloatingMain,
    FloatingSub,
}

/// Logical pixels; negative origins allow monitors left of or above the primary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WindowBounds {
    #[ts(as = "Option<i32>", optional)]
    pub x: i32,
    #[ts(as = "Option<i32>", optional)]
    pub y: i32,
    #[ts(as = "Option<u32>", optional)]
    pub width: u32,
    #[ts(as = "Option<u32>", optional)]
    pub height: u32,
}

impl WindowBounds {
    pub fn validate(&self) -> Result<(), DisplaySettingsError> {
        for (field, value, minimum, maximum) in [
            ("x", i64::from(self.x), -100_000, 100_000),
            ("y", i64::from(self.y), -100_000, 100_000),
            ("width", i64::from(self.width), 1, 100_000),
            ("height", i64::from(self.height), 1, 100_000),
        ] {
            if !(i64::from(minimum)..=i64::from(maximum)).contains(&value) {
                return Err(DisplaySettingsError::InvalidWindowBounds {
                    field,
                    minimum,
                    maximum,
                });
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WindowPlacement {
    #[ts(as = "Option<WindowBounds>", optional)]
    pub bounds: WindowBounds,
    #[serde(default)]
    #[ts(as = "Option<bool>", optional)]
    pub maximized: bool,
}

impl Default for WindowPlacement {
    fn default() -> Self {
        Self {
            bounds: WindowBounds {
                x: 0,
                y: 0,
                width: 1280,
                height: 820,
            },
            maximized: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct FloatingBoardsView {
    #[ts(as = "Option<bool>", optional)]
    pub main: bool,
    #[ts(as = "Option<bool>", optional)]
    pub keep_main_board: bool,
    #[ts(as = "Option<bool>", optional)]
    pub sub: bool,
    #[ts(as = "Option<bool>", optional)]
    pub main_on_top: bool,
    #[ts(as = "Option<bool>", optional)]
    pub sub_on_top: bool,
    #[ts(as = "Option<bool>", optional)]
    pub main_locked: bool,
    #[ts(as = "Option<bool>", optional)]
    pub sub_locked: bool,
}

/// The four window-owned fields, committed together after native operations.
/// Other display preferences are deliberately outside this transaction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct WindowLayout {
    #[ts(optional = nullable)]
    pub main_window: Option<WindowPlacement>,
    #[ts(as = "Option<FloatingBoardsView>", optional)]
    pub floating_boards: FloatingBoardsView,
    #[ts(optional = nullable)]
    pub floating_main: Option<WindowBounds>,
    #[ts(optional = nullable)]
    pub floating_sub: Option<WindowBounds>,
}

impl From<&crate::DisplaySettings> for WindowLayout {
    fn from(display: &crate::DisplaySettings) -> Self {
        Self {
            main_window: display.main_window,
            floating_boards: display.floating_boards,
            floating_main: display.floating_main,
            floating_sub: display.floating_sub,
        }
    }
}

impl WindowLayout {
    pub(crate) fn apply_to(self, display: &mut crate::DisplaySettings) {
        display.main_window = self.main_window;
        display.floating_boards = self.floating_boards;
        display.floating_main = self.floating_main;
        display.floating_sub = self.floating_sub;
    }

    pub fn validate(&self) -> Result<(), DisplaySettingsError> {
        for bounds in [
            self.main_window.map(|value| value.bounds),
            self.floating_main,
            self.floating_sub,
        ]
        .into_iter()
        .flatten()
        {
            bounds.validate()?;
        }
        Ok(())
    }
}

/// The seven content panels; toolbars remain independent display preferences.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct LayoutPanels {
    #[ts(as = "Option<bool>", optional)]
    pub show_winrate_graph: bool,
    #[ts(as = "Option<bool>", optional)]
    pub show_comment: bool,
    #[ts(as = "Option<bool>", optional)]
    pub show_variation_graph: bool,
    #[ts(as = "Option<bool>", optional)]
    pub show_sub_board: bool,
    #[ts(as = "Option<bool>", optional)]
    pub show_list_pane: bool,
    #[ts(as = "Option<bool>", optional)]
    pub show_information_panel: bool,
    #[ts(as = "Option<bool>", optional)]
    pub show_status_bar: bool,
}

impl Default for LayoutPanels {
    fn default() -> Self {
        Self {
            show_winrate_graph: true,
            show_comment: true,
            show_variation_graph: true,
            show_sub_board: true,
            show_list_pane: true,
            show_information_panel: true,
            show_status_bar: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct CustomLayout {
    #[ts(as = "Option<LayoutPanels>", optional)]
    pub panels: LayoutPanels,
    #[ts(as = "Option<LayoutMode>", optional)]
    pub layout_mode: LayoutMode,
    #[ts(as = "Option<bool>", optional)]
    pub large_sub_board: bool,
    #[ts(as = "Option<bool>", optional)]
    pub large_winrate_graph: bool,
    #[ts(as = "Option<u32>", optional)]
    pub board_position: u32,
    #[ts(as = "Option<WindowPlacement>", optional)]
    pub main_window: WindowPlacement,
    #[ts(as = "Option<FloatingBoardsView>", optional)]
    pub floating_boards: FloatingBoardsView,
    #[ts(optional = nullable)]
    pub floating_main: Option<WindowBounds>,
    #[ts(optional = nullable)]
    pub floating_sub: Option<WindowBounds>,
}

impl Default for CustomLayout {
    fn default() -> Self {
        Self {
            panels: Default::default(),
            layout_mode: LayoutMode::Normal,
            large_sub_board: false,
            large_winrate_graph: false,
            board_position: 4,
            main_window: Default::default(),
            floating_boards: Default::default(),
            floating_main: None,
            floating_sub: None,
        }
    }
}

impl CustomLayout {
    pub fn validate(&self) -> Result<(), DisplaySettingsError> {
        validate_layout(
            self.large_sub_board,
            self.large_winrate_graph,
            self.board_position,
        )?;
        self.main_window.bounds.validate()?;
        for bounds in [self.floating_main, self.floating_sub]
            .into_iter()
            .flatten()
        {
            bounds.validate()?;
        }
        Ok(())
    }
}

pub(crate) fn validate_layout(
    sub: bool,
    graph: bool,
    position: u32,
) -> Result<(), DisplaySettingsError> {
    if sub && graph {
        return Err(DisplaySettingsError::ConflictingEnlargement);
    }
    if position > 8 {
        return Err(DisplaySettingsError::OutOfRange {
            field: "board_position",
            minimum: 0,
            maximum: 8,
        });
    }
    Ok(())
}

/// The first valid work area is the primary monitor. Overlap wins, then nearest
/// center; entirely off-screen windows return to the primary monitor. If a work
/// area is smaller than the minimum, fitting inside it takes precedence.
pub fn fit_window_bounds(
    bounds: WindowBounds,
    work_areas: &[WindowBounds],
    minimum: (u32, u32),
) -> WindowBounds {
    let mut valid = work_areas
        .iter()
        .filter(|area| area.width > 0 && area.height > 0);
    let Some(primary) = valid.next() else {
        return bounds;
    };
    let mut target = primary;
    let mut best_overlap = overlap(bounds, *primary);
    let mut best_distance = center_distance(bounds, *primary);
    for candidate in valid {
        let area = overlap(bounds, *candidate);
        let distance = center_distance(bounds, *candidate);
        if area > best_overlap || (area > 0 && area == best_overlap && distance < best_distance) {
            target = candidate;
            best_overlap = area;
            best_distance = distance;
        }
    }
    let width = bounds.width.max(minimum.0.max(1)).min(target.width);
    let height = bounds.height.max(minimum.1.max(1)).min(target.height);
    let x = i64::from(bounds.x).clamp(
        i64::from(target.x),
        i64::from(target.x) + i64::from(target.width - width),
    );
    let y = i64::from(bounds.y).clamp(
        i64::from(target.y),
        i64::from(target.y) + i64::from(target.height - height),
    );
    WindowBounds {
        x: x.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        y: y.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        width,
        height,
    }
}

fn overlap(a: WindowBounds, b: WindowBounds) -> i128 {
    let width = (i64::from(a.x) + i64::from(a.width)).min(i64::from(b.x) + i64::from(b.width))
        - i64::from(a.x.max(b.x));
    let height = (i64::from(a.y) + i64::from(a.height)).min(i64::from(b.y) + i64::from(b.height))
        - i64::from(a.y.max(b.y));
    i128::from(width.max(0)) * i128::from(height.max(0))
}

fn center_distance(a: WindowBounds, b: WindowBounds) -> i128 {
    let dx = 2 * (i128::from(a.x) - i128::from(b.x)) + i128::from(a.width) - i128::from(b.width);
    let dy = 2 * (i128::from(a.y) - i128::from(b.y)) + i128::from(a.height) - i128::from(b.height);
    dx * dx + dy * dy
}

#[cfg(test)]
mod tests;
