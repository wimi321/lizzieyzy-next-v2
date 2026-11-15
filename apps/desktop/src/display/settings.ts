import type { DisplaySettings, GraphPointView, MoveNumberMode, MoveRankMarkMode } from "@/api/backend";
import type { MoveWindow } from "../board/moveMarks";
import type { SuggestionStyle } from "../board/suggestionText";
import type { Arrangement } from "../frame/arrange";
import type { RegionName } from "../frame/MainPanel";

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
  show_list_pane: true,
  winrate_graph_mode: "Both",
  fill_winrate_graph: true,
  show_blunder_bar: false,
  score_lead_includes_komi: true,
  confirm_new_board: true,
  confirm_overwrite: true,
  show_names_below_board: true,
  estimate_location: "Off",
  estimate_style: "TransparentSmall",
  pure_estimate_style: "BigSquare",
  remember_estimate: false,
  moves_ownership: true,
  estimate_shortcut: false,
  sub_board_heat: "None",
  sub_board_raw: false,
  top_toolbar_layout: "Separate",
  top_toolbar_wrap: false,
  top_toolbar_hidden: [],
  bottom_toolbar_hidden: [],
  autoplay_interval_ms: 2000,
  painting_color: "Red",
  control_panel_bounds: null,
  layout_mode: "Normal",
  large_sub_board: false,
  large_winrate_graph: false,
  board_position: 4,
  floating_boards: { main: false, keep_main_board: false, sub: false, main_on_top: false, sub_on_top: false, main_locked: false, sub_locked: false },
  custom_layouts: [null, null],
  main_window: null,
  floating_main: null,
  floating_sub: null,
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

/** Java's `toggleShowMoveNumber` (M, and the 手数 button): none → all → the last move → none. */
export function nextMoveNumbers(display: Display): Partial<Display> {
  if (display.move_numbers === "None") return { move_numbers: "All" };
  if (display.move_numbers === "All") return { move_numbers: "LastOne" };
  return { move_numbers: "None" };
}

/**
 * Java's `toggleShowMoveRankMark` (Alt+M): all → the last move → the last N → none → all.
 * Marks for the latest moves hide the move numbers, which would cover them.
 */
export function nextRankMarks(display: Display): Partial<Display> {
  const order: Record<MoveRankMarkMode, MoveRankMarkMode> = {
    All: "LastOne",
    LastOne: display.last_move_rank_marks > 1 ? "LastN" : "None",
    LastN: "None",
    None: "All",
  };
  const next = order[display.move_rank_marks];
  return next === "LastOne" || next === "LastN" ? { move_rank_marks: next, move_numbers: "None" } : { move_rank_marks: next };
}

/** The panels 显示 → 面板 has switched off. */
export function hiddenRegions(display: Display): RegionName[] {
  // 精简模式 shows the board alone; 思考模式 and 四方图 draw no separate sub-board.
  if (display.layout_mode === "Min") return ["captured", "statistics", "graph", "comment", "tree", "subBoard", "list", "status", "extra"];
  const hidden: RegionName[] = display.layout_mode === "Normal" ? [] : ["subBoard"];
  if (!display.show_information_panel) hidden.push("captured", "statistics");
  if (!display.show_winrate_graph) hidden.push("graph");
  if (!display.show_comment) hidden.push("comment");
  if (!display.show_variation_graph) hidden.push("tree");
  if (!display.show_sub_board && !hidden.includes("subBoard")) hidden.push("subBoard");
  if (!display.show_list_pane) hidden.push("list");
  if (!display.show_status_bar) hidden.push("status");
  return hidden;
}

/** The seven panels of 显示 → 面板 that 精简模式 hides and 可视化面板设置 switches. */
export const PANEL_FIELDS = ["show_sub_board", "show_winrate_graph", "show_comment", "show_variation_graph", "show_list_pane", "show_information_panel", "show_status_bar"] as const;

/**
 * A change with Java's rules applied: the two enlargements exclude each other and bring their panel
 * along, 放大胜率图 puts the candidate list away (and the list it), and switching a panel on in
 * 精简模式 goes back to the default layout.
 */
export function withLayoutRules(display: Display, patch: Partial<Display>): Display {
  const next = { ...display, ...patch };
  if (patch.large_sub_board === true) Object.assign(next, { large_winrate_graph: false, show_sub_board: true });
  if (patch.large_winrate_graph === true) Object.assign(next, { large_sub_board: false, show_winrate_graph: true, show_list_pane: false });
  if (patch.show_list_pane === true && patch.large_winrate_graph === undefined) next.large_winrate_graph = false;
  const panelOn = PANEL_FIELDS.some((field) => patch[field] === true && !display[field]);
  if (display.layout_mode === "Min" && patch.layout_mode === undefined && panelOn) next.layout_mode = "Normal";
  return next;
}

/** What decides the main panel's layout besides the panels themselves. */
export function arrangementOf(display: Display): Arrangement {
  return { mode: display.layout_mode, largeSubBoard: display.large_sub_board, largeGraph: display.large_winrate_graph, position: display.board_position };
}

/** What the candidates on the board say, and how. */
export function suggestionStyle(display: Display, komi: number): SuggestionStyle {
  return {
    winrate: display.show_winrate_in_suggestion,
    visits: display.show_visits_in_suggestion,
    score: display.show_score_in_suggestion,
    scoreAsDiff: display.show_score_as_diff,
    order: display.show_suggestion_order,
    blackWinrate: display.winrate_always_black,
    scoreWithKomi: display.score_lead_includes_komi,
    komi,
  };
}

/**
 * Black's lead as 目数显示 asks for it: KataGo's lead, which counts komi, or the board score
 * without it, which is Black's lead plus komi.
 */
export function shiftScores(points: readonly GraphPointView[], offset: number): readonly GraphPointView[] {
  if (offset === 0) return points;
  return points.map((point) => (point.summary ? { ...point, summary: { ...point.summary, score_lead: point.summary.score_lead + offset } } : point));
}
