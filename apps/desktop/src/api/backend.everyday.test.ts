import { beforeEach, expect, test, vi } from "vitest";
import type { Settings } from "./backend";

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, Channel: class {} }));

beforeEach(() => {
  vi.resetModules();
  ipc.invoke.mockReset();
});

test("daily commands forward exact IPC arguments", async () => {
  const api = await import("./backend");
  for (const [name, args, command, payload, result] of [
    ["copySgf", [], "copy_sgf", undefined, undefined],
    ["pasteSgf", [], "paste_sgf", undefined, 7],
    ["saveCleanSgf", ["clean.sgf", true, false], "save_clean_sgf", { path: "clean.sgf", keepComments: true, currentBranch: false }, undefined],
    ["transformBoard", ["RotateRight"], "transform_board", { transform: "RotateRight" }, 8],
    ["alwaysOnTop", [], "always_on_top", undefined, false],
    ["setAlwaysOnTop", [true], "set_always_on_top", { enabled: true }, true],
    ["uiScale", [], "ui_scale", undefined, 1],
    ["setUiScale", [1.5], "set_ui_scale", { scale: 1.5 }, 1.5],
    ["playSound", [], "play_sound", undefined, true],
    ["setPlaySound", [false], "set_play_sound", { enabled: false }, false],
  ] as const) {
    ipc.invoke.mockResolvedValueOnce(result);
    const call = api[name] as (...args: readonly unknown[]) => Promise<unknown>;
    expect(await call(...args)).toBe(result);
    expect(ipc.invoke).toHaveBeenLastCalledWith(...(payload === undefined ? [command] : [command, payload]));
  }
  const error = { type: "Clipboard", data: { type: "EmptyText" } };
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(api.pasteSgf()).rejects.toBe(error);
});

test("PNG commands send the original binary body and encode Unicode paths in a header", async () => {
  const api = await import("./backend");
  // Keep a nonzero byte offset: passing .buffer would also include the sentinel bytes.
  const bytes = new Uint8Array([99, 137, 80, 78, 71, 255, 99]).subarray(1, 6);
  ipc.invoke.mockResolvedValue(undefined);
  for (const path of ["棋谱 截图.png", "C:\\围棋 棋谱\\对局 + 100% #1.png", "/tmp/棋盘 导出.png"]) {
    await api.savePng(path, bytes);
    expect(ipc.invoke).toHaveBeenLastCalledWith("save_png", bytes, {
      headers: { "x-lizzieyzy-path": encodeURIComponent(path) },
    });
    const [, body, options] = ipc.invoke.mock.lastCall!;
    expect(body).toBe(bytes);
    expect(body).toBeInstanceOf(Uint8Array);
    expect(decodeURIComponent(options.headers["x-lizzieyzy-path"])).toBe(path);
  }
  await api.copyPng(bytes);
  expect(ipc.invoke).toHaveBeenLastCalledWith("copy_png", bytes);
  expect(ipc.invoke.mock.lastCall![1]).toBe(bytes);
});

test("daily mock queries have defaults and writes require scripts", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  mock.setScript({ commands: [
    { command: "copySgf", result: undefined },
    { command: "pasteSgf", result: 7 },
    { command: "saveCleanSgf", result: undefined },
    { command: "savePng", result: undefined },
    { command: "copyPng", result: undefined },
    { command: "transformBoard", result: 8 },
    { command: "setAlwaysOnTop", result: true },
    { command: "setUiScale", result: 1.5 },
    { command: "setPlaySound", result: false },
    { command: "alwaysOnTop", result: true },
    { command: "uiScale", result: 1.25 },
    { command: "playSound", result: false },
  ] });
  expect(await mock.alwaysOnTop()).toBe(false);
  expect(await mock.uiScale()).toBe(1);
  expect(await mock.playSound()).toBe(true);
  await mock.copySgf();
  expect(await mock.pasteSgf()).toBe(7);
  await mock.saveCleanSgf("clean.sgf", false, true);
  const bytes = new Uint8Array([137, 80, 78, 71]);
  const path = "棋盘 导出.png";
  await mock.savePng(path, bytes);
  await mock.copyPng(bytes);
  expect(mock.recordedCalls()).toContainEqual({ command: "savePng", args: [path, bytes] });
  expect(mock.recordedCalls()).toContainEqual({ command: "copyPng", args: [bytes] });
  expect(await mock.transformBoard("FlipHorizontal")).toBe(8);
  expect(await mock.setAlwaysOnTop(true)).toBe(true);
  expect(await mock.setUiScale(1.5)).toBe(1.5);
  expect(await mock.setPlaySound(false)).toBe(false);
  expect(await mock.alwaysOnTop()).toBe(true);
  expect(await mock.uiScale()).toBe(1.25);
  expect(await mock.playSound()).toBe(false);
  expect(mock.recordedCalls()).toContainEqual({ command: "saveCleanSgf", args: ["clean.sgf", false, true] });
  for (const operation of [
    () => mock.copySgf(), () => mock.pasteSgf(),
    () => mock.saveCleanSgf("clean.sgf", false, false),
    () => mock.savePng("board.png", new Uint8Array()), () => mock.copyPng(new Uint8Array()),
    () => mock.transformBoard("RotateLeft"),
    () => mock.setAlwaysOnTop(false), () => mock.setUiScale(1), () => mock.setPlaySound(true),
  ]) await expect(operation()).rejects.toThrow("Expected mock command (none)");
});

test("daily setting defaults preserve old payloads and explicit script values", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const legacy = { version: 1, engine: null, pondering: true };
  mock.setScript({ commands: [
    { command: "settings", result: legacy },
    { command: "settings", result: { ...legacy, always_on_top: true, ui_scale: 2, play_sound: false } },
  ] });
  const defaults = await mock.settings();
  expect([defaults.always_on_top, defaults.ui_scale, defaults.play_sound]).toEqual([false, 1, true]);
  const encoded = JSON.parse(JSON.stringify(defaults)) as Settings;
  for (const key of ["always_on_top", "ui_scale", "play_sound"]) expect(encoded).not.toHaveProperty(key);
  const explicit = await mock.settings();
  expect(JSON.parse(JSON.stringify(explicit))).toMatchObject({ always_on_top: true, ui_scale: 2, play_sound: false });
});
