import { beforeEach, expect, test, vi } from "vitest";
import type { CommandError, DisplaySettingsPayload, FloatingBoardsView, WindowCommands, WindowLayout, WireAppEvent } from "./backend";
import { defaultDisplaySettings, defaultFloatingBoards } from "../../e2e/fixtures";

interface FakeChannel { onmessage: (event: WireAppEvent) => void }
const ipc = vi.hoisted(() => ({ invoke: vi.fn(), channels: [] as FakeChannel[] }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: ipc.invoke,
  Channel: class implements FakeChannel {
    onmessage: (event: WireAppEvent) => void = () => {};
    constructor() { ipc.channels.push(this); }
  },
}));
beforeEach(() => { vi.resetModules(); ipc.invoke.mockReset(); ipc.channels.length = 0; });

const floating: FloatingBoardsView = {
  main: true, keep_main_board: false, sub: true,
  main_on_top: true, sub_on_top: false, main_locked: false, sub_locked: true,
};
const layout: WindowLayout = {
  main_window: { bounds: { x: -1200, y: 50, width: 800, height: 600 }, maximized: false },
  floating_boards: floating,
  floating_main: { x: 500, y: 50, width: 600, height: 600 },
  floating_sub: null,
};
const applied: DisplaySettingsPayload = { ...layout, layout_mode: "Thinking", board_position: 7 };
function windowCalls(api: Required<WindowCommands>) {
  return [
    { run: () => api.fitMainWindow(520, 400), wire: "fit_main_window", args: { width: 520, height: 400 }, result: undefined },
    { run: api.resetWindowPositions, wire: "reset_window_positions", args: undefined, result: undefined },
    { run: () => api.openFloatingBoard("Main"), wire: "open_floating_board", args: { kind: "Main" }, result: floating },
    { run: () => api.closeFloatingBoard("Sub"), wire: "close_floating_board", args: { kind: "Sub" }, result: floating },
    { run: () => api.setFloatingBoardOptions("Sub", false, true), wire: "set_floating_board_options", args: { kind: "Sub", onTop: false, locked: true }, result: floating },
    { run: () => api.setFloatingLayout(true, false, true), wire: "set_floating_layout", args: { main: true, keepMainBoard: false, sub: true }, result: floating },
    { run: () => api.applyWindowLayout(layout), wire: "apply_window_layout", args: { layout }, result: applied },
  ];
}

test("window APIs pass exact arguments, replies and failures to Rust", async () => {
  const api = await import("./backend");
  const failure: CommandError = { type: "WindowUnavailable", data: "unavailable" };
  for (const call of windowCalls(api)) {
    ipc.invoke.mockResolvedValueOnce(call.result);
    expect(await call.run()).toBe(call.result);
    expect(ipc.invoke.mock.calls.at(-1)).toEqual(call.args ? [call.wire, call.args] : [call.wire]);
    ipc.invoke.mockRejectedValueOnce(failure);
    await expect(call.run()).rejects.toBe(failure);
  }
});

test("layout and floating listeners share the webview channel and unsubscribe independently", async () => {
  const api = await import("./backend");
  ipc.invoke.mockResolvedValue(17);
  const original = vi.fn(); const boards = vi.fn(); const display = vi.fn();
  const ids = await Promise.all([
    api.subscribe(original), api.subscribeFloatingBoardsChanged(boards), api.subscribeDisplaySettingsChanged(display),
  ]);
  expect(ipc.channels).toHaveLength(1);
  const settings = { layout_mode: "Thinking" as const, board_position: 1 };
  ipc.channels[0]!.onmessage({ type: "FloatingBoardsChanged", data: floating });
  ipc.channels[0]!.onmessage({ type: "DisplaySettingsChanged", data: settings });
  const engine = { type: "EngineStatusChanged", data: { type: "Stopped" } } as const;
  ipc.channels[0]!.onmessage(engine);
  expect(boards.mock.calls).toEqual([[floating]]);
  expect(display.mock.calls).toEqual([[settings]]);
  expect(original.mock.calls).toEqual([[engine]]);
  await api.unsubscribe(ids[0]!); await api.unsubscribe(ids[1]!);
  expect(ipc.invoke).toHaveBeenCalledTimes(1);
  ipc.channels[0]!.onmessage({ type: "DisplaySettingsChanged", data: {} });
  expect(display).toHaveBeenCalledTimes(2);
  await api.unsubscribe(ids[2]!);
  expect(ipc.invoke).toHaveBeenLastCalledWith("unsubscribe", { id: 17 });
});

