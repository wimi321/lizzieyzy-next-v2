import { act, cleanup, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";
import en from "../locales/en.json";
import zh from "../locales/zh-CN.json";
import { BottomToolbar, type BottomToolbarProps } from "../frame/BottomToolbar";
import { inFrameLayout } from "../frame/layout";
import { MainPanel } from "../frame/MainPanel";
import { TopToolbar, type TopToolbarProps } from "../frame/TopToolbar";
import { matchShortcut } from "../menu/shortcuts";
import { AutoPlayDialog } from "./AutoPlayDialog";
import { ControlPanel } from "./ControlPanel";
import { PaintOverlay } from "./PaintOverlay";
import { SavePanel, type SavePanelProps } from "./SavePanel";
import { useAutoPlay } from "./useAutoPlay";
import { filledSlot, type Slot, slotError, useSaveSlots } from "./useSaveSlots";
import type { ResearchToolbar } from "./useResearch";
import { intervalOf, placed } from "./values";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("自动播放", () => {
  function playing(start: { current: number; atEnd?: boolean }) {
    const step = vi.fn();
    const hook = renderHook(({ current, atEnd }) => useAutoPlay(step, current, atEnd), { initialProps: { current: start.current, atEnd: start.atEnd ?? false } });
    return { step, hook };
  }

  test("one move forward every interval, on a fake clock", () => {
    vi.useFakeTimers();
    const { step, hook } = playing({ current: 1 });
    act(() => hook.result.current.start(1500));
    expect(hook.result.current.playing).toBe(true);
    act(() => vi.advanceTimersByTime(1499));
    expect(step).not.toHaveBeenCalled();
    act(() => vi.advanceTimersByTime(1));
    expect(step).toHaveBeenCalledExactlyOnceWith(1);
    // Its own step lands; the next beat goes on from there.
    hook.rerender({ current: 2, atEnd: false });
    act(() => vi.advanceTimersByTime(1500));
    expect(step).toHaveBeenCalledTimes(2);
  });

  test("a move the user makes takes that beat", () => {
    vi.useFakeTimers();
    const { step, hook } = playing({ current: 1 });
    act(() => hook.result.current.start(1000));
    hook.rerender({ current: 7, atEnd: false });
    act(() => vi.advanceTimersByTime(1000));
    expect(step).not.toHaveBeenCalled();
    act(() => vi.advanceTimersByTime(1000));
    expect(step).toHaveBeenCalledOnce();
  });

  test("it stops at the branch's last move, and when asked", () => {
    vi.useFakeTimers();
    const { step, hook } = playing({ current: 1 });
    act(() => hook.result.current.start(1000));
    act(() => vi.advanceTimersByTime(1000));
    hook.rerender({ current: 2, atEnd: true });
    expect(hook.result.current.playing).toBe(false);
    act(() => vi.advanceTimersByTime(5000));
    expect(step).toHaveBeenCalledOnce();

    hook.rerender({ current: 2, atEnd: false });
    act(() => hook.result.current.start(1000));
    act(() => hook.result.current.stop());
    act(() => vi.advanceTimersByTime(5000));
    expect(step).toHaveBeenCalledOnce();
  });

  test("starting at the last move does nothing", () => {
    const { hook } = playing({ current: 4, atEnd: true });
    act(() => hook.result.current.start(1000));
    expect(hook.result.current.playing).toBe(false);
  });

  test("the interval is typed in seconds, decimals allowed, 0.1 to 60", () => {
    expect(intervalOf("1.5")).toBe(1500);
    expect(intervalOf(" 0,5 ")).toBe(500);
    expect(intervalOf(".25")).toBe(250);
    expect(intervalOf("60")).toBe(60_000);
    for (const bad of ["0.05", "61", "", "abc", "-1", "1e3"]) expect(intervalOf(bad)).toBeNull();
  });

  test("设置自动播放 starts with the typed interval and refuses a bad one", () => {
    const onStart = vi.fn();
    const onClose = vi.fn();
    render(<AutoPlayDialog intervalMs={2000} labels={zh.research.autoPlay} onStart={onStart} onClose={onClose} />);
    const field = screen.getByTestId("autoplay-interval");
    expect(field).toHaveProperty("value", "2");
    fireEvent.change(field, { target: { value: "99" } });
    expect(screen.getByTestId("autoplay-ok")).toHaveProperty("disabled", true);
    expect(screen.getByText(zh.research.autoPlay.invalid)).toBeTruthy();
    fireEvent.change(field, { target: { value: "0.8" } });
    fireEvent.click(screen.getByTestId("autoplay-ok"));
    expect(onStart).toHaveBeenCalledExactlyOnceWith(800);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalledOnce();
  });
});

describe("自由涂鸦", () => {
  function sheet(color: "Blue" | "Green" | "Red" = "Red") {
    const onColor = vi.fn();
    const onClose = vi.fn();
    render(<PaintOverlay color={color} labels={zh.research.paint} onColor={onColor} onClose={onClose} />);
    const svg = screen.getByRole("img", { name: zh.research.paint.title });
    const draw = (button: number, from: number, to: number) => {
      fireEvent.pointerDown(svg, { button, buttons: button === 0 ? 1 : 2, clientX: from, clientY: from, pointerId: 1 });
      fireEvent.pointerMove(svg, { button: -1, buttons: button === 0 ? 1 : 2, clientX: to, clientY: to, pointerId: 1 });
      fireEvent.pointerUp(svg, { button, buttons: 0, clientX: to, clientY: to, pointerId: 1 });
    };
    return { onColor, onClose, svg, draw, overlay: screen.getByTestId("paint-overlay") };
  }

  test("the left button draws a stroke in the pen's colour; the right one draws nothing", () => {
    const { svg, draw, overlay } = sheet("Blue");
    draw(2, 5, 30);
    expect(overlay.dataset.strokes).toBe("0");
    expect(svg.querySelectorAll("polyline")).toHaveLength(0);
    draw(0, 5, 30);
    expect(overlay.dataset.strokes).toBe("1");
    const line = svg.querySelector("polyline")!;
    expect(line.getAttribute("stroke")).toBe("#0000ff");
    expect(line.getAttribute("points")?.split(" ")).toHaveLength(2);
  });

  test("undo takes the last stroke, clear takes them all; a pen is a colour choice", () => {
    const { draw, overlay, onColor } = sheet();
    draw(0, 1, 2);
    draw(0, 3, 4);
    fireEvent.click(screen.getByTestId("paint-undo"));
    expect(overlay.dataset.strokes).toBe("1");
    fireEvent.click(screen.getByTestId("paint-clear"));
    expect(overlay.dataset.strokes).toBe("0");
    expect(screen.getByTestId("paint-undo")).toHaveProperty("disabled", true);
    expect(screen.getByTestId("paint-red").getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(screen.getByTestId("paint-green"));
    expect(onColor).toHaveBeenCalledExactlyOnceWith("Green");
  });

  test("Esc closes the sheet before anything under it sees the key", () => {
    const { onClose } = sheet();
    const below = vi.fn();
    window.addEventListener("keydown", below);
    fireEvent.keyDown(window, { key: "Escape" });
    window.removeEventListener("keydown", below);
    expect(onClose).toHaveBeenCalledOnce();
    expect(below).not.toHaveBeenCalled();
    fireEvent.click(screen.getByTestId("paint-close"));
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});

describe("独立棋局控制面板", () => {
  test("Java's eight buttons run their entries; 返回 and 涂鸦 wait for what they need", () => {
    const run = { first: vi.fn(), last: vi.fn(), back5: vi.fn(), forward5: vi.fn(), back: vi.fn(), forward: vi.fn(), paint: vi.fn() };
    render(
      <ControlPanel
        bounds={null}
        labels={zh.research.panel}
        actions={{ ...Object.fromEntries(Object.entries(run).map(([key, fn]) => [key, { run: fn }])), main: { run: vi.fn(), enabled: false } } as never}
        onBounds={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getAllByRole("button").slice(1).map((button) => button.textContent)).toEqual(["|<", ">|", "<<", ">>", "<", ">", "返回", "涂鸦"]);
    expect(screen.getByTestId("control-main")).toHaveProperty("disabled", true);
    for (const key of Object.keys(run) as (keyof typeof run)[]) fireEvent.click(screen.getByTestId(`control-${key}`));
    for (const fn of Object.values(run)) expect(fn).toHaveBeenCalledOnce();
  });

  test("dragging the title moves it, the corner sizes it; each is saved once, on release", () => {
    const onBounds = vi.fn();
    const shown = (bounds: { x: number; y: number; width: number; height: number }) => (
      <ControlPanel bounds={bounds} labels={zh.research.panel} actions={{} as never} onBounds={onBounds} onClose={vi.fn()} />
    );
    const { rerender } = render(shown({ x: 100, y: 80, width: 400, height: 300 }));
    const title = screen.getByTestId("control-panel-title");
    fireEvent.pointerDown(title, { button: 0, clientX: 150, clientY: 90, pointerId: 1 });
    fireEvent.pointerMove(title, { clientX: 170, clientY: 130, pointerId: 1 });
    expect(screen.getByTestId("control-panel").style.left).toBe("120px");
    expect(onBounds).not.toHaveBeenCalled();
    fireEvent.pointerUp(title, { button: 0, clientX: 170, clientY: 130, pointerId: 1 });
    expect(onBounds).toHaveBeenLastCalledWith({ x: 120, y: 120, width: 400, height: 300 });
    // The app saves them and shows the panel there.
    rerender(shown(onBounds.mock.lastCall![0]));
    const corner = screen.getByTestId("control-panel-size");
    fireEvent.pointerDown(corner, { button: 0, clientX: 500, clientY: 400, pointerId: 2 });
    fireEvent.pointerMove(corner, { clientX: 540, clientY: 450, pointerId: 2 });
    fireEvent.pointerUp(corner, { button: 0, clientX: 540, clientY: 450, pointerId: 2 });
    expect(onBounds).toHaveBeenLastCalledWith({ x: 120, y: 120, width: 440, height: 350 });
    expect(onBounds).toHaveBeenCalledTimes(2);
  });

  test("it opens where Java does, and never where it cannot be dragged back", () => {
    const view = { width: 1200, height: 800 };
    expect(placed(null, view)).toEqual({ x: 350, y: 170, width: 500, height: 460 });
    expect(placed({ x: 5000, y: -40, width: 50, height: 9000 }, view)).toEqual({ x: 1140, y: 0, width: 220, height: 800 });
    expect(placed({ x: -900, y: 10, width: 300, height: 300 }, view).x).toBe(-240);
  });
});

const BOARD = { width: 2, height: 2, stones: ["Black", null, null, "White"] as const };

function slot(id: string, extra: Partial<Slot> = {}): Slot {
  return filledSlot({ id, name: id, saved_at_unix_ms: 1_700_000_000_000, move_number: 12, board: { ...BOARD, stones: [...BOARD.stones] }, ...extra });
}

describe("存档与读档", () => {
  function panel(props: Partial<SavePanelProps> = {}) {
    const on = {
      onLoad: vi.fn(), onOverwrite: vi.fn(), onRename: vi.fn(), onDelete: vi.fn(), onDeleteAll: vi.fn(),
      onNew: vi.fn(), onAutosave: vi.fn(), onRestore: vi.fn(), onClose: vi.fn(),
    };
    render(<SavePanel slots={[slot("autosave", { is_autosave: true, name: "自动存档" }), slot("slot-a"), slot("slot-b", { corrupt: true, board: null })]} autosave restore={false} error={null} locale="zh-CN" labels={zh.research.saves} {...on} {...props} />);
    return on;
  }
  const card = (id: string) => screen.getAllByTestId("save-slot").find((element) => element.dataset.slotId === id)!;
  const buttons = (id: string) => [...card(id).querySelectorAll("button")].map((button) => button.textContent);

  test("the autosave only loads, a damaged slot only deletes; 新建存档 comes last", () => {
    panel();
    const labels = zh.research.saves;
    expect(screen.getAllByTestId("save-slot").map((element) => element.dataset.slotId)).toEqual(["autosave", "slot-a", "slot-b"]);
    expect(buttons("autosave")).toEqual([labels.load]);
    expect(buttons("slot-a")).toEqual([labels.load, labels.overwrite, labels.rename, labels.delete]);
    expect(buttons("slot-b")).toEqual([labels.delete]);
    expect(card("slot-b").textContent).toContain(labels.corrupt);
    expect(card("slot-a").textContent).toContain("第 12 手");
    expect(card("slot-a").querySelectorAll("circle")).toHaveLength(2);
    expect(screen.getByTestId("save-new")).toBe(card("slot-b").nextElementSibling);
  });

  test("overwrite, delete and delete all ask first; rename and load do not", () => {
    const on = panel();
    fireEvent.click(card("slot-a").querySelector('[data-testid="save-slot-overwrite"]')!);
    expect(screen.getByTestId("save-confirm").textContent).toContain(zh.research.saves.overwriteConfirm);
    fireEvent.click(screen.getByTestId("save-confirm-cancel"));
    expect(on.onOverwrite).not.toHaveBeenCalled();
    fireEvent.click(card("slot-a").querySelector('[data-testid="save-slot-overwrite"]')!);
    fireEvent.click(screen.getByTestId("save-confirm-ok"));
    expect(on.onOverwrite).toHaveBeenCalledExactlyOnceWith("slot-a");

    fireEvent.click(card("slot-b").querySelector('[data-testid="save-slot-delete"]')!);
    fireEvent.click(screen.getByTestId("save-confirm-ok"));
    expect(on.onDelete).toHaveBeenCalledExactlyOnceWith("slot-b");
    fireEvent.click(screen.getByTestId("save-delete-all"));
    expect(screen.getByTestId("save-confirm").textContent).toContain(zh.research.saves.deleteAllConfirm);
    fireEvent.click(screen.getByTestId("save-confirm-ok"));
    expect(on.onDeleteAll).toHaveBeenCalledOnce();

    const name = card("slot-a").querySelector("input")!;
    expect(card("slot-a").querySelector('[data-testid="save-slot-rename"]')).toHaveProperty("disabled", true);
    fireEvent.change(name, { target: { value: "  中盘  " } });
    fireEvent.click(card("slot-a").querySelector('[data-testid="save-slot-rename"]')!);
    expect(on.onRename).toHaveBeenCalledExactlyOnceWith("slot-a", "中盘");
    fireEvent.click(card("autosave").querySelector('[data-testid="save-slot-load"]')!);
    expect(on.onLoad).toHaveBeenCalledExactlyOnceWith("autosave");
  });

  test("a new save may go without a name; the switches say what is saved", () => {
    const on = panel();
    fireEvent.click(screen.getByTestId("save-new-button"));
    fireEvent.change(screen.getByTestId("save-new-name"), { target: { value: " 布局 " } });
    fireEvent.click(screen.getByTestId("save-new-button"));
    expect(on.onNew.mock.calls).toEqual([[""], ["布局"]]);
    expect(screen.getByTestId("save-new-name")).toHaveProperty("value", "");
    fireEvent.click(screen.getByTestId("save-autosave"));
    fireEvent.click(screen.getByTestId("save-restore"));
    expect(on.onAutosave).toHaveBeenCalledExactlyOnceWith(false);
    expect(on.onRestore).toHaveBeenCalledExactlyOnceWith(true);
  });

  test("Rust's errors read in the interface language", () => {
    const labels = zh.research.saves;
    expect(slotError({ type: "SaveSlots", data: { type: "Capacity" } }, labels)).toBe(labels.errors.Capacity);
    expect(slotError({ type: "SaveSlots", data: { type: "Io", data: { reason: "disk full" } } }, labels)).toBe("读写存档失败：disk full");
    expect(slotError({ type: "Corrupt", data: { id: "slot-b" } }, labels)).toBe(labels.errors.Corrupt);
    expect(slotError("boom", labels)).toBe("boom");
  });

  test("the list is read on opening and again after each change; a failure is said in the panel", async () => {
    const listed = [slot("slot-a")];
    const backend = {
      saveSlots: vi.fn().mockResolvedValue(listed),
      saveToNewSlot: vi.fn().mockResolvedValue(slot("slot-c")),
      overwriteSlot: vi.fn().mockRejectedValue({ type: "SaveSlots", data: { type: "Missing", data: { id: "slot-a" } } }),
      renameSlot: vi.fn(), loadSlot: vi.fn().mockResolvedValue(3), deleteSlot: vi.fn(), deleteAllSlots: vi.fn(),
      setAutosave: vi.fn().mockResolvedValue(false), setRestoreLastGame: vi.fn(),
    };
    const { result } = renderHook(() => useSaveSlots(backend, en.research.saves));
    act(() => result.current.load({ autosave: true, restore_last_game: true }));
    expect(result.current.panel.restore).toBe(true);
    await act(async () => result.current.show());
    expect(result.current.open).toBe(true);
    expect(result.current.panel.slots).toEqual(listed);
    await act(async () => result.current.panel.onNew(""));
    expect(backend.saveToNewSlot).toHaveBeenCalledWith("");
    expect(backend.saveSlots).toHaveBeenCalledTimes(2);
    await act(async () => result.current.panel.onOverwrite("slot-a"));
    expect(result.current.panel.error).toBe(en.research.saves.errors.Missing);
    await act(async () => result.current.panel.onAutosave(false));
    expect(result.current.panel.autosave).toBe(false);
    await act(async () => result.current.panel.onLoad("slot-a"));
    expect(backend.loadSlot).toHaveBeenCalledWith("slot-a");
    expect(result.current.open).toBe(true);
    act(() => result.current.toggle());
    expect(result.current.open).toBe(false);
  });

  test("the panel covers the list and the tree, or the right half when neither shows", () => {
    const node = <div data-testid="covering" />;
    const { rerender } = render(<MainPanel regions={{ list: <div />, tree: <div /> }} boardSize={19} cover={{ regions: ["list", "tree"], node }} />);
    const layout = inFrameLayout(1280, 720, 19, { comment: true, subBoard: true, tree: true, status: true, list: true });
    const top = Math.min(layout.list.y, layout.tree.y);
    const cover = screen.getByTestId("main-cover");
    expect(cover.style.top).toBe(`${top}px`);
    expect(cover.style.left).toBe(`${Math.min(layout.list.x, layout.tree.x)}px`);
    expect(cover.style.height).toBe(`${Math.max(layout.list.y + layout.list.height, layout.tree.y + layout.tree.height) - top}px`);
    expect(screen.getByTestId("covering")).toBeTruthy();
    rerender(<MainPanel regions={{}} hidden={["tree"]} boardSize={19} cover={{ regions: ["list", "tree"], node }} />);
    expect(screen.getByTestId("main-cover").style.left).toBe("640px");
    expect(screen.getByTestId("main-cover").style.width).toBe("640px");
  });
});

describe("the entries", () => {
  test("Ctrl+K or ⌘K searches, Ctrl+A plays; plain A still analyses", () => {
    const key = (code: string, mods: Partial<KeyboardEvent> = {}) =>
      matchShortcut({ key: "", code, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, repeat: false, ...mods });
    expect(key("KeyK", { ctrlKey: true })).toBe("functionSearch");
    expect(key("KeyK", { metaKey: true })).toBe("functionSearch");
    expect(key("KeyA", { ctrlKey: true })).toBe("makeAutoPlay");
    expect(key("KeyA")).toBe("autoAnalyze");
  });

  test("the top toolbar's 搜索功能, 控制面板 and 存档 work; 涂鸦 comes with the markup tools", () => {
    const research: ResearchToolbar = { onSearch: vi.fn(), controlPanel: true, onControlPanel: vi.fn(), saveLoad: false, onSaveLoad: vi.fn(), onPaint: vi.fn() };
    const noop = vi.fn();
    const props = {
      labels: zh.frame.top, hasDocument: true, hasBoard: true, offMainLine: false, komi: 7.5, markupOpen: false, trial: false, playColor: null,
      onNew: noop, onSetMain: noop, onMarkup: noop, onChangeTurn: noop, onColor: noop, onAlternate: noop, onKomi: noop, onBoardSize: noop, onOpen: noop,
      onSave: noop, onFox: noop, onTencent: noop, onStrength: noop, onAutoSetup: noop, onFlash: noop, onBackMain: noop, onPass: noop, onTsumego: noop, research,
    } satisfies TopToolbarProps;
    const { rerender } = render(<TopToolbar {...props} />);
    expect(screen.queryByTestId("tool-paint")).toBeNull();
    expect(screen.getByTestId("tool-control-panel").getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(screen.getByTestId("tool-search"));
    fireEvent.click(screen.getByTestId("tool-control-panel"));
    fireEvent.click(screen.getByTestId("tool-save-load"));
    rerender(<TopToolbar {...props} markupOpen />);
    fireEvent.click(screen.getByTestId("tool-paint"));
    for (const fn of [research.onSearch, research.onControlPanel, research.onSaveLoad, research.onPaint]) expect(fn).toHaveBeenCalledOnce();
  });

  test("the bottom 自动播放 is pressed while playing", () => {
    const onClick = vi.fn();
    const noop = vi.fn();
    const props: BottomToolbarProps = {
      labels: zh.frame.bottom, pondering: false, hasBoard: true, ownership: false, coordinates: true, canSetMain: false, canDelete: false, trial: false, autoAnalysis: null,
      onTogglePondering: noop, onToggleOwnership: noop, onToggleCoordinates: noop, onStep: noop, onSetMain: noop, onClear: noop, onDelete: noop, onTrial: noop, onRemote: noop,
      autoPlay: { playing: true, onClick },
    };
    render(<BottomToolbar {...props} />);
    const button = screen.getByTestId("bottom-auto-play");
    expect(button.getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(button);
    expect(onClick).toHaveBeenCalledOnce();
  });
});
