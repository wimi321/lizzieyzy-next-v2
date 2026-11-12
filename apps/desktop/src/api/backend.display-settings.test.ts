import { beforeEach, expect, expectTypeOf, test, vi } from "vitest";
import { boardView, defaultDisplaySettings, documentView } from "../../e2e/fixtures";
import type { DisplaySettings, DisplaySettingsPayload, VariationPreviewView, WireAppEvent } from "./backend";

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, Channel: class {} }));

beforeEach(() => {
  vi.resetModules();
  ipc.invoke.mockReset();
});

test("legacy required defaults remain assignable while display commands expose the new field", async () => {
  const { show_list_pane, ...legacyDefaults } = defaultDisplaySettings;
  const legacy: Required<DisplaySettings> = legacyDefaults;
  expect(legacy.show_coordinates).toBe(true);
  expect(show_list_pane).toBe(true);
  const backend = await import("./backend");
  expectTypeOf(backend.setDisplaySettings).returns.toEqualTypeOf<Promise<DisplaySettingsPayload>>();
  ipc.invoke.mockResolvedValueOnce({ ...legacy, show_list_pane: false });
  expect((await backend.setDisplaySettings(legacy)).show_list_pane).toBe(false);
});

test("display settings API forwards the complete object and preserves persistence errors", async () => {
  const backend = await import("./backend");
  const display: DisplaySettings = {
    ...defaultDisplaySettings, show_coordinates: false, show_sub_board: false, show_list_pane: false,
    move_numbers: "LastN", last_move_numbers: 23,
  };
  ipc.invoke.mockResolvedValueOnce(display);
  expect(await backend.setDisplaySettings(display)).toEqual(display);
  expect(ipc.invoke).toHaveBeenLastCalledWith("set_display_settings", { display });
  ipc.invoke.mockResolvedValueOnce({ version: 1, engine: null, pondering: true, display });
  expect((await backend.settings()).display).toEqual(display);
  expect(ipc.invoke).toHaveBeenLastCalledWith("settings");
  const error = { type: "App", data: "invalid display setting" };
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(backend.setDisplaySettings({ last_move_numbers: 0 })).rejects.toBe(error);
  // Every generated display preference remains optional for older callers.
  const legacy: DisplaySettings = {};
  ipc.invoke.mockResolvedValueOnce(defaultDisplaySettings);
  expect(await backend.setDisplaySettings(legacy)).toEqual(defaultDisplaySettings);
  expect(ipc.invoke).toHaveBeenLastCalledWith("set_display_settings", { display: {} });
});

test("mock settings fill display defaults and preserve explicitly scripted preferences", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const legacy = { version: 1, engine: null, pondering: true };
  const display: DisplaySettings = { show_coordinates: false, show_sub_board: false, move_numbers: "All" };
  mock.setScript({ commands: [
    { command: "settings", result: legacy },
    { command: "settings", result: legacy },
    { command: "settings", result: { ...legacy, display } },
    { command: "settings", result: { ...legacy, display: { show_list_pane: false } } },
  ] });
  const first = await mock.settings();
  expect(first.display).toEqual(defaultDisplaySettings);
  expect(first.display!.show_sub_board).toBe(true);
  expect(first.display!.show_list_pane).toBe(true);
  first.display!.show_coordinates = false;
  expect((await mock.settings()).display).toEqual(defaultDisplaySettings);
  const scripted = await mock.settings();
  expect(scripted.display).toEqual({ ...display, show_list_pane: true });
  expect(scripted.display!.show_sub_board).toBe(false);
  expect(scripted.display!.show_list_pane).toBe(true);
  expect((await mock.settings()).display!.show_list_pane).toBe(false);
});

