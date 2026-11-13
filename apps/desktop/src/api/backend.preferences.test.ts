import { beforeEach, expect, test, vi } from "vitest";
import type { Settings } from "./backend";

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, Channel: class {} }));

beforeEach(() => {
  vi.resetModules();
  ipc.invoke.mockReset();
});

test("preference commands remain optional and forward arguments, results and errors", async () => {
  const api = await import("./backend");
  for (const language of ["System", "ZhCn", "En"] as const) {
    ipc.invoke.mockResolvedValueOnce(language);
    expect(await api.setLanguage(language)).toBe(language);
    expect(ipc.invoke).toHaveBeenLastCalledWith("set_language", { language });
  }
  ipc.invoke.mockResolvedValueOnce(false);
  expect(await api.setAutoCheckUpdates(false)).toBe(false);
  expect(ipc.invoke).toHaveBeenLastCalledWith("set_auto_check_updates", { enabled: false });
  const error = { type: "App", data: "settings could not be saved" };
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(api.setLanguage("En")).rejects.toBe(error);
});

test("mock preferences require scripts and do not simulate persistence", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  mock.setScript({ commands: [
    { command: "setLanguage", result: "En" },
    { command: "setAutoCheckUpdates", result: false },
  ] });
  expect(await mock.setLanguage("En")).toBe("En");
  expect(await mock.setAutoCheckUpdates(false)).toBe(false);
  expect(mock.recordedCalls()).toEqual([
    { command: "setLanguage", args: ["En"] },
    { command: "setAutoCheckUpdates", args: [false] },
  ]);
  await expect(mock.setLanguage("ZhCn")).rejects.toThrow("Expected mock command (none)");
  await expect(mock.setAutoCheckUpdates(true)).rejects.toThrow("Expected mock command (none)");
});

test("new settings defaults preserve the serialized legacy payload and explicit values", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const legacy = { version: 1, engine: null, pondering: true };
  mock.setScript({ commands: [
    { command: "settings", result: legacy },
    { command: "settings", result: { ...legacy, language: "En", auto_check_updates: false, last_update_check_unix_ms: 0 } },
  ] });
  const defaults = await mock.settings();
  expect(defaults.language).toBe("System");
  expect(defaults.auto_check_updates).toBe(true);
  expect(defaults.last_update_check_unix_ms).toBeNull();
  const encoded = JSON.parse(JSON.stringify(defaults)) as Settings;
  expect(encoded).not.toHaveProperty("language");
  expect(encoded).not.toHaveProperty("auto_check_updates");
  expect(encoded).not.toHaveProperty("last_update_check_unix_ms");
  const explicit = await mock.settings();
  expect(explicit.language).toBe("En");
  expect(explicit.auto_check_updates).toBe(false);
  expect(explicit.last_update_check_unix_ms).toBe(0);
  expect(JSON.parse(JSON.stringify(explicit))).toMatchObject({
    language: "En", auto_check_updates: false, last_update_check_unix_ms: 0,
  });
});
