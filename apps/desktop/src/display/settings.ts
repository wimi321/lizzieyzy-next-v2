import type { DisplaySettings, MoveNumberMode, MoveRankMarkMode } from "@/api/backend";
import type { MoveWindow } from "../board/moveMarks";

export type Display = Required<DisplaySettings>;

/**
 * What the 显示 menu starts with. Rust owns the settings and always sends every field; these
 * only stand in for fields a backend older than the interface leaves out.
 */
export const DISPLAY_DEFAULTS: Display = {
  show_coordinates: true,
  move_numbers: "None",
  last_move_numbers: 1,
  move_numbers_from_one: false,
  branch_move_numbers_from_one: true,
  show_all_branch_move_numbers: false,
  move_rank_marks: "LastOne",
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
  next_moves: "Evaluation",
  next_move_min_visits: 30,
  show_winrate_graph: true,
  show_comment: true,
  show_variation_graph: true,
  show_information_panel: true,
  show_status_bar: true,
  show_top_toolbar: true,
  show_bottom_toolbar: true,
  show_sub_board: true,
  winrate_graph_mode: "Both",
  fill_winrate_graph: true,
  show_blunder_bar: false,
  score_lead_includes_komi: true,
  confirm_new_board: true,
  confirm_overwrite: true,
  show_names_below_board: true,
};

export function filled(display: DisplaySettings | null | undefined): Display {
  const result: Record<string, unknown> = { ...DISPLAY_DEFAULTS };
  for (const [key, value] of Object.entries(display ?? {})) if (value !== undefined) result[key] = value;
  return result as Display;
}

/** The move-number setting as a count of latest moves: 0 none, -1 all. */
export function numberWindow(display: Display): MoveWindow {
  const windows: Record<MoveNumberMode, MoveWindow> = { None: 0, LastOne: 1, LastFive: 5, LastTen: 10, All: -1, LastN: display.last_move_numbers };
  return windows[display.move_numbers];
}

/** The rank-mark setting as a count of latest moves: 0 none, -1 all. */
export function rankWindow(display: Display): MoveWindow {
  const windows: Record<MoveRankMarkMode, MoveWindow> = { None: 0, LastOne: 1, LastN: display.last_move_rank_marks, All: -1 };
  return windows[display.move_rank_marks];
}