test("window mock commands require scripts, preserve payloads and replay both events", async () => {
  const mock = await import("../../e2e/mock-backend");
  const failure: CommandError = { type: "WindowUnavailable", data: "unavailable" };
  mock.setScript({ commands: [
    { command: "fitMainWindow", result: undefined },
    { command: "resetWindowPositions", result: undefined },
    { command: "openFloatingBoard", result: floating, events: [{ type: "FloatingBoardsChanged", data: floating }] },
    { command: "closeFloatingBoard", result: floating },
    { command: "setFloatingBoardOptions", result: floating },
    { command: "setFloatingLayout", result: floating },
    { command: "applyWindowLayout", result: applied },
    { command: "openFloatingBoard", result: {}, error: failure },
  ] });
  const boards = vi.fn(); const display = vi.fn(); const original = vi.fn();
  const ids = await Promise.all([
    mock.subscribeFloatingBoardsChanged(boards), mock.subscribeDisplaySettingsChanged(display), mock.subscribe(original),
  ]);
  try {
    for (const call of windowCalls(mock)) expect(await call.run()).toEqual(call.result);
    expect(mock.recordedCalls()).toEqual([
      { command: "fitMainWindow", args: [520, 400] },
      { command: "resetWindowPositions", args: [] },
      { command: "openFloatingBoard", args: ["Main"] },
      { command: "closeFloatingBoard", args: ["Sub"] },
      { command: "setFloatingBoardOptions", args: ["Sub", false, true] },
      { command: "setFloatingLayout", args: [true, false, true] },
      { command: "applyWindowLayout", args: [layout] },
    ]);
    await expect(mock.openFloatingBoard("Main")).rejects.toEqual(failure);
    for (const call of windowCalls(mock)) await expect(call.run()).rejects.toThrow("Expected mock command (none)");
    expect(boards).toHaveBeenLastCalledWith(floating);
    mock.pushEvents([
      { type: "FloatingBoardsChanged", data: { keep_main_board: true } },
      { type: "DisplaySettingsChanged", data: { board_position: 8 } },
    ]);
    expect(boards.mock.calls.at(-1)![0].main).toBe(false);
    expect(boards.mock.calls.at(-1)![0].keep_main_board).toBe(true);
    expect(display.mock.calls.at(-1)![0].layout_mode).toBe("Normal");
    expect(display.mock.calls.at(-1)![0].board_position).toBe(8);
    expect(original).not.toHaveBeenCalled();
  } finally { for (const id of ids) await mock.unsubscribe(id); }
});

test("applying window layouts replays scripted display values and errors without applying client rules", async () => {
  const mock = await import("../../e2e/mock-backend");
  const failure: CommandError = { type: "WindowUnavailable", data: "restore failed" };
  mock.setScript({ commands: [
    { command: "applyWindowLayout", result: { board_position: 0, floating_boards: { sub: true } } },
    { command: "applyWindowLayout", result: {}, error: failure },
  ] });
  const result = await mock.applyWindowLayout(layout);
  expect(result.board_position).toBe(0);
  expect(result.floating_boards?.sub).toBe(true);
  expect(result.floating_boards?.main).toBe(false);
  expect(result.layout_mode).toBe("Normal");
  expect(result.main_window).toBeNull();
  expect(result.custom_layouts).toEqual([null, null]);
  await expect(mock.applyWindowLayout(layout)).rejects.toEqual(failure);
  await expect(mock.applyWindowLayout(layout)).rejects.toThrow("Expected mock command (none)");
});

test("legacy display fixtures receive layout defaults while explicit and nested fields survive", async () => {
  const mock = await import("../../e2e/mock-backend");
  mock.setScript({ commands: [
    { command: "settings", result: { version: 1, engine: null, pondering: true } },
    { command: "setDisplaySettings", result: {
      layout_mode: "MultiView", large_sub_board: true, board_position: 0,
      floating_boards: { main: true, main_locked: true },
      main_window: { maximized: true, bounds: { x: -1280 } }, floating_sub: { x: -400 },
      custom_layouts: [{ panels: { show_comment: false }, floating_boards: { sub: true },
        main_window: { bounds: { x: -200 } }, floating_main: { width: 700 } }, null],
    } },
  ] });
  const defaults = (await mock.settings()).display!;
  expect(defaults.layout_mode).toBe("Normal");
  expect(defaults.large_sub_board).toBe(false); expect(defaults.large_winrate_graph).toBe(false);
  expect(defaults.board_position).toBe(4);
  expect(defaults.floating_boards).toEqual(defaultFloatingBoards);
  expect(defaults.custom_layouts).toEqual([null, null]);
  expect(defaults.main_window).toBeNull(); expect(defaults.floating_main).toBeNull(); expect(defaults.floating_sub).toBeNull();
  const explicit = await mock.setDisplaySettings({});
  expect(explicit.layout_mode).toBe("MultiView"); expect(explicit.large_sub_board).toBe(true);
  expect(explicit.board_position).toBe(0); expect(explicit.floating_boards?.main_locked).toBe(true);
  expect(explicit.floating_boards?.sub).toBe(false);
  expect(explicit.main_window?.maximized).toBe(true); expect(explicit.main_window?.bounds?.x).toBe(-1280);
  expect(explicit.main_window?.bounds?.width).toBe(1280); expect(explicit.floating_sub?.height).toBe(300);
  const layout = explicit.custom_layouts?.[0];
  expect(layout?.panels?.show_comment).toBe(false); expect(layout?.panels?.show_list_pane).toBe(true);
  expect(layout?.layout_mode).toBe("Normal"); expect(layout?.board_position).toBe(4);
  expect(layout?.main_window?.bounds?.x).toBe(-200); expect(layout?.main_window?.maximized).toBe(false);
  expect(layout?.floating_boards?.sub).toBe(true); expect(layout?.floating_main?.width).toBe(700);
  expect(layout?.floating_main?.height).toBe(600); expect(layout?.floating_sub).toBeNull();
  expect(defaultDisplaySettings.layout_mode).toBe("Normal");
  expect(Object.keys(defaultDisplaySettings)).not.toContain("layout_mode");
});
