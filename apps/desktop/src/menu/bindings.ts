import type { BoardSyncPhase, VariationTreeView } from "@/api/backend";
import { JUMP } from "../frame/BottomToolbar";
import { branchTarget, mainTrunkTarget } from "./branch";

/** What a menu entry or shortcut does now. Entries without a binding are shown disabled. */
export interface MenuBinding {
  run?(): void;
  /** Defaults to true when `run` is set. */
  enabled?: boolean;
  checked?: boolean;
}

export type MenuBindings = Readonly<Record<string, MenuBinding>>;

export function isEnabled(binding: MenuBinding | undefined): boolean {
  return binding?.run !== undefined && binding.enabled !== false;
}

export interface MenuState {
  hasDocument: boolean;
  hasBoard: boolean;
  ownership: boolean;
  /** Whole-game analysis is running or paused. */
  wholeGameActive: boolean;
  /** The engine's first choice for the displayed position, if it has one. */
  hasBestMove: boolean;
  /** An engine profile is saved, so KataGo can be tuned. */
  engineConfigured: boolean;
  /** Java offers readboard, the board synchronization tool, on Windows only. */
  windows: boolean;
  syncPhase: BoardSyncPhase;
  syncOptions: { always_sync: boolean; always_goto_last: boolean };
  tree: VariationTreeView | null;
  current: number | null;
  canUndo: boolean;
  canRedo: boolean;
  /** Recently opened games, newest first: entries `recent0`… of 最近打开. */
  recent: readonly string[];
  /** The setup tool in hand, or null outside 起始局面设置模式. */
  setupTool: "Black" | "White" | "Erase" | null;
  /** The game has no move yet, so its setup stones and first player can be edited. */
  setupEditable: boolean;
  /** 试下 is on: entries that replace the game wait until it ends. */
  trial: boolean;
  /** 添加黑子 / 添加白子 in force, or null for alternate play. */
  playColor: "Black" | "White" | null;
  /** 插入黑子 / 插入白子 as a mode, or null. */
  insertColor: "Black" | "White" | null;
}

export interface MenuActions {
  open(): void;
  openRecent(path: string): void;
  clearRecent(): void;
  newBoard(): void;
  setBoardSize(): void;
  editGameInfo(): void;
  save(): void;
  saveAs(): void;
  deleteMove(): void;
  deleteBranch(): void;
  setAsMain(): void;
  undo(): void;
  redo(): void;
  toggleSetup(): void;
  setupTool(tool: "Black" | "White" | "Erase"): void;
  clearSetup(): void;
  setFirstPlayer(color: "Black" | "White"): void;
  convertToStart(): void;
  colorMode(color: "Black" | "White"): void;
  alternate(): void;
  insertMode(color: "Black" | "White"): void;
  quit(): void;
  setOwnership(visible: boolean): void;
  togglePondering(): void;
  openAutoAnalysis(): void;
  /** 整盘速览（闪电）: one visit per move over the displayed line. */
  flashAnalyze(): void;
  cancelAutoAnalysis(): void;
  openStrength(): void;
  playBestMove(): void;
  pass(): void;
  step(steps: number): void;
  goTo(node: number): void;
  openFox(): void;
  startBoardSync(): void;
  toggleSyncOption(key: "always_sync" | "always_goto_last"): void;
  openTsumego(): void;
  openBenchmark(): void;
  openSettings(): void;
  openAbout(): void;
}

function when(enabled: boolean, run: () => void): MenuBinding {
  return { run, enabled };
}

/**
 * The entries the new app can already do, keyed by the Java ids in `spec.ts`, plus the
 * navigation keys Java handles without a menu entry. Shown-but-fixed panels are checked.
 */
