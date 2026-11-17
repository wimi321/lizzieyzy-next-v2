import { useEffect, useRef } from "react";
import { isEnabled, type MenuBindings } from "./bindings";

interface Shortcut {
  id: string;
  /** `KeyboardEvent.code`, so letters work whatever the keyboard layout or input method. */
  code: string;
  /** Ctrl, or Cmd on macOS, as Java's `controlIsPressed` reads it. */
  control?: boolean;
  /** The Control key itself, never Cmd: ⌘W closes windows on macOS. */
  ctrlOnly?: boolean;
  alt?: boolean;
  shift?: boolean;
  /** Navigation repeats while the key is held; commands run once per press. */
  repeat?: boolean;
}

/** Java's keys for the entries the new app can do (`Input.java`, and the menu labels). */
const SHORTCUTS: readonly Shortcut[] = [
  { id: "open", code: "KeyO" },
  { id: "openUrl", code: "KeyQ" },
  { id: "saveAs", code: "KeyS" },
  { id: "save", code: "KeyS", control: true },
  { id: "togglePonder", code: "Space" },
  { id: "autoAnalyze", code: "KeyA" },
  { id: "clearAllLizzieCache", code: "KeyA", shift: true },
  { id: "makeAutoPlay", code: "KeyA", control: true },
  // 搜索功能 (`FunctionSearchController`): Ctrl+K, or ⌘K on macOS.
  { id: "functionSearch", code: "KeyK", control: true },
  { id: "playPassMove", code: "KeyP" },
  { id: "playBestMove", code: "Comma" },
  // Java's game keys: N 人机对局, Alt+N its analysis-mode twin, Alt+E 引擎对局, Enter 人机续弈 for the side to move.
  { id: "newGenmoveGame", code: "KeyN" },
  { id: "newAnalyzeModeGame", code: "KeyN", alt: true },
  { id: "newEngineGame", code: "KeyE", alt: true },
  { id: "continueGameHere", code: "Enter" },
  { id: "coordsMenu", code: "KeyC" },
  { id: "hawkEye2", code: "KeyY" },
  { id: "toggleMoveNumbers", code: "KeyM" },
  { id: "showAllMoveNumberInBranch", code: "KeyM", control: true },
  { id: "toggleMoveRankMarks", code: "KeyM", alt: true },
  { id: "backToMainBranch", code: "KeyB" },
  { id: "setAsMain", code: "KeyL" },
  { id: "delete", code: "Delete" },
  { id: "delete", code: "Backspace" },
  { id: "deleteBranch", code: "Delete", shift: true },
  { id: "deleteBranch", code: "Backspace", shift: true },
  { id: "setInfo", code: "KeyI" },
  { id: "setBoard", code: "KeyI", control: true },
  { id: "clearBoard", code: "Home", control: true },
  { id: "engineConfig", code: "KeyX", alt: true },
  // 引擎参数 (Alt+D) and 终局点目 (Ctrl+Q), as Java's menu labels say.
  { id: "engineParameters", code: "KeyD", alt: true },
  { id: "scoreGame", code: "KeyQ", control: true },
  { id: "readBoard", code: "KeyO", alt: true },
  { id: "tsumeGoMenu", code: "KeyE", shift: true },
  { id: "jumpToFirst", code: "Home" },
  { id: "jumpToLast", code: "End" },
  { id: "jumpToLeft", code: "ArrowLeft", repeat: true },
  { id: "jumpToRight", code: "ArrowRight", repeat: true },
  { id: "previousMove", code: "ArrowUp", repeat: true },
  { id: "nextMove", code: "ArrowDown", repeat: true },
  { id: "back10", code: "ArrowUp", control: true, repeat: true },
  { id: "forward10", code: "ArrowDown", control: true, repeat: true },
  // 纯网络(H); "." toggles Kata形势 once 使用快捷键(大键盘点) is on.
  { id: "showHeatmap", code: "KeyH" },
  { id: "toggleKataEstimate", code: "Period" },
  { id: "copySgf", code: "KeyC", control: true },
  { id: "pasteSgf", code: "KeyV", control: true },
  { id: "copyBoardScreen", code: "KeyC", shift: true },
  { id: "copySubBoardScreen", code: "KeyC", alt: true },
  { id: "saveRaw", code: "KeyS", control: true, shift: true },
  { id: "saveBranchRaw", code: "KeyS", control: true, alt: true },
  { id: "saveMainBoardScreen", code: "KeyS", alt: true },
  { id: "saveSubBoardScreen", code: "KeyS", shift: true },
  { id: "saveWinrate", code: "KeyS", shift: true, alt: true },
  { id: "alwaysOnTop", code: "KeyZ", control: true },
  { id: "spinRight", code: "ArrowRight", control: true, alt: true },
  { id: "spinLeft", code: "ArrowLeft", control: true, alt: true },
  { id: "mirrorVertical", code: "ArrowUp", control: true, alt: true },
  { id: "mirrorHorizon", code: "ArrowDown", control: true, alt: true },
  // 布局模式 Alt+1…5, 自定义布局 Alt+8 / 9, 放大小棋盘 Ctrl+F, 放大胜率图 Ctrl+W, 主棋盘位置 [ ].
  { id: "defaultView", code: "Digit1", alt: true },
  { id: "classicView", code: "Digit2", alt: true },
  { id: "minView", code: "Digit3", alt: true },
  { id: "extraMode3", code: "Digit4", alt: true },
  { id: "extraMode1", code: "Digit5", alt: true },
  { id: "extraMode2", code: "Digit6", alt: true },
  { id: "customeMode1", code: "Digit8", alt: true },
  { id: "customeMode2", code: "Digit9", alt: true },
  { id: "largeSubBoard", code: "KeyF", control: true },
  { id: "largeWinrateGraph", code: "KeyW", ctrlOnly: true },
  // 浮动主棋盘 Alt+Q, 浮动小棋盘 Alt+A (Java's code; its Chinese label says Alt+W), 浮动双棋盘 Alt+7.
  { id: "independentMainBoard", code: "KeyQ", alt: true },
  { id: "independentSubBoard", code: "KeyA", alt: true },
  { id: "indepBothBoard", code: "Digit7", alt: true },
  { id: "leftMove", code: "BracketLeft", repeat: true },
  { id: "rightMove", code: "BracketRight", repeat: true },
];