test("mock display updates require a script and return only its reply", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const display: DisplaySettings = {
    ...defaultDisplaySettings, show_coordinates: false, show_sub_board: false, show_list_pane: false,
  };
  const reply: DisplaySettings = { ...display, last_move_numbers: 8 };
  const error = { type: "App", data: "settings could not be saved" } as const;
  mock.setScript({ commands: [
    { command: "setDisplaySettings", result: reply },
    { command: "setDisplaySettings", result: defaultDisplaySettings, error },
  ] });
  expect(await mock.setDisplaySettings(display)).toEqual(reply);
  await expect(mock.setDisplaySettings({ last_move_numbers: 0 })).rejects.toEqual(error);
  expect(mock.recordedCalls()).toEqual([
    { command: "setDisplaySettings", args: [display] },
    { command: "setDisplaySettings", args: [{ last_move_numbers: 0 }] },
  ]);
  await expect(mock.setDisplaySettings(display)).rejects.toThrow("Expected mock command (none)");
});

test("legacy mock documents and PV replies fill provenance arrays without calculating history", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const view = documentView(boardView(3, 2, [[0, 0, "Black"]]), 0);
  const preview: VariationPreviewView = {
    revision: 4, node_id: 0, board: view.board!, applied_moves: [], stones: [], stop_reason: null,
  };
  const event: WireAppEvent = { type: "DocumentChanged", data: { revision: 4, view } };
  mock.setScript({ initialEvents: [event], commands: [{ command: "previewVariation", result: preview }] });
  const listener = vi.fn();
  const subscription = await mock.subscribe(listener);
  try {
    const received = listener.mock.calls[0]![0].data.view;
    expect(received.board.stone_move_numbers).toEqual(Array(6).fill(null));
    expect(received.board.stone_node_ids).toEqual(Array(6).fill(null));
    expect(received.tree.branch_start_move_number).toBeNull();
    expect(view.board!.stone_move_numbers).toBeUndefined();
    expect(view.board!.stone_node_ids).toBeUndefined();
    expect(view.tree.branch_start_move_number).toBeUndefined();
    for (const reply of [await mock.previewVariation(0, []), await mock.previewVariation(0, [])]) {
      expect(reply.board.stone_move_numbers).toEqual(Array(6).fill(null));
      expect(reply.board.stone_node_ids).toEqual(Array(6).fill(null));
    }
    expect(listener).toHaveBeenCalledTimes(1);
    expect(mock.recordedCalls()).toEqual([{ command: "previewVariation", args: [0, []] }]);
  } finally {
    await mock.unsubscribe(subscription);
  }
});

test("mock preserves explicit board provenance and branch start in documents and PV replies", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const board = boardView(2, 2, [[0, 0, "Black"], [1, 1, "White"]]);
  board.stone_move_numbers = [1, null, null, 7];
  board.stone_node_ids = [9, null, null, 14];
  const view = documentView(board, 14);
  view.tree.branch_start_move_number = 6;
  const preview: VariationPreviewView = {
    revision: 7, node_id: 14,
    board: { ...board, stone_move_numbers: [1, null, null, 1], stone_node_ids: [9, null, null, null] },
    applied_moves: [], stones: [], stop_reason: null,
  };
  mock.setScript({
    initialEvents: [{ type: "DocumentChanged", data: { revision: 7, view } }],
    commands: [{ command: "previewVariation", result: preview }],
  });
  const listener = vi.fn();
  const subscription = await mock.subscribe(listener);
  try {
    const received = listener.mock.calls[0]![0].data.view;
    expect(received.board.stone_move_numbers).toEqual(board.stone_move_numbers);
    expect(received.board.stone_node_ids).toEqual(board.stone_node_ids);
    expect(received.tree.branch_start_move_number).toBe(6);
    expect(await mock.previewVariation(14, [])).toEqual(preview);
    const fallback = await mock.previewVariation(14, []);
    expect(fallback.board.stone_move_numbers).toEqual(board.stone_move_numbers);
    expect(fallback.board.stone_node_ids).toEqual(board.stone_node_ids);
  } finally {
    await mock.unsubscribe(subscription);
  }
});
