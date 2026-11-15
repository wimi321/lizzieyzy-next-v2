import { act, cleanup, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";
import type { FloatingBoardsView } from "@/api/backend";
import { analysisUpdated, documentChanged, midGame, midGameAnalysis } from "../../e2e/fixtures";
import { DISPLAY_DEFAULTS } from "../display/settings";
import { PanelSettingsDialog } from "../layout/PanelSettingsDialog";
import zh from "../locales/zh-CN.json";
import { matchShortcut } from "../menu/shortcuts";
import { FloatingBoard, type FloatingBoardProps } from "./FloatingBoard";
import { DRAG_THRESHOLD, SQUARE_DELAY } from "./frame";
import { type FloatingBackend, useFloating } from "./useFloating";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

const BOARDS = (change: FloatingBoardsView = {}): FloatingBoardsView => ({ main: false, keep_main_board: false, sub: false, main_on_top: false, sub_on_top: false, main_locked: false, sub_locked: false, ...change });

describe("the main window's floating entries", () => {
  function hook() {
    let changed: ((view: FloatingBoardsView) => void) | null = null;
    const backend: FloatingBackend & Record<"setFloatingLayout" | "fitMainWindow", ReturnType<typeof vi.fn>> = {
      setFloatingLayout: vi.fn((main: boolean, keep: boolean, sub: boolean) => Promise.resolve(BOARDS({ main, keep_main_board: keep, sub }))),
      fitMainWindow: vi.fn().mockResolvedValue(undefined),
      subscribeFloatingBoardsChanged: vi.fn(async (listener) => {
        changed = listener;
        return 7;
      }),
      unsubscribe: vi.fn().mockResolvedValue(undefined),
    };
    const result = renderHook(() => useFloating(backend));
    return { backend, result: result.result, push: (view: FloatingBoardsView) => act(() => changed?.(view)) };
  }

  test("each entry shows what is open; choosing an open one puts the board back", async () => {
    const { backend, result } = hook();
    act(() => result.current.load({ display: { ...DISPLAY_DEFAULTS, floating_boards: BOARDS({ main: true }) } }));
    expect(result.current.bindings.independentMainBoard?.checked).toBe(true);
    expect(result.current.bindings.independentMainBoard2?.checked).toBe(false);
    expect(result.current.arrangement).toEqual({ boardFloated: true });
    expect(result.current.hide(["comment"])).toEqual(["comment", "board"]);
    await act(async () => result.current.bindings.independentMainBoard?.run?.());
    expect(backend.setFloatingLayout).toHaveBeenLastCalledWith(false, false, false);
    expect(result.current.bindings.independentMainBoard?.checked).toBe(false);
    await act(async () => result.current.bindings.independentMainBoard2?.run?.());
    expect(backend.setFloatingLayout).toHaveBeenLastCalledWith(true, true, false);
    // Kept in the main window as well: nothing is left out of it.
    expect(result.current.hide([])).toEqual([]);
    await act(async () => result.current.bindings.independentSubBoard?.run?.());
    expect(backend.setFloatingLayout).toHaveBeenLastCalledWith(true, true, true);
    expect(result.current.hide([])).toEqual(["subBoard"]);
  });

  test("the floating-board modes are presets; 浮动双棋盘 narrows the window as Java does", async () => {
    const { backend, result } = hook();
    for (const [id, args] of [["indepMainBoard", [true, false, false]], ["indepSubBoard", [false, false, true]], ["indepExtraBoard", [true, true, true]], ["indepBothBoard", [true, false, true]]] as const) {
      await act(async () => result.current.bindings[id]?.run?.());
      expect(backend.setFloatingLayout).toHaveBeenLastCalledWith(...args);
    }
    expect(backend.fitMainWindow).toHaveBeenCalledExactlyOnceWith(Math.round(window.innerHeight * 0.65), window.innerHeight);
  });

  test("a board closed from its own window is unchecked here too", async () => {
    const { result, push } = hook();
    await act(async () => undefined);
    push(BOARDS({ sub: true }));
    expect(result.current.bindings.independentSubBoard?.checked).toBe(true);
    push(BOARDS());
    expect(result.current.bindings.independentSubBoard?.checked).toBe(false);
  });

  test("Alt+Q floats the main board, Alt+A the sub-board, Alt+7 both", () => {
    const key = (code: string) => matchShortcut({ key: "", code, ctrlKey: false, metaKey: false, altKey: true, shiftKey: false, repeat: false });
    expect([key("KeyQ"), key("KeyA"), key("Digit7")]).toEqual(["independentMainBoard", "independentSubBoard", "indepBothBoard"]);
  });

  test("the panel settings' floating switches grey what Java greys", () => {
    const onChange = vi.fn();
    const props = { slot: null, display: DISPLAY_DEFAULTS, labels: zh.layout.dialog, onChange: vi.fn(), onApply: vi.fn(), onCancel: vi.fn() };
    const { rerender } = render(<PanelSettingsDialog {...props} floating={{ layout: { main: false, keep: false, sub: true }, onChange }} />);
    expect(screen.getByTestId("panel-show_sub_board")).toHaveProperty("disabled", true);
    expect(screen.getByTestId("panel-large_sub_board")).toHaveProperty("disabled", false);
    fireEvent.click(screen.getByTestId("panel-float-main"));
    expect(onChange).toHaveBeenLastCalledWith({ main: true, keep: false, sub: true });
    rerender(<PanelSettingsDialog {...props} floating={{ layout: { main: true, keep: false, sub: false }, onChange }} />);
    expect(screen.getByTestId("panel-large_sub_board")).toHaveProperty("disabled", true);
    expect(screen.getByTestId("panel-show_sub_board")).toHaveProperty("disabled", false);
  });
});

type Backend = FloatingBoardProps["backend"];

function pageBackend(options: FloatingBoardsView = BOARDS()) {
  const backend = {
    settings: vi.fn().mockResolvedValue({ version: 1, engine: null, pondering: true, display: { ...DISPLAY_DEFAULTS, floating_boards: options } }),
    subscribe: vi.fn(async (listener: (event: never) => void) => {
      queueMicrotask(() => {
        listener(documentChanged(midGame(), 3) as never);
        listener(analysisUpdated(midGameAnalysis(3, 20)) as never);
      });
      return 1;
    }),
    unsubscribe: vi.fn().mockResolvedValue(undefined),
    subscribeFloatingBoardsChanged: vi.fn().mockResolvedValue(2),
    play: vi.fn().mockResolvedValue(4),
    step: vi.fn().mockResolvedValue(4),
    togglePondering: vi.fn().mockResolvedValue(undefined),
    previewVariation: vi.fn().mockResolvedValue({ revision: 3, node_id: 20, board: midGame().board, applied_moves: [], stones: [], stop_reason: null }),
    addRestrictionPoints: vi.fn().mockResolvedValue(null),
    clearAnalysisRestriction: vi.fn().mockResolvedValue(undefined),
    closeFloatingBoard: vi.fn().mockResolvedValue(BOARDS()),
    setFloatingBoardOptions: vi.fn().mockResolvedValue(BOARDS()),
    dragWindow: vi.fn().mockResolvedValue(undefined),
    resizeWindow: vi.fn().mockResolvedValue(undefined),
    setWindowSquare: vi.fn().mockResolvedValue(undefined),
  };
  return backend as unknown as Backend & typeof backend;
}

async function page(kind: "Main" | "Sub", options?: FloatingBoardsView) {
  const backend = pageBackend(options);
  render(<FloatingBoard kind={kind} backend={backend} />);
  await act(async () => undefined);
  await act(async () => undefined);
  return { backend, root: screen.getByTestId("floating-board") };
}

describe("a floating board's window", () => {
  test("关闭, 置顶 and 锁定 change its own window", async () => {
    const { backend } = await page("Main", BOARDS({ main: true, main_on_top: true }));
    fireEvent.click(screen.getByTestId("floating-close"));
    fireEvent.click(screen.getByTestId("floating-pin"));
    fireEvent.click(screen.getByTestId("floating-lock"));
    expect(backend.closeFloatingBoard).toHaveBeenCalledExactlyOnceWith("Main");
    expect(backend.setFloatingBoardOptions.mock.calls).toEqual([["Main", false, false], ["Main", true, true]]);
    expect(screen.getByTestId("floating-pin").getAttribute("aria-pressed")).toBe("true");
  });

  test("a press that moves drags the window; a still press does not; Alt+drag is the board's", async () => {
    const { backend, root } = await page("Main");
    fireEvent.pointerDown(root, { button: 0, clientX: 100, clientY: 100 });
    fireEvent.pointerMove(root, { buttons: 1, clientX: 100 + DRAG_THRESHOLD - 1, clientY: 100 });
    expect(backend.dragWindow).not.toHaveBeenCalled();
    fireEvent.pointerMove(root, { buttons: 1, clientX: 100 + DRAG_THRESHOLD + 1, clientY: 100 });
    fireEvent.pointerMove(root, { buttons: 1, clientX: 150, clientY: 100 });
    expect(backend.dragWindow).toHaveBeenCalledOnce();
    fireEvent.pointerUp(root);
    fireEvent.pointerDown(root, { button: 0, altKey: true, clientX: 100, clientY: 100 });
    fireEvent.pointerMove(root, { buttons: 1, clientX: 160, clientY: 160 });
    expect(backend.dragWindow).toHaveBeenCalledOnce();
    // An edge sizes the window.
    fireEvent.pointerDown(root.querySelector('[data-edge="SouthEast"]')!);
    expect(backend.resizeWindow).toHaveBeenCalledExactlyOnceWith("SouthEast");
  });

  test("locked, it neither moves nor sizes", async () => {
    const { backend, root } = await page("Sub", BOARDS({ sub: true, sub_locked: true }));
    expect(root.querySelectorAll(".floating-edge")).toHaveLength(0);
    fireEvent.pointerDown(root, { button: 0, clientX: 100, clientY: 100 });
    fireEvent.pointerMove(root, { buttons: 1, clientX: 180, clientY: 100 });
    expect(backend.dragWindow).not.toHaveBeenCalled();
  });

  test("after a resize it is made square again, once the resizing stops", async () => {
    const { backend } = await page("Main");
    vi.useFakeTimers();
    Object.assign(window, { innerWidth: 640, innerHeight: 600 });
    fireEvent(window, new Event("resize"));
    act(() => vi.advanceTimersByTime(SQUARE_DELAY - 10));
    fireEvent(window, new Event("resize"));
    act(() => vi.advanceTimersByTime(SQUARE_DELAY - 10));
    expect(backend.setWindowSquare).not.toHaveBeenCalled();
    act(() => vi.advanceTimersByTime(20));
    expect(backend.setWindowSquare).toHaveBeenCalledExactlyOnceWith(600);
  });

  test("the main board's wheel and keys move through the game", async () => {
    const { backend } = await page("Main");
    fireEvent.wheel(screen.getByTestId("board"), { deltaY: 40, deltaMode: 0 });
    fireEvent.keyDown(window, { key: "ArrowUp" });
    fireEvent.keyDown(window, { key: " " });
    expect(backend.step.mock.calls).toEqual([[1], [-1]]);
    expect(backend.togglePondering).toHaveBeenCalledOnce();
  });

  test("the sub-board's wheel shows the best line one move less, or more", async () => {
    const { backend } = await page("Sub");
    const best = [...midGameAnalysis(3, 20).candidates].sort((a, b) => a.order - b.order)[0]!;
    expect(backend.previewVariation).toHaveBeenLastCalledWith(20, best.pv);
    const content = screen.getByTestId("floating-board").querySelector(".floating-content")!;
    await act(async () => fireEvent.wheel(content, { deltaY: -40, deltaMode: 0 }));
    expect(backend.previewVariation).toHaveBeenLastCalledWith(20, best.pv.slice(0, -1));
    await act(async () => fireEvent.wheel(content, { deltaY: 40, deltaMode: 0 }));
    expect(backend.previewVariation).toHaveBeenLastCalledWith(20, best.pv);
  });
});
