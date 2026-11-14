/**
 * 显示 → 工具栏 → 自定义内容. Each toolbar item is named by its Java menu id. The saved lists
 * (`top_toolbar_hidden`, `bottom_toolbar_hidden`) hold the items switched away from Java's
 * defaults, so an item Java hides at first is listed once it is shown.
 */

/** The top toolbar's groups Java shows at first (`Config`: show-basic-btn, show-rule-menu, …). */
export const TOP_ITEMS: Readonly<Record<string, boolean>> = {
  showDoubleMenuBtn: false,
  showBasicBtn: true,
  playMoveTools: true,
  allowAvoidTools: true,
  showGameController: true,
  showTimeLimitController: false,
  showPlayoutLimitController: false,
  showCandidateController: true,
  showRuleMenu: true,
  showTsumeGoMenu: true,
  showParamMenu: true,
  showGobanMenu: true,
  showSaveLoadMenu: true,
  // Java #709: PDA and 广度 (WRN) can be hidden on their own.
  showPdaInToolbar: true,
  showWrnInToolbar: true,
};

/** The bottom toolbar's buttons Java shows at first (`Config`: clearButton, kataEstimate, …). */
export const BOTTOM_ITEMS: Readonly<Record<string, boolean>> = {
  kataEstimateButton: true,
  batchOpen: true,
  openfile: false,
  savefile: false,
  finalScore: true,
  heatMap: true,
  badMoves: false,
  analyzeList: false,
  refresh: false,
  analyse: true,
  tryPlay: true,
  setMainButton: true,
  backMain: false,
  clearButton: true,
  deleteMove: true,
  moveRank: false,
  move: true,
  coords: true,
  autoPlay: true,
};

export function shown(items: Readonly<Record<string, boolean>>, switched: readonly string[], id: string): boolean {
  return (items[id] ?? false) !== switched.includes(id);
}

/** The saved list with one item switched the other way. */
export function switchItem(switched: readonly string[], id: string): string[] {
  return switched.includes(id) ? switched.filter((item) => item !== id) : [...switched, id];
}

/** Which items a toolbar shows, from its saved list. */
export type ItemFilter = (id: string) => boolean;

export const itemFilter =
  (items: Readonly<Record<string, boolean>>, switched: readonly string[]): ItemFilter =>
  (id) =>
    shown(items, switched, id);
