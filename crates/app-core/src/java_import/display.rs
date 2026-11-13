use crate::{AppError, DisplaySettings, SettingsError};
use serde_json::{Value, json};

pub(super) fn merge(
    ui: &Value,
    leelaz: &Value,
    persisted: &Value,
    display: &mut DisplaySettings,
    issues: &mut Vec<String>,
) -> Result<(), AppError> {
    let mut value = serde_json::to_value(&*display).map_err(SettingsError::from)?;
    for (java, field) in [
        ("show-coordinates", "show_coordinates"),
        ("movenumber-from-one", "move_numbers_from_one"),
        ("new-move-number-in-branch", "branch_move_numbers_from_one"),
        ("show-moveall-inbranch", "show_all_branch_move_numbers"),
        ("show-winrate-in-suggestion", "show_winrate_in_suggestion"),
        ("show-playouts-in-suggestion", "show_visits_in_suggestion"),
        ("show-scoremean-in-suggestion", "show_score_in_suggestion"),
        ("show-score-as-diff", "show_score_as_diff"),
        ("show-suggestion-order", "show_suggestion_order"),
        ("win-rate-always-black", "winrate_always_black"),
        ("show-suggestion-variations", "show_hover_variation"),
        ("show-suggestion-maxred", "suggestion_max_red"),
        ("show-winrate-graph", "show_winrate_graph"),
        ("show-comment", "show_comment"),
        ("show-variation-graph", "show_variation_graph"),
        ("show-subboard", "show_sub_board"),
        ("show-list-pane", "show_list_pane"),
        ("show-captured", "show_information_panel"),
        ("show-status", "show_status_bar"),
        ("show-edit-bar", "show_top_toolbar"),
        ("show-winrate-graph-fill", "fill_winrate_graph"),
        ("show-blunder-bar", "show_blunder_bar"),
        ("show-new-board-hint", "confirm_new_board"),
        ("show-replace-file-hint", "confirm_overwrite"),
        ("show-name-in-board", "show_names_below_board"),
        ("min-playouts-for-next-move", "next_move_min_visits"),
        ("txt-move-rank-mark-last-move", "last_move_rank_marks"),
    ] {
        if let Some(source) = ui.get(java) {
            value[field] = source.clone();
        }
    }
    if let Some(source) = leelaz.get("limit-max-suggestion") {
        let count = integer(source, "limit-max-suggestion")?;
        value["max_suggestions"] = json!(count.max(0));
    }
    if let Some(source) = ui.get("show-katago-score-lead-with-komi") {
        value["score_lead_includes_komi"] =
            json!(!boolean(source, "show-katago-score-lead-with-komi")?);
    }
    if let Some(source) = ui.get("only-last-move-number") {
        let count = integer(source, "only-last-move-number")?;
        if count > 0 {
            value["last_move_numbers"] = json!(count);
        }
    }
    let count = if let Some(source) = ui.get("allow-move-number") {
        Some(integer(source, "allow-move-number")?)
    } else if let Some(source) = ui.get("show-move-number") {
        if boolean(source, "show-move-number")? {
            Some(
                ui.get("only-last-move-number")
                    .map(|value| integer(value, "only-last-move-number"))
                    .transpose()?
                    .filter(|count| *count > 0)
                    .unwrap_or(-1),
            )
        } else {
            Some(0)
        }
    } else {
        None
    };
    if let Some(count) = count {
        value["move_numbers"] = json!(match count {
            0 => "None",
            -1 => "All",
            1 => "LastOne",
            5 => "LastFive",
            10 => "LastTen",
            2.. => "LastN",
            _ => return Err(super::invalid("invalid allow-move-number")),
        });
        if count > 0 {
            value["last_move_numbers"] = json!(count);
        }
    }
    if let Some(source) = ui.get("move-rank-mark-last-move") {
        let count = integer(source, "move-rank-mark-last-move")?;
        value["move_rank_marks"] = json!(match count {
            -1 => "None",
            0 => "All",
            1 => "LastOne",
            2.. => "LastN",
            _ => return Err(super::invalid("invalid move-rank-mark-last-move")),
        });
        if count > 1 {
            value["last_move_rank_marks"] = json!(count);
        }
    }
    if ui.get("show-next-moves").is_some() || ui.get("show-next-move-blunder").is_some() {
        let shown = ui
            .get("show-next-moves")
            .map(|value| boolean(value, "show-next-moves"))
            .transpose()?
            .unwrap_or(true);
        let evaluation = ui
            .get("show-next-move-blunder")
            .map(|value| boolean(value, "show-next-move-blunder"))
            .transpose()?
            .unwrap_or(true);
        value["next_moves"] = json!(if !shown {
            "None"
        } else if evaluation {
            "Evaluation"
        } else {
            "Simple"
        });
    }
    if ui.get("show-win-rate-line").is_some() || ui.get("show-score-lead-line").is_some() {
        let winrate = ui
            .get("show-win-rate-line")
            .map(|value| boolean(value, "show-win-rate-line"))
            .transpose()?
            .unwrap_or(true);
        let score = ui
            .get("show-score-lead-line")
            .map(|value| boolean(value, "show-score-lead-line"))
            .transpose()?
            .unwrap_or(true);
        match (winrate, score) {
            (true, true) => value["winrate_graph_mode"] = json!("Both"),
            (true, false) => value["winrate_graph_mode"] = json!("Winrate"),
            (false, true) => value["winrate_graph_mode"] = json!("ScoreLead"),
            (false, false) => issues.push("both Java graph lines are hidden; existing graph mode retained because the new mode has no None variant".into()),
        }
    }
    if let Some(height) = persisted["ui-persist"]["main-window-other"].get(0) {
        value["show_bottom_toolbar"] =
            json!(integer(height, "persist.ui-persist.main-window-other[0]")? > 0);
    }
    let imported: DisplaySettings = serde_json::from_value(value).map_err(SettingsError::from)?;
    imported.validate().map_err(SettingsError::from)?;
    *display = imported;
    Ok(())
}

pub(super) fn integer(value: &Value, key: &str) -> Result<i64, AppError> {
    value
        .as_i64()
        .ok_or_else(|| super::invalid(format!("{key} must be an integer")))
}

pub(super) fn boolean(value: &Value, key: &str) -> Result<bool, AppError> {
    value
        .as_bool()
        .ok_or_else(|| super::invalid(format!("{key} must be a boolean")))
}
