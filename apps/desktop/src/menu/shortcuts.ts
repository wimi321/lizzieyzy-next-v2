import { useEffect, useRef } from "react";
import { isEnabled, type MenuBindings } from "./bindings";

interface Shortcut {
  id: string;
  /** `KeyboardEvent.code`, so letters work whatever the keyboard layout or input method. */
  code: string;
  /** Ctrl, or Cmd on macOS, as Java's `controlIsPressed` reads it. */
  control?: boolean;
  alt?: boolean;
  shift?: boolean;
  /** Navigation repeats while the key is held; commands run once per press. */
  repeat?: boolean;
}

/** Java's keys for the entries the new app can do (`Input.java`, and the menu labels). */
const SHORTCUTS: readonly Shortcut[] = [
  { id: "open", code: "KeyO" },
  { id: "saveAs", code: "KeyS" },
  { id: "save", code: "KeyS", control: true },
  { id: "togglePonder", code: "Space" },
  { id: "autoAnalyze", code: "KeyA" },
  { id: "playPassMove", code: "KeyP" },
  { id: "playBestMove", code: "Comma" },
  { id: "coordsMenu", code: "KeyC" },
  { id: "backToMainBranch", code: "KeyB" },
  { id: "engineConfig", code: "KeyX", alt: true },
  { id: "tsumeGoMenu", code: "KeyE", shift: true },
  { id: "jumpToFirst", code: "Home" },
  { id: "jumpToLast", code: "End" },
  { id: "jumpToLeft", code: "ArrowLeft", repeat: true },
  { id: "jumpToRight", code: "ArrowRight", repeat: true },
  { id: "previousMove", code: "ArrowUp", repeat: true },
  { id: "nextMove", code: "ArrowDown", repeat: true },
  { id: "back10", code: "ArrowUp", control: true, repeat: true },
  { id: "forward10", code: "ArrowDown", control: true, repeat: true },
];

type KeyPress = Pick<KeyboardEvent, "key" | "code" | "ctrlKey" | "metaKey" | "altKey" | "shiftKey" | "repeat">;

/** Synthetic events may carry only `key`; read it as the code it would have. */
function codeOf(event: KeyPress): string {
  if (event.code) return event.code;
  if (/^[a-z]$/i.test(event.key)) return `Key${event.key.toUpperCase()}`;
  return event.key === " " ? "Space" : event.key === "," ? "Comma" : event.key;
}

export function matchShortcut(event: KeyPress): string | null {
  const code = codeOf(event);
  const control = event.ctrlKey || event.metaKey;
  const shortcut = SHORTCUTS.find(
    (candidate) =>
      candidate.code === code &&
      Boolean(candidate.control) === control &&
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
      const binding = id === null ? undefined : latest.current.bindings[id];
      if (!isEnabled(binding)) return;
      event.preventDefault();
      binding?.run?.();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}
