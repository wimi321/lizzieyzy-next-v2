import { beforeEach, expect, test, vi } from "vitest";
import type { AnalysisCacheCommands } from "./backend";

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, Channel: class {} }));

beforeEach(() => {
  vi.resetModules();
  ipc.invoke.mockReset();
});

test("cache API forwards all scopes, persisted state and errors without frontend decisions", async () => {
  const backend = await import("./backend");
  const legacy: AnalysisCacheCommands = {};
  expect(legacy.clearAnalysis).toBeUndefined();
  ipc.invoke.mockResolvedValueOnce(true);
  expect(await backend.cacheEnabled()).toBe(true);
  expect(ipc.invoke).toHaveBeenLastCalledWith("cache_enabled");
  ipc.invoke.mockResolvedValueOnce(false);
  expect(await backend.setCacheEnabled(false)).toBe(false);
  expect(ipc.invoke).toHaveBeenLastCalledWith("set_cache_enabled", { enabled: false });
  for (const all of [false, true]) {
    ipc.invoke.mockResolvedValueOnce(undefined);
    expect(await backend.clearCache(all)).toBeUndefined();
    expect(ipc.invoke).toHaveBeenLastCalledWith("clear_cache", { all });
    ipc.invoke.mockResolvedValueOnce(12);
    expect(await backend.clearAnalysis(all)).toBe(12);
    expect(ipc.invoke).toHaveBeenLastCalledWith("clear_analysis", { all });
  }
  const error = { type: "App", data: "settings write failed" };
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(backend.setCacheEnabled(true)).rejects.toBe(error);
});

test("mock cache query uses explicit replies or true default and mutations require scripts", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  mock.setScript({ commands: [
    { command: "setCacheEnabled", result: false },
    { command: "clearCache", result: undefined },
    { command: "clearAnalysis", result: 9 },
    { command: "cacheEnabled", result: true },
  ] });
  expect(await mock.cacheEnabled()).toBe(true);
  expect(mock.recordedCalls()).toEqual([]);
  expect(await mock.setCacheEnabled(false)).toBe(false);
  expect(await mock.cacheEnabled()).toBe(false);
  expect(await mock.clearCache(false)).toBeUndefined();
  expect(await mock.clearAnalysis(true)).toBe(9);
  expect(await mock.cacheEnabled()).toBe(true);
  expect(mock.recordedCalls()).toEqual([
    { command: "setCacheEnabled", args: [false] },
    { command: "clearCache", args: [false] },
    { command: "clearAnalysis", args: [true] },
    { command: "cacheEnabled", args: [] },
  ]);
  await expect(mock.setCacheEnabled(true)).rejects.toThrow("Expected mock command (none)");
  await expect(mock.clearCache(true)).rejects.toThrow("Expected mock command (none)");
  await expect(mock.clearAnalysis(false)).rejects.toThrow("Expected mock command (none)");
});

test("mock settings supply legacy cache default and preserve explicit disabled state", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const legacy = { version: 1, engine: null, pondering: true };
  const error = { type: "App", data: "settings write failed" } as const;
  mock.setScript({ commands: [
    { command: "settings", result: legacy },
    { command: "settings", result: { ...legacy, cache_enabled: false } },
    { command: "setCacheEnabled", result: true, error },
  ] });
  expect((await mock.settings()).cache_enabled).toBe(true);
  expect((await mock.settings()).cache_enabled).toBe(false);
  expect(await mock.cacheEnabled()).toBe(false);
  await expect(mock.setCacheEnabled(true)).rejects.toEqual(error);
  expect(await mock.cacheEnabled()).toBe(false);
  mock.setScript({});
  expect(await mock.cacheEnabled()).toBe(true);
});