export function menuBindings(state: MenuState, actions: MenuActions): MenuBindings {
  const { tree, current } = state;
  const navigable = tree !== null && current !== null;
  const left = navigable ? branchTarget(tree, current, -1) : null;
  const right = navigable ? branchTarget(tree, current, 1) : null;
  const trunk = navigable ? mainTrunkTarget(tree, current) : null;
  const goTo = (node: number | null) => () => {
    if (node !== null) actions.goTo(node);
  };
  const step = (steps: number) => when(state.hasDocument, () => actions.step(steps));
  const always: MenuBinding = { checked: true };
  // The root is the game itself; it is cleared, not deleted.
  const deletable = navigable && current !== tree.root;
  // A trial is a copy of the game: what would replace the game waits until it ends.
  const free = !state.trial;
  const recent: Record<string, MenuBinding> = {};
  state.recent.forEach((path, index) => {
    recent[`recent${index}`] = when(free, () => actions.openRecent(path));
  });
  if (state.recent.length > 0) recent.clearRecent = when(true, actions.clearRecent);

  return {
    newBoard: when(free, actions.newBoard),
    open: when(free, actions.open),
    ...recent,
    save: when(state.hasDocument, actions.save),
    saveAs: when(state.hasDocument, actions.saveAs),
    exit: when(true, actions.quit),

    // The 显示 settings are bound by `displayBindings`.
    kataEstimateClose: { run: () => actions.setOwnership(false), checked: !state.ownership },
    kataEstimateOnMainBoard: { run: () => actions.setOwnership(true), checked: state.ownership },

    playBestMove: when(state.hasBestMove, actions.playBestMove),
    setupModeToggle: { run: actions.toggleSetup, enabled: state.hasBoard, checked: state.setupTool !== null },
    setupToolBlack: { run: () => actions.setupTool("Black"), enabled: state.hasBoard, checked: state.setupTool === "Black" },
    setupToolWhite: { run: () => actions.setupTool("White"), enabled: state.hasBoard, checked: state.setupTool === "White" },
    setupToolErase: { run: () => actions.setupTool("Erase"), enabled: state.hasBoard, checked: state.setupTool === "Erase" },
    setupClearAll: when(state.setupEditable, actions.clearSetup),
    setupBlackToPlay: when(state.setupEditable, () => actions.setFirstPlayer("Black")),
    setupWhiteToPlay: when(state.setupEditable, () => actions.setFirstPlayer("White")),
    convertCurrentPosition: when(state.hasBoard, actions.convertToStart),
    playPassMove: when(state.hasBoard, actions.pass),

    togglePonder: when(true, actions.togglePondering),
    autoAnalyze: when(state.hasDocument, actions.openAutoAnalysis),
    wholeGameDeepAnalysis: when(state.hasDocument, actions.openAutoAnalysis),
    flashAnalyzeAllGame: when(state.hasDocument, actions.flashAnalyze),
    stopAutoAnalyze: when(state.wholeGameActive, actions.cancelAutoAnalysis),
    playerStrengthEstimate: when(state.hasDocument, actions.openStrength),
    tsumeGoMenu: when(state.hasBoard && free, actions.openTsumego),
    // 引擎一键设置 opens Java's 性能优化 page: KataGo's own benchmark, started by the user.
    autoSetup: when(state.engineConfigured, actions.openBenchmark),
    comprehensiveSettings: when(true, actions.openSettings),

    clearBoard: when(free, actions.newBoard),
    addBlack: when(state.hasBoard, () => actions.colorMode("Black")),
    addWhite: when(state.hasBoard, () => actions.colorMode("White")),
    alternatelyMoves: when(state.hasBoard, actions.alternate),
    insertBlack: { run: () => actions.insertMode("Black"), enabled: state.hasBoard, checked: state.insertColor === "Black" },
    insertWhite: { run: () => actions.insertMode("White"), enabled: state.hasBoard, checked: state.insertColor === "White" },
    backToMainBranch: when(trunk !== null, goTo(trunk)),
    // Off the main line there is something to promote.
    setAsMain: when(trunk !== null, actions.setAsMain),
    delete: when(deletable, actions.deleteMove),
    deleteBranch: when(deletable, actions.deleteBranch),
    undoDelete: when(state.canUndo, actions.undo),
    redoDelete: when(state.canRedo, actions.redo),
    setInfo: when(state.hasDocument, actions.editGameInfo),
    setBoard: when(free, actions.setBoardSize),
    jumpToFirst: step(-JUMP),
    jumpToLast: step(JUMP),
    jumpToLeft: when(left !== null, goTo(left)),
    jumpToRight: when(right !== null, goTo(right)),
    previousMove: step(-1),
    nextMove: step(1),
    back10: step(-10),
    forward10: step(10),

    foxKifu: when(free, actions.openFox),
    // Started once; readboard's own window stops it, as does the toolbar chip.
    readBoard: when(free && state.windows && (state.syncPhase === "Stopped" || state.syncPhase === "Failed"), actions.startBoardSync),
    alwaysKeepBoardStatSync: { run: () => actions.toggleSyncOption("always_sync"), checked: state.syncOptions.always_sync },
    alwaysGoToLastMove: { run: () => actions.toggleSyncOption("always_goto_last"), checked: state.syncOptions.always_goto_last },
    about: when(true, actions.openAbout),
    languageDefault: always,
    // The engine entries (the list, 设置 → 引擎, restart, shut down) are bound by `useEngines`.
  };
}
