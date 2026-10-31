import type { BoardSyncPhase, VariationTreeView } from "@/api/backend";
import { JUMP } from "../toolbar/Toolbar";
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
  coordinates: boolean;
  ownership: boolean;
  /** Whole-game analysis is running or paused. */
  wholeGameActive: boolean;
  /** The engine's first choice for the displayed position, if it has one. */
  hasBestMove: boolean;
  engineFailed: boolean;
  /** Java offers readboard, the board synchronization tool, on Windows only. */
  windows: boolean;
  syncPhase: BoardSyncPhase;
  syncOptions: { always_sync: boolean; always_goto_last: boolean };
  tree: VariationTreeView | null;
  current: number | null;
}

export interface MenuActions {
  open(): void;
  saveAs(): void;
  quit(): void;
  toggleCoordinates(): void;
  setOwnership(visible: boolean): void;
  togglePondering(): void;
  openAutoAnalysis(): void;
  cancelAutoAnalysis(): void;
  openStrength(): void;
  playBestMove(): void;
  pass(): void;
  step(steps: number): void;
  goTo(node: number): void;
  openFox(): void;
  startBoardSync(): void;
  toggleSyncOption(key: "always_sync" | "always_goto_last"): void;
  openEngineSetup(): void;
  restartEngine(): void;
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

  return {
    open: when(true, actions.open),
    // There is no remembered file to write back to yet, so saving always asks where.
    save: when(state.hasDocument, actions.saveAs),
    saveAs: when(state.hasDocument, actions.saveAs),
    exit: when(true, actions.quit),

    winrateGraph: always,
    commitPane: always,
    variationPane: always,
    listPane: always,
    informationPane: always,
    coordsMenu: { run: actions.toggleCoordinates, checked: state.coordinates },
    showVariationOnMouse: always,
    kataEstimateClose: { run: () => actions.setOwnership(false), checked: !state.ownership },
    kataEstimateOnMainBoard: { run: () => actions.setOwnership(true), checked: state.ownership },

    playBestMove: when(state.hasBestMove, actions.playBestMove),
    playPassMove: when(state.hasBoard, actions.pass),

    togglePonder: when(true, actions.togglePondering),
    autoAnalyze: when(state.hasDocument, actions.openAutoAnalysis),
    wholeGameDeepAnalysis: when(state.hasDocument, actions.openAutoAnalysis),
    stopAutoAnalyze: when(state.wholeGameActive, actions.cancelAutoAnalysis),
    playerStrengthEstimate: when(state.hasDocument, actions.openStrength),

    backToMainBranch: when(trunk !== null, goTo(trunk)),
    jumpToFirst: step(-JUMP),
    jumpToLast: step(JUMP),
    jumpToLeft: when(left !== null, goTo(left)),
    jumpToRight: when(right !== null, goTo(right)),
    previousMove: step(-1),
    nextMove: step(1),
    back10: step(-10),
    forward10: step(10),

    foxKifu: when(true, actions.openFox),
    // Started once; readboard's own window stops it, as does the toolbar chip.
    readBoard: when(state.windows && (state.syncPhase === "Stopped" || state.syncPhase === "Failed"), actions.startBoardSync),
    alwaysKeepBoardStatSync: { run: () => actions.toggleSyncOption("always_sync"), checked: state.syncOptions.always_sync },
    alwaysGoToLastMove: { run: () => actions.toggleSyncOption("always_goto_last"), checked: state.syncOptions.always_goto_last },
    about: when(true, actions.openAbout),
    engineConfig: when(true, actions.openEngineSetup),
    languageDefault: always,
    engine0: always,
    // Configuring the same engine again restarts it only once it has failed.
    restartCurrentEngine: when(state.engineFailed, actions.restartEngine),
  };
}
