import { useCallback, useEffect, useState } from "react";
import type { Backend, FloatingBoardsView, Settings } from "@/api/backend";
import { filled } from "../display/settings";
import type { RegionName } from "../frame/MainPanel";
import type { MenuBindings } from "../menu/bindings";

export type FloatingBackend = Pick<Backend, "setFloatingLayout" | "fitMainWindow" | "subscribeFloatingBoardsChanged" | "unsubscribe">;

/** The three states every floating entry is a preset of. */
export interface FloatingLayout {
  main: boolean;
  keep: boolean;
  sub: boolean;
}

type Floating = Required<FloatingBoardsView>;
const NONE: Floating = { main: false, keep_main_board: false, sub: false, main_on_top: false, sub_on_top: false, main_locked: false, sub_locked: false };
const full = (view: FloatingBoardsView | null | undefined): Floating => ({ ...NONE, ...view });

/** Java's 浮动双棋盘 leaves the main window this share of its height wide. */
const BOTH_BOARDS_RATIO = 0.65;

function report(error: unknown) {
  console.error("Floating board command failed", error);
}

/**
 * The main window's side of 浮动主棋盘 / 浮动小棋盘: the entries of 显示 → 面板 and 显示 → 浮动棋盘模式,
 * kept to what Rust says is open, and what the main panel then leaves out.
 */
export function useFloating(backend: FloatingBackend) {
  const [view, setView] = useState<Floating>(NONE);
  const load = useCallback((settings: Pick<Settings, "display">) => setView(full(filled(settings.display).floating_boards)), []);

  // A board closed from its own window changes this too.
  useEffect(() => {
    let active = true;
    let id: number | undefined;
    void backend.subscribeFloatingBoardsChanged?.((next) => active && setView(full(next))).then((subscription) => {
      id = subscription;
      if (!active) void backend.unsubscribe(subscription);
    });
    return () => {
      active = false;
      if (id !== undefined) void backend.unsubscribe(id);
    };
  }, [backend]);

  const set = ({ main, keep, sub }: FloatingLayout) => void backend.setFloatingLayout(main, keep, sub).then((next) => setView(full(next)), report);
  const now: FloatingLayout = { main: view.main, keep: view.keep_main_board, sub: view.sub };
  const floated = now.main && !now.keep;
  const bindings: MenuBindings = {
    // 显示 → 面板: each shows what is open; choosing an open one puts that board back.
    independentMainBoard: { run: () => set({ ...now, main: !floated, keep: false }), checked: floated },
    independentMainBoard2: { run: () => set({ ...now, main: !(now.main && now.keep), keep: !(now.main && now.keep) }), checked: now.main && now.keep },
    independentSubBoard: { run: () => set({ ...now, sub: !now.sub }), checked: now.sub },
    // 显示 → 浮动棋盘模式: presets.
    indepBothBoard: {
      run: () => {
        const bottom = document.querySelector<HTMLElement>('[data-testid="bottom-toolbar"]')?.offsetHeight ?? 0;
        set({ main: true, keep: false, sub: true });
        void backend.fitMainWindow(Math.round((window.innerHeight - bottom) * BOTH_BOARDS_RATIO), window.innerHeight).catch(report);
      },
    },
    indepMainBoard: { run: () => set({ main: true, keep: false, sub: false }) },
    indepSubBoard: { run: () => set({ main: false, keep: false, sub: true }) },
    indepExtraBoard: { run: () => set({ main: true, keep: true, sub: true }) },
  };

  return {
    load,
    layout: now,
    set,
    bindings,
    arrangement: { boardFloated: floated },
    /** The main panel leaves out a board that floats. */
    hide: (hidden: readonly RegionName[]): RegionName[] => [...hidden, ...(floated ? (["board"] as const) : []), ...(now.sub ? (["subBoard"] as const) : [])],
  };
}
