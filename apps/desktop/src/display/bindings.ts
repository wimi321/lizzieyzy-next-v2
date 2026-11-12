import type { MenuBinding } from "../menu/bindings";
import type { Display } from "./settings";

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
  return {
    subboard: toggle("show_sub_board"),
    winrateGraph: toggle("show_winrate_graph"),
    commitPane: toggle("show_comment"),
    variationPane: toggle("show_variation_graph"),
    informationPane: toggle("show_information_panel"),
    statusPanel: toggle("show_status_bar"),
    // One top toolbar layout here: Java's separated and combined rows are the same toolbar.
    topToolBarSeparated: choose("show_top_toolbar", true),
    topToolBarInVisible: choose("show_top_toolbar", false),
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

    leadWithKomi: choose("score_lead_includes_komi", true),
    leadWithoutKomi: choose("score_lead_includes_komi", false),
  };
}
