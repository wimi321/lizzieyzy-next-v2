//! Persisted rendering preferences. These never change core analysis perspective.

use crate::{AppCore, AppError, SettingsError, actor};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use ts_rs::TS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum MoveNumberMode {
    None,
    LastOne,
    LastFive,
    LastTen,
    All,
    LastN,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum MoveRankMarkMode {
    None,
    LastOne,
    LastN,
    All,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum NextMoveHint {
    None,
    Simple,
    Evaluation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum WinrateGraphMode {
    Winrate,
    ScoreLead,
    Both,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct DisplaySettings {
    #[ts(as = "Option<bool>", optional)]
    pub show_coordinates: bool,
    #[ts(as = "Option<MoveNumberMode>", optional)]
    pub move_numbers: MoveNumberMode,
    /// Retained custom N even when a fixed preset or None is selected.
    #[ts(as = "Option<u32>", optional)]
    pub last_move_numbers: u32,
    #[ts(as = "Option<bool>", optional)]
    pub move_numbers_from_one: bool,
    #[ts(as = "Option<bool>", optional)]
    pub branch_move_numbers_from_one: bool,
    #[ts(as = "Option<bool>", optional)]
    pub show_all_branch_move_numbers: bool,
    #[ts(as = "Option<MoveRankMarkMode>", optional)]
    pub move_rank_marks: MoveRankMarkMode,
    #[ts(as = "Option<u32>", optional)]
    pub last_move_rank_marks: u32,
    #[ts(as = "Option<bool>", optional)]
    pub show_winrate_in_suggestion: bool,
    #[ts(as = "Option<bool>", optional)]
    pub show_visits_in_suggestion: bool,
    #[ts(as = "Option<bool>", optional)]
    pub show_score_in_suggestion: bool,
    #[ts(as = "Option<bool>", optional)]
    pub show_score_as_diff: bool,
    #[ts(as = "Option<bool>", optional)]
    pub show_suggestion_order: bool,
    #[ts(as = "Option<bool>", optional)]
    pub winrate_always_black: bool,
    #[ts(as = "Option<bool>", optional)]
    pub show_hover_variation: bool,
    /// Java's nonpositive limit means unlimited; the canonical value is zero.
    #[ts(as = "Option<u32>", optional)]
    pub max_suggestions: u32,
    #[ts(as = "Option<bool>", optional)]
    pub suggestion_max_red: bool,
    #[ts(as = "Option<NextMoveHint>", optional)]
    pub next_moves: NextMoveHint,
    /// Zero disables the minimum-visits filter.
    #[ts(as = "Option<u32>", optional)]
    pub next_move_min_visits: u32,
    #[ts(as = "Option<bool>", optional)]
    pub show_winrate_graph: bool,
    #[ts(as = "Option<bool>", optional)]
    pub show_comment: bool,
    #[ts(as = "Option<bool>", optional)]
    pub show_variation_graph: bool,
    #[ts(as = "Option<bool>", optional)]
    pub show_sub_board: bool,
    #[ts(as = "Option<bool>", optional)]
    pub show_information_panel: bool,
    #[ts(as = "Option<bool>", optional)]
    pub show_status_bar: bool,
    #[ts(as = "Option<bool>", optional)]
    pub show_top_toolbar: bool,
    #[ts(as = "Option<bool>", optional)]
    pub show_bottom_toolbar: bool,
    #[ts(as = "Option<WinrateGraphMode>", optional)]
    pub winrate_graph_mode: WinrateGraphMode,
    #[ts(as = "Option<bool>", optional)]
    pub fill_winrate_graph: bool,
    #[ts(as = "Option<bool>", optional)]
    pub show_blunder_bar: bool,
    /// Inverse of Java's misleading show-katago-score-lead-with-komi flag:
    /// false there selects the komi-adjusted lead; true selects board score.
    #[ts(as = "Option<bool>", optional)]
    pub score_lead_includes_komi: bool,
    #[ts(as = "Option<bool>", optional)]
    pub confirm_new_board: bool,
    #[ts(as = "Option<bool>", optional)]
    pub confirm_overwrite: bool,
    #[ts(as = "Option<bool>", optional)]
    pub show_names_below_board: bool,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            show_coordinates: true,
            move_numbers: MoveNumberMode::None,
            last_move_numbers: 1,
            move_numbers_from_one: false,
            branch_move_numbers_from_one: true,
            show_all_branch_move_numbers: false,
            move_rank_marks: MoveRankMarkMode::LastOne,
            last_move_rank_marks: 3,
            show_winrate_in_suggestion: true,
            show_visits_in_suggestion: true,
            show_score_in_suggestion: true,
            show_score_as_diff: false,
            show_suggestion_order: true,
            winrate_always_black: false,
            show_hover_variation: true,
            max_suggestions: 10,
            suggestion_max_red: false,
            next_moves: NextMoveHint::Evaluation,
            next_move_min_visits: 30,
            show_winrate_graph: true,
            show_comment: true,
            show_variation_graph: true,
            show_sub_board: true,
            show_information_panel: true,
            show_status_bar: true,
            show_top_toolbar: true,
            show_bottom_toolbar: true,
            winrate_graph_mode: WinrateGraphMode::Both,
            fill_winrate_graph: true,
            show_blunder_bar: false,
            score_lead_includes_komi: true,
            confirm_new_board: true,
            confirm_overwrite: true,
            show_names_below_board: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DisplaySettingsError {
    #[error("{field} must be in {minimum}..={maximum}")]
    OutOfRange {
        field: &'static str,
        minimum: u32,
        maximum: u32,
    },
}

impl DisplaySettings {
    pub fn validate(&self) -> Result<(), DisplaySettingsError> {
        // MovenumberDialog accepts positive decimal values with fewer than ten
        // digits; the other Java controls parse signed 32-bit integers.
        for (field, value, minimum, maximum) in [
            ("last_move_numbers", self.last_move_numbers, 1, 999_999_999),
            (
                "last_move_rank_marks",
                self.last_move_rank_marks,
                1,
                i32::MAX as u32,
            ),
            ("max_suggestions", self.max_suggestions, 0, i32::MAX as u32),
            (
                "next_move_min_visits",
                self.next_move_min_visits,
                0,
                i32::MAX as u32,
            ),
        ] {
            if !(minimum..=maximum).contains(&value) {
                return Err(DisplaySettingsError::OutOfRange {
                    field,
                    minimum,
                    maximum,
                });
            }
        }
        Ok(())
    }
}

impl AppCore {
    /// Replace the entire display object after validation and atomic storage.
    pub async fn set_display_settings(
        &self,
        display: DisplaySettings,
    ) -> Result<DisplaySettings, AppError> {
        display.validate().map_err(SettingsError::from)?;
        let (reply, response) = oneshot::channel();
        self.commands
            .send(actor::Command::SetDisplaySettings(display, reply))
            .await
            .map_err(|_| AppError::Closed)?;
        response.await.map_err(|_| AppError::Closed)?
    }
}

#[cfg(test)]
mod tests;
