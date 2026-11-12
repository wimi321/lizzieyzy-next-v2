use super::*;
use serde_json::json;

#[test]
fn defaults_match_java_config_and_maintainer_overrides() {
    // Config.java uiConfig opt*/createDefaultConfig, plus P2-06 overrides.
    // Menu.setMoveNumber maps allow-move-number 0/-1/positive to off/all/last N.
    // Bottom toolbar uses persisted ui.main-window-other[0] (default height 26).
    // kataScolreLead maps Java false to the komi-adjusted score, hence true here.
    assert_eq!(
        serde_json::to_value(DisplaySettings::default()).unwrap(),
        json!({
            "show_coordinates": true,
            "move_numbers": "None",
            "last_move_numbers": 1,
            "move_numbers_from_one": false,
            "branch_move_numbers_from_one": true,
            "show_all_branch_move_numbers": false,
            "move_rank_marks": "LastOne",
            "last_move_rank_marks": 3,
            "show_winrate_in_suggestion": true,
            "show_visits_in_suggestion": true,
            "show_score_in_suggestion": true,
            "show_score_as_diff": false,
            "show_suggestion_order": true,
            "winrate_always_black": false,
            "show_hover_variation": true,
            "max_suggestions": 10,
            "suggestion_max_red": false,
            "next_moves": "Evaluation",
            "next_move_min_visits": 30,
            "show_winrate_graph": true,
            "show_comment": true,
            "show_variation_graph": true,
            "show_sub_board": true,
            "show_information_panel": true,
            "show_status_bar": true,
            "show_top_toolbar": true,
            "show_bottom_toolbar": true,
            "winrate_graph_mode": "Both",
            "fill_winrate_graph": true,
            "show_blunder_bar": false,
            "score_lead_includes_komi": true,
            "confirm_new_board": true,
            "confirm_overwrite": true,
            "show_names_below_board": true,
        }),
    );
}

#[test]
fn missing_fields_use_defaults_but_explicit_values_and_modes_survive() {
    let display: DisplaySettings = serde_json::from_value(json!({
        "show_coordinates": false,
        "move_numbers": "LastN",
        "last_move_numbers": 24,
        "next_moves": "Simple",
        "show_winrate_graph": false,
    }))
    .unwrap();
    assert_eq!(
        display,
        DisplaySettings {
            show_coordinates: false,
            move_numbers: MoveNumberMode::LastN,
            last_move_numbers: 24,
            next_moves: NextMoveHint::Simple,
            show_winrate_graph: false,
            ..DisplaySettings::default()
        }
    );
    for mode in [
        MoveNumberMode::None,
        MoveNumberMode::LastOne,
        MoveNumberMode::LastFive,
        MoveNumberMode::LastTen,
        MoveNumberMode::All,
        MoveNumberMode::LastN,
    ] {
        let mut changed = display.clone();
        changed.move_numbers = mode;
        let restored: DisplaySettings =
            serde_json::from_value(serde_json::to_value(&changed).unwrap()).unwrap();
        assert_eq!(restored, changed);
    }
    for mode in [
        MoveRankMarkMode::None,
        MoveRankMarkMode::LastOne,
        MoveRankMarkMode::LastN,
        MoveRankMarkMode::All,
    ] {
        let mut changed = display.clone();
        changed.move_rank_marks = mode;
        let restored: DisplaySettings =
            serde_json::from_value(serde_json::to_value(&changed).unwrap()).unwrap();
        assert_eq!(restored, changed);
    }
    for (field, value) in [("move_numbers", "Unknown"), ("winrate_graph_mode", "None")] {
        assert!(serde_json::from_value::<DisplaySettings>(json!({field: value})).is_err());
    }
}

#[test]
fn numeric_ranges_preserve_java_limits_without_magic_disabled_counts() {
    let valid = DisplaySettings {
        last_move_numbers: 999_999_999,
        last_move_rank_marks: i32::MAX as u32,
        max_suggestions: i32::MAX as u32,
        next_move_min_visits: i32::MAX as u32,
        ..DisplaySettings::default()
    };
    valid.validate().unwrap();
    DisplaySettings {
        max_suggestions: 0,
        next_move_min_visits: 0,
        ..valid.clone()
    }
    .validate()
    .unwrap();
    for (field, value) in [
        ("last_move_numbers", 0),
        ("last_move_numbers", 1_000_000_000),
        ("last_move_rank_marks", 0),
        ("last_move_rank_marks", i32::MAX as u32 + 1),
        ("max_suggestions", i32::MAX as u32 + 1),
        ("next_move_min_visits", i32::MAX as u32 + 1),
    ] {
        let mut json = serde_json::to_value(&valid).unwrap();
        json[field] = value.into();
        let display: DisplaySettings = serde_json::from_value(json).unwrap();
        assert!(
            matches!(display.validate(), Err(DisplaySettingsError::OutOfRange { field: actual, .. }) if actual == field)
        );
    }
    for value in [json!(-1), json!(1.5), json!(4_294_967_296u64)] {
        assert!(
            serde_json::from_value::<DisplaySettings>(json!({"last_move_numbers": value})).is_err()
        );
    }
}
