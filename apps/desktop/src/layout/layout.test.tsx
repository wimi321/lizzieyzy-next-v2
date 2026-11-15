import { act, cleanup, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";
import type { CustomLayout, Settings } from "@/api/backend";
import { type Display, DISPLAY_DEFAULTS, hiddenRegions, withLayoutRules } from "../display/settings";
import zh from "../locales/zh-CN.json";
import type { MenuSubmenu } from "../menu/spec";
import { MenuBar } from "../menu/MenuBar";
import { matchShortcut } from "../menu/shortcuts";
import { customLayoutOf, displayOf, PRESETS, presetWindow, windowsOf } from "./presets";
import { type LayoutBackend, useLayout } from "./useLayout";

afterEach(cleanup);

const MEASURES = { innerWidth: 1429, innerHeight: 941, panelWidth: 1429, panelHeight: 833, bottomToolbar: 26 };
const PLACED = { bounds: { x: 40, y: 30, width: 1500, height: 960 }, maximized: false };

describe("布局模式", () => {
  test("each entry sets what Java's sets, and sizes the window by Java's share of its height", () => {
    expect(PRESETS.defaultView.patch).toMatchObject({ layout_mode: "Normal", show_comment: true, show_list_pane: true, show_status_bar: true, large_sub_board: false });
    expect(PRESETS.classicView.patch).toMatchObject({ layout_mode: "Normal", show_status_bar: false, show_comment: false, show_variation_graph: false, large_sub_board: true });
    expect(PRESETS.extraMode3.patch).toMatchObject({ layout_mode: "Thinking", show_comment: false });
    expect(PRESETS.extraMode1.patch).toMatchObject({ layout_mode: "MultiView", show_comment: false });
    expect(PRESETS.minView.patch).toMatchObject({ layout_mode: "Min", show_comment: false, show_sub_board: false });
    expect(PRESETS.minView.patch).not.toHaveProperty("show_status_bar");
    expect(presetWindow("defaultView", MEASURES)).toEqual({ width: Math.round(915 * 1.65), height: 941 });
    expect(presetWindow("classicView", MEASURES).width).toBe(Math.round(915 * 1.45));
    // 精简模式: a near square around the board, the toolbars kept.
    expect(presetWindow("minView", MEASURES)).toEqual({ width: Math.round(833 * 0.94), height: 941 });
  });

  test("the enlargements exclude each other and bring their panel; a panel switched on leaves 精简模式", () => {
    const base: Display = { ...DISPLAY_DEFAULTS, show_sub_board: false, large_winrate_graph: true };
    expect(withLayoutRules(base, { large_sub_board: true })).toMatchObject({ large_sub_board: true, large_winrate_graph: false, show_sub_board: true });
    expect(withLayoutRules(DISPLAY_DEFAULTS, { large_winrate_graph: true })).toMatchObject({ large_sub_board: false, show_list_pane: false, show_winrate_graph: true });
    expect(withLayoutRules(base, { show_list_pane: true }).large_winrate_graph).toBe(false);
    const min: Display = { ...DISPLAY_DEFAULTS, layout_mode: "Min", show_comment: false };
    expect(withLayoutRules(min, { show_comment: true }).layout_mode).toBe("Normal");
    expect(withLayoutRules(min, { show_coordinates: false }).layout_mode).toBe("Min");
    expect(withLayoutRules(min, { show_comment: false }).layout_mode).toBe("Min");
  });

  test("精简模式 shows the board alone; 思考模式 and 四方图 draw no separate sub-board", () => {
    expect(hiddenRegions({ ...DISPLAY_DEFAULTS, layout_mode: "Min" })).not.toContain("board");
    expect(hiddenRegions({ ...DISPLAY_DEFAULTS, layout_mode: "Min" })).toContain("comment");
    expect(hiddenRegions({ ...DISPLAY_DEFAULTS, layout_mode: "Thinking" })).toEqual(["subBoard"]);
    expect(hiddenRegions({ ...DISPLAY_DEFAULTS, layout_mode: "MultiView", show_sub_board: false })).toEqual(["subBoard"]);
  });

  test("a custom layout keeps the panels, the mode and the windows' places, and gives them back", () => {
    const now: Display = { ...DISPLAY_DEFAULTS, show_comment: false, layout_mode: "Thinking", board_position: 2, main_window: PLACED, floating_main: { x: 1, y: 2, width: 600, height: 600 } };
    const layout = customLayoutOf(now);
    expect(layout).toMatchObject({ layout_mode: "Thinking", board_position: 2, main_window: PLACED, panels: { show_comment: false, show_sub_board: true } });
    expect(displayOf(layout)).toMatchObject({ show_comment: false, layout_mode: "Thinking", board_position: 2, large_sub_board: false });
    expect(windowsOf(layout)).toEqual({ main_window: PLACED, floating_boards: now.floating_boards, floating_main: now.floating_main, floating_sub: null });
    // A window never moved has no place to keep: choosing the slot leaves it where it is.
    expect(windowsOf(customLayoutOf(DISPLAY_DEFAULTS)).main_window).toBeNull();
  });
});

function fakeBackend(saved: Partial<Display> = {}): LayoutBackend & { fitMainWindow: ReturnType<typeof vi.fn>; applyWindowLayout: ReturnType<typeof vi.fn>; resetWindowPositions: ReturnType<typeof vi.fn> } {
  return {
    fitMainWindow: vi.fn().mockResolvedValue(undefined),
    resetWindowPositions: vi.fn().mockResolvedValue(undefined),
    applyWindowLayout: vi.fn().mockResolvedValue(DISPLAY_DEFAULTS),
    settings: vi.fn().mockResolvedValue({ version: 1, engine: null, pondering: true, display: { ...DISPLAY_DEFAULTS, ...saved } } as Settings),
    previewVariation: vi.fn().mockResolvedValue(null),
  };
}

function withLayout(display: Display, backend = fakeBackend()) {
  const set = vi.fn();
  const hook = renderHook(({ now }) => useLayout(backend, now, set, zh.layout), { initialProps: { now: display } });
  return { set, hook, backend };
}

describe("the layout entries", () => {
  test("a mode entry sets the display and asks Rust for Java's window size; a checked mode chosen again goes back to the default", () => {
    const { set, hook, backend } = withLayout(DISPLAY_DEFAULTS);
    act(() => hook.result.current.bindings.extraMode3?.run?.());
    expect(set).toHaveBeenLastCalledWith(PRESETS.extraMode3.patch);
    expect(backend.fitMainWindow).toHaveBeenCalledOnce();
    hook.rerender({ now: { ...DISPLAY_DEFAULTS, layout_mode: "Thinking" } });
    expect(hook.result.current.bindings.extraMode3?.checked).toBe(true);
    act(() => hook.result.current.bindings.extraMode3?.run?.());
    expect(set).toHaveBeenLastCalledWith(PRESETS.defaultView.patch);
    // 双引擎模式 waits for the comparison engine.
    expect(hook.result.current.bindings.extraMode2).toBeUndefined();
  });

  test("放大 entries are checked only with their panel shown; the board moves in eighths, 0 to 8", () => {
    const { set, hook } = withLayout({ ...DISPLAY_DEFAULTS, large_sub_board: true, show_sub_board: false, board_position: 8 });
    expect(hook.result.current.bindings.largeSubBoard?.checked).toBe(false);
    act(() => hook.result.current.bindings.largeSubBoard?.run?.());
    expect(set).toHaveBeenLastCalledWith({ large_sub_board: true });
    expect(hook.result.current.bindings.rightMove?.enabled).toBe(false);
    act(() => hook.result.current.bindings.leftMove?.run?.());
    expect(set).toHaveBeenLastCalledWith({ board_position: 7 });
    act(() => hook.result.current.bindings.restoreDefaultPanelSizes?.run?.());
    expect(set).toHaveBeenLastCalledWith({ board_position: 4 });
  });

  test("重置界面位置 asks Rust at once", () => {
    const { hook, backend } = withLayout(DISPLAY_DEFAULTS);
    act(() => hook.result.current.bindings.deletePersistFile?.run?.());
    expect(backend.resetWindowPositions).toHaveBeenCalledOnce();
  });

  test("可视化面板设置 previews each change; 取消 puts the panels back as they were", () => {
    const { set, hook } = withLayout(DISPLAY_DEFAULTS);
    act(() => hook.result.current.bindings.visualizedPanelSettings?.run?.());
    expect(hook.result.current.dialogOpen).toBe(true);
    render(<>{hook.result.current.dialog}</>);
    expect(screen.getByRole("dialog", { name: "可视化面板设置" })).toBeTruthy();
    fireEvent.click(screen.getByTestId("panel-show_comment"));
    expect(set).toHaveBeenLastCalledWith({ show_comment: false });
    fireEvent.click(screen.getByText(zh.layout.dialog.cancel));
    expect(set).toHaveBeenLastCalledWith(expect.objectContaining({ show_comment: true, board_position: 4, layout_mode: "Normal" }));
    expect(hook.result.current.dialogOpen).toBe(false);
  });

  test("an empty slot opens 设置; 应用 keeps the window as it is with the places Rust saved; the slot then gives them back", async () => {
    const backend = fakeBackend({ main_window: PLACED });
    const { set, hook } = withLayout({ ...DISPLAY_DEFAULTS, show_comment: false }, backend);
    act(() => hook.result.current.bindings.customeMode2?.run?.());
    render(<>{hook.result.current.dialog}</>);
    expect(screen.getByRole("dialog", { name: "设置自定义布局 2" })).toBeTruthy();
    await act(async () => fireEvent.click(screen.getByTestId("panel-settings-apply")));
    const saved = set.mock.lastCall![0].custom_layouts as [CustomLayout | null, CustomLayout | null];
    expect(saved[0]).toBeNull();
    expect(saved[1]).toMatchObject({ main_window: PLACED, panels: { show_comment: false } });

    hook.rerender({ now: { ...DISPLAY_DEFAULTS, custom_layouts: saved } });
    act(() => hook.result.current.bindings.customeMode2?.run?.());
    expect(set).toHaveBeenLastCalledWith(expect.objectContaining({ show_comment: false }));
    expect(backend.applyWindowLayout).toHaveBeenCalledWith(expect.objectContaining({ main_window: PLACED }));
    // 设置 on a saved slot shows it first.
    act(() => hook.result.current.bindings.customeMode2?.aside?.run());
    expect(hook.result.current.dialogOpen).toBe(true);
  });

  test("the keys: Alt+1…5, Alt+8 / 9, Ctrl+F, the Control key's W, [ and ]", () => {
    const key = (code: string, mods: Partial<KeyboardEvent> = {}) =>
      matchShortcut({ key: "", code, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, repeat: false, ...mods });
    expect([1, 2, 3, 4, 5, 8, 9].map((digit) => key(`Digit${digit}`, { altKey: true }))).toEqual(["defaultView", "classicView", "minView", "extraMode3", "extraMode1", "customeMode1", "customeMode2"]);
    expect(key("KeyF", { ctrlKey: true })).toBe("largeSubBoard");
    expect(key("KeyF", { metaKey: true })).toBe("largeSubBoard");
    expect(key("KeyW", { ctrlKey: true })).toBe("largeWinrateGraph");
    // ⌘W stays the system's.
    expect(key("KeyW", { metaKey: true })).toBeNull();
    expect(key("BracketLeft")).toBe("leftMove");
    expect(matchShortcut({ key: "]", code: "", ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, repeat: false })).toBe("rightMove");
  });
});

describe("a menu row's own button", () => {
  test("设置 closes the menu and runs its own action, not the row's", () => {
    const row = vi.fn();
    const aside = vi.fn();
    const menus: MenuSubmenu[] = [{ kind: "submenu", id: "viewMenu", key: null, text: "View", items: [{ kind: "item", id: "customeMode1", key: null, text: "Custom 1" }] }];
    render(<MenuBar menus={menus} bindings={{ customeMode1: { run: row, aside: { label: "设置", run: aside } } }} labels={{}} label="menu" />);
    fireEvent.click(screen.getByRole("menuitem", { name: "View" }));
    fireEvent.click(screen.getByTestId("menu-aside-customeMode1"));
    expect(aside).toHaveBeenCalledOnce();
    expect(row).not.toHaveBeenCalled();
    expect(screen.queryByTestId("menu-aside-customeMode1")).toBeNull();
  });
});
