import type { MenuBinding } from "../menu/bindings";
import type { EstimateStyle } from "@/api/backend";
import { BOTTOM_ITEMS, shown, switchItem, TOP_ITEMS } from "../frame/toolbarItems";
import { type Display, nextMoveNumbers, nextRankMarks } from "./settings";

/** The Kata形势显示 entries' suffixes, after kataEstimate / pureEstimate. */
const STYLE_IDS: Readonly<Record<string, EstimateStyle>> = {
  ByTransparentSmall: "TransparentSmall",
  ByTransparent: "Transparent",
  ByTransparentNotOnLive: "TransparentOffStones",
  ByBigSquare: "BigSquare",
  BySize: "BySize",
};

/** The 显示 entries that ask for a number. */
export type DisplayPrompt = "numbers" | "ranks" | "minVisits";

type Switch = { [K in keyof Display]: Display[K] extends boolean ? K : never }[keyof Display];

/**
 * The 显示 menu, keyed by the Java ids in `spec.ts`: each entry shows the saved setting and
 * changes it. Entries that ask for a number open their prompt instead.
 */
export function displayBindings(display: Display, set: (patch: Partial<Display>) => void, ask: (prompt: DisplayPrompt) => void): Record<string, MenuBinding> {
  const toggle = (key: Switch): MenuBinding => ({ run: () => set({ [key]: !display[key] }), checked: display[key] });
  const choose = <K extends keyof Display>(key: K, value: Display[K]): MenuBinding => ({
    run: () => set({ [key]: value } as Partial<Display>),
    checked: display[key] === value,
  });
  const items = (key: "top_toolbar_hidden" | "bottom_toolbar_hidden", defaults: Readonly<Record<string, boolean>>) =>
    Object.fromEntries(
      Object.keys(defaults).map((id) => [id, { run: () => set({ [key]: switchItem(display[key], id) }), checked: shown(defaults, display[key], id) }]),
    );
  const styles = (prefix: "kataEstimate" | "pureEstimate", key: "estimate_style" | "pure_estimate_style") =>
    Object.fromEntries(Object.entries(STYLE_IDS).map(([suffix, style]) => [`${prefix}${suffix}`, choose(key, style)]));
  return {
    subboard: toggle("show_sub_board"),
    winrateGraph: toggle("show_winrate_graph"),
    commitPane: toggle("show_comment"),
    variationPane: toggle("show_variation_graph"),
    listPane: toggle("show_list_pane"),
    informationPane: toggle("show_information_panel"),
    statusPanel: toggle("show_status_bar"),
    topToolBarSeparated: {
      run: () => set({ show_top_toolbar: true, top_toolbar_layout: "Separate" }),
      checked: display.show_top_toolbar && display.top_toolbar_layout === "Separate",
    },
    topToolBarCombined: {
      run: () => set({ show_top_toolbar: true, top_toolbar_layout: "Combined" }),
      checked: display.show_top_toolbar && display.top_toolbar_layout === "Combined",
    },
    topToolBarInVisible: choose("show_top_toolbar", false),
    autoWrap: { ...toggle("top_toolbar_wrap"), enabled: display.show_top_toolbar && display.top_toolbar_layout === "Separate" },
    ...items("top_toolbar_hidden", TOP_ITEMS),
    ...items("bottom_toolbar_hidden", BOTTOM_ITEMS),
    bottomToolbarVisible: choose("show_bottom_toolbar", true),
    bottomToolbarInVisible: choose("show_bottom_toolbar", false),

    coordsMenu: toggle("show_coordinates"),
    noMoveNum: choose("move_numbers", "None"),
    lastOneMoveNum: choose("move_numbers", "LastOne"),
    lastFiveMoveNum: choose("move_numbers", "LastFive"),
    lastTenMoveNum: choose("move_numbers", "LastTen"),
    allMoveNum: choose("move_numbers", "All"),
    anyMoveNum: { run: () => ask("numbers"), checked: display.move_numbers === "LastN" },
    moveNumberAlwaysFromOne: toggle("move_numbers_from_one"),
    moveNumberInBranchFromOne: choose("branch_move_numbers_from_one", true),
    moveNumberInBranchContinue: choose("branch_move_numbers_from_one", false),
    showAllMoveNumberInBranch: toggle("show_all_branch_move_numbers"),

    rankNoneMove: choose("move_rank_marks", "None"),
    rankLastMove: choose("move_rank_marks", "LastOne"),
    setCustomMoves: { run: () => ask("ranks"), checked: display.move_rank_marks === "LastN" },
    rankAllMove: choose("move_rank_marks", "All"),

    suggestion1: toggle("show_winrate_in_suggestion"),
    suggestion2: toggle("show_visits_in_suggestion"),
    suggestion3: toggle("show_score_in_suggestion"),
    showScoreAsDiff: toggle("show_score_as_diff"),
    showSuggestionOrder: toggle("show_suggestion_order"),
    alwaysShowBlackWinrate: toggle("winrate_always_black"),
    showVariationOnMouse: toggle("show_hover_variation"),

    nextMoveHintNone: choose("next_moves", "None"),
    nextMoveHintSimple: choose("next_moves", "Simple"),
    nextMoveHintInformation: choose("next_moves", "Evaluation"),
    minPlayoutsForNextMove: { run: () => ask("minVisits") },

    showNameInBoard: toggle("show_names_below_board"),
    showNewBoardConfirmDialog: toggle("confirm_new_board"),
    showReplaceFileConfirmDialog: toggle("confirm_overwrite"),

    showWinRateLineOnly: choose("winrate_graph_mode", "Winrate"),
    showScoreLeadLineOnly: choose("winrate_graph_mode", "ScoreLead"),
    showBothLine: choose("winrate_graph_mode", "Both"),
    showWinrateGraphFill: toggle("fill_winrate_graph"),
    showBlunderBar: toggle("show_blunder_bar"),

    // KataGo相关设置 → Kata形势显示.
    kataEstimateClose: choose("estimate_location", "Off"),
    kataEstimateCloseView: choose("estimate_location", "Hidden"),
    kataEstimateOnMainBoard: choose("estimate_location", "Main"),
    kataEstimateOnSubBoard: choose("estimate_location", "Sub"),
    kataEstimateOnBothBoard: choose("estimate_location", "Both"),
    kataEstimateSaveState: toggle("remember_estimate"),
    ...styles("kataEstimate", "estimate_style"),
    ...styles("pureEstimate", "pure_estimate_style"),
    useMovesOwnership: toggle("moves_ownership"),
    useKataEstimateShortcut: toggle("estimate_shortcut"),
    // 小棋盘设置: a heatmap, the line, or the position's stones alone.
    showHeat: { run: () => set({ sub_board_heat: "Policy", sub_board_raw: false }), checked: display.sub_board_heat === "Policy" },
    showHeatAfterCalc: { run: () => set({ sub_board_heat: "AfterSearch", sub_board_raw: false }), checked: display.sub_board_heat === "AfterSearch" },
    notShowHeat: choose("sub_board_heat", "None"),
    subBoardShowVar: {
      run: () => set({ sub_board_heat: "None", sub_board_raw: false }),
      checked: display.sub_board_heat === "None" && !display.sub_board_raw,
    },
    subBoardShowRaw: { run: () => set({ sub_board_heat: "None", sub_board_raw: true }), checked: display.sub_board_raw },

    leadWithKomi: choose("score_lead_includes_komi", true),
    leadWithoutKomi: choose("score_lead_includes_komi", false),

    // Java's keys without a menu entry of their own: M, Alt+M.
    toggleMoveNumbers: { run: () => set(nextMoveNumbers(display)) },
    toggleMoveRankMarks: { run: () => set(nextRankMarks(display)) },
  };
}