type KeyPress = Pick<KeyboardEvent, "key" | "code" | "ctrlKey" | "metaKey" | "altKey" | "shiftKey" | "repeat">;

/** Synthetic events may carry only `key`; read it as the code it would have. */
function codeOf(event: KeyPress): string {
  if (event.code) return event.code;
  if (/^[a-z]$/i.test(event.key)) return `Key${event.key.toUpperCase()}`;
  if (event.key === "[") return "BracketLeft";
  if (event.key === "]") return "BracketRight";
  return event.key === " " ? "Space" : event.key === "," ? "Comma" : event.key === "." ? "Period" : event.key;
}

export function matchShortcut(event: KeyPress): string | null {
  const code = codeOf(event);
  const control = event.ctrlKey || event.metaKey;
  const shortcut = SHORTCUTS.find(
    (candidate) =>
      candidate.code === code &&
      (candidate.ctrlOnly ? event.ctrlKey && !event.metaKey : Boolean(candidate.control) === control) &&
      Boolean(candidate.alt) === event.altKey &&
      Boolean(candidate.shift) === event.shiftKey &&
      (!event.repeat || candidate.repeat),
  );
  return shortcut?.id ?? null;
}

/** Typing, menus and dialogs keep their own keys. */
function ownsKeys(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  if (target instanceof HTMLInputElement || target instanceof HTMLSelectElement || target instanceof HTMLTextAreaElement) {
    return true;
  }
  return target.closest('[contenteditable="true"], [role="dialog"], [role="menu"], [role="menubar"]') !== null;
}

/** Runs the bound entry for a pressed shortcut while `active` (no dialog is open). */
export function useShortcuts(bindings: MenuBindings, active: boolean) {
  const latest = useRef({ bindings, active });
  useEffect(() => {
    latest.current = { bindings, active };
  });

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!latest.current.active || event.defaultPrevented || event.isComposing || ownsKeys(event.target)) return;
      const id = matchShortcut(event);
      // Ctrl+C on selected text (a comment, a dialog's message) copies the text, not the game.
      if (id === "copySgf" && window.getSelection()?.isCollapsed === false) return;
      const binding = id === null ? undefined : latest.current.bindings[id];
      if (!isEnabled(binding)) return;
      event.preventDefault();
      binding?.run?.();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}
