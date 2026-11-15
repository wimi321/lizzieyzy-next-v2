import { beforeEach, expect, test, vi } from "vitest";
import { defaultDisplaySettings } from "../../e2e/fixtures";
import type { CommandError, SaveSlot } from "./backend";

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, Channel: class {} }));

beforeEach(() => {
  vi.resetModules();
  ipc.invoke.mockReset();
});

test("save slot API passes exact commands and payloads to Rust", async () => {
  const api = await import("./backend");
  const slot: SaveSlot = { id: "slot-1", name: "保存" };
  for (const [call, command, args, result] of [
    [api.saveSlots, "save_slots", undefined, [slot]],
    [() => api.saveToNewSlot(" 保存 "), "save_to_new_slot", { name: " 保存 " }, slot],
    [() => api.overwriteSlot("slot-1"), "overwrite_slot", { id: "slot-1" }, slot],
    [() => api.renameSlot("slot-1", "新名称"), "rename_slot", { id: "slot-1", name: "新名称" }, slot],
    [() => api.loadSlot("slot-1"), "load_slot", { id: "slot-1" }, 7],
    [() => api.deleteSlot("slot-1"), "delete_slot", { id: "slot-1" }, undefined],
    [api.deleteAllSlots, "delete_all_slots", undefined, undefined],
    [() => api.setAutosave(false), "set_autosave", { enabled: false }, false],
    [() => api.setRestoreLastGame(true), "set_restore_last_game", { enabled: true }, true],
  ] as const) {
    ipc.invoke.mockResolvedValueOnce(result);
    expect(await call()).toBe(result);
    expect(ipc.invoke).toHaveBeenLastCalledWith(...(args === undefined ? [command] : [command, args]));
  }
  const error: CommandError = { type: "SaveSlots", data: { type: "Capacity" } };
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(api.saveToNewSlot("保存")).rejects.toBe(error);
});

test("all save slot mock commands require scripts and preserve replies and errors", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const slot: SaveSlot = {
    id: "slot-1", name: "原样", saved_at_unix_ms: 123, node_path: [1, 0], move_number: 2,
    board: { width: 2, height: 2, stones: ["Black", null, null, "White"] },
    is_autosave: false, corrupt: false,
  };
  const error: CommandError = { type: "SaveSlots", data: { type: "InvalidName" } };
  mock.setScript({ commands: [
    { command: "saveSlots", result: [slot] },
    { command: "saveToNewSlot", result: slot },
    { command: "overwriteSlot", result: slot },
    { command: "renameSlot", result: slot },
    { command: "loadSlot", result: 7 },
    { command: "deleteSlot", result: undefined },
    { command: "deleteAllSlots", result: undefined },
    { command: "setAutosave", result: false },
    { command: "setRestoreLastGame", result: true },
    { command: "renameSlot", result: {}, error },
  ] });
  expect(await mock.saveSlots()).toEqual([slot]);
  expect(await mock.saveToNewSlot("")).toEqual(slot);
  expect(await mock.overwriteSlot("slot-1")).toEqual(slot);
  expect(await mock.renameSlot("slot-1", "新名")).toEqual(slot);
  expect(await mock.loadSlot("slot-1")).toBe(7);
  await mock.deleteSlot("slot-1");
  await mock.deleteAllSlots();
  expect(await mock.setAutosave(false)).toBe(false);
  expect(await mock.setRestoreLastGame(true)).toBe(true);
  await expect(mock.renameSlot("slot-1", "")).rejects.toEqual(error);
  for (const call of [
    mock.saveSlots,
    () => mock.saveToNewSlot(""),
    () => mock.overwriteSlot("slot-1"),
    () => mock.renameSlot("slot-1", ""),
    () => mock.loadSlot("slot-1"),
    () => mock.deleteSlot("slot-1"),
    mock.deleteAllSlots,
    () => mock.setAutosave(true),
    () => mock.setRestoreLastGame(false),
  ]) await expect(call()).rejects.toThrow("Expected mock command");
});

test("save slot mock fills optional fields and research setting defaults only", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const legacy = { version: 1, engine: null, pondering: true };
  mock.setScript({ commands: [
    { command: "settings", result: legacy },
    { command: "settings", result: { ...legacy, autosave: false, restore_last_game: true,
      display: { autoplay_interval_ms: 100, painting_color: "Blue", control_panel_bounds: { x: -100, y: 20, width: 400, height: 600 } } } },
    { command: "saveSlots", result: [{ id: "broken", corrupt: true }] },
    { command: "saveToNewSlot", result: { board: { width: 2, height: 3 } } },
    { command: "setDisplaySettings", result: { control_panel_bounds: { x: -500 } } },
  ] });
  const defaults = await mock.settings();
  expect(defaults.autosave).toBe(true);
  expect(defaults.restore_last_game).toBe(false);
  expect(defaults.display?.autoplay_interval_ms).toBe(2000);
  expect(defaults.display?.painting_color).toBe("Red");
  expect(defaults.display?.control_panel_bounds).toBeNull();
  const explicit = await mock.settings();
  expect(explicit.autosave).toBe(false);
  expect(explicit.restore_last_game).toBe(true);
  expect(explicit.display?.autoplay_interval_ms).toBe(100);
  expect(explicit.display?.painting_color).toBe("Blue");
  expect(explicit.display?.control_panel_bounds?.x).toBe(-100);
  const slots = await mock.saveSlots();
  expect(slots[0]?.corrupt).toBe(true);
  expect(slots[0]?.node_path).toEqual([]);
  expect(slots[0]?.is_autosave).toBe(false);
  expect(slots[0]?.board).toBeNull();
  expect((await mock.saveToNewSlot("")).board?.stones).toEqual(Array(6).fill(null));
  const partial = await mock.setDisplaySettings({ control_panel_bounds: { x: -500 } });
  expect(partial.control_panel_bounds?.x).toBe(-500);
  expect(partial.control_panel_bounds?.y).toBe(0);
  expect(partial.control_panel_bounds?.width).toBe(640);
  expect(partial.control_panel_bounds?.height).toBe(480);
});

test("research display defaults preserve legacy fixture payloads and explicit writes", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const legacy = { ...defaultDisplaySettings, show_coordinates: false };
  for (const field of ["autoplay_interval_ms", "painting_color", "control_panel_bounds"] as const) {
    expect(Object.hasOwn(legacy, field)).toBe(false);
    expect(defaultDisplaySettings[field]).toBeDefined();
  }
  const explicit = { ...legacy, autoplay_interval_ms: 300, painting_color: "Green" as const,
    control_panel_bounds: { x: -10, y: 20, width: 500, height: 600 } };
  mock.setScript({ commands: [
    { command: "setDisplaySettings", result: legacy },
    { command: "setDisplaySettings", result: explicit },
  ] });
  const defaults = await mock.setDisplaySettings(legacy);
  expect(defaults.autoplay_interval_ms).toBe(2000);
  expect(defaults.painting_color).toBe("Red");
  expect(defaults.control_panel_bounds).toBeNull();
  expect(await mock.setDisplaySettings(explicit)).toEqual(explicit);
  expect(mock.recordedCalls()).toEqual([
    { command: "setDisplaySettings", args: [legacy] },
    { command: "setDisplaySettings", args: [explicit] },
  ]);
});
