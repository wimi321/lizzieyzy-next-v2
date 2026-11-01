import { beforeEach, expect, test, vi } from "vitest";
import type { WholeGameProgress, WireAppEvent } from "./backend";

interface FakeChannel { onmessage: (event: WireAppEvent) => void }
const ipc = vi.hoisted(() => ({ invoke: vi.fn(), channels: [] as FakeChannel[] }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: ipc.invoke,
  Channel: class implements FakeChannel {
    onmessage: (event: WireAppEvent) => void = () => {};
    constructor() { ipc.channels.push(this); }
  },
}));

beforeEach(() => {
  vi.resetModules();
  ipc.invoke.mockReset();
  ipc.channels.length = 0;
});

test("auto-curve settings forward reads, updates and persistence errors", async () => {
  const backend = await import("./backend");
  ipc.invoke.mockResolvedValueOnce(true);
  expect(await backend.autoQuickAnalyzeOnLoad()).toBe(true);
  expect(ipc.invoke).toHaveBeenLastCalledWith("auto_quick_analyze_on_load");
  ipc.invoke.mockResolvedValueOnce(false);
  expect(await backend.setAutoQuickAnalyzeOnLoad(false)).toBe(false);
  expect(ipc.invoke).toHaveBeenLastCalledWith("set_auto_quick_analyze_on_load", { enabled: false });
  const error = { type: "App", data: "settings could not be saved" };
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(backend.setAutoQuickAnalyzeOnLoad(true)).rejects.toBe(error);
});

test("automatic progress keeps its source through the shared event channel and query", async () => {
  const backend = await import("./backend");
  const progress: WholeGameProgress = {
    completed: 2, total: 4, visits: 1, source: "AutomaticQuick", status: { type: "Running" },
  };
  ipc.invoke.mockResolvedValueOnce(38);
  const listener = vi.fn();
  const id = await backend.subscribeWholeGameProgress(listener);
  ipc.channels[0]!.onmessage({ type: "WholeGameProgress", data: progress });
  expect(listener).toHaveBeenCalledWith(progress);
  ipc.invoke.mockResolvedValueOnce(progress);
  expect(await backend.wholeGameProgress()).toEqual(progress);
  await backend.unsubscribe(id);
});

test("mock auto-curve reads default to enabled without consuming unrelated commands", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  mock.setScript({ commands: [
    { command: "settings", result: { version: 1, engine: null, pondering: true } },
    { command: "setAutoQuickAnalyzeOnLoad", result: false },
    { command: "autoQuickAnalyzeOnLoad", result: true },
    { command: "openBytes", result: 1 },
  ] });
  expect(await mock.autoQuickAnalyzeOnLoad()).toBe(true);
  expect(mock.recordedCalls()).toEqual([]);
  expect((await mock.settings()).auto_quick_analyze_on_load).toBe(true);
  expect(await mock.autoQuickAnalyzeOnLoad()).toBe(true);
  expect(await mock.setAutoQuickAnalyzeOnLoad(false)).toBe(false);
  // A scripted read overrides the latest fixture value.
  expect(await mock.autoQuickAnalyzeOnLoad()).toBe(true);
  expect(await mock.autoQuickAnalyzeOnLoad()).toBe(true);
  expect(await mock.openBytes([])).toBe(1);
  expect(mock.recordedCalls().map((call) => call.command)).toEqual([
    "settings", "setAutoQuickAnalyzeOnLoad", "autoQuickAnalyzeOnLoad", "openBytes",
  ]);
});

test("mock settings and updates retain false and replay both progress sources", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const automatic: WholeGameProgress = {
    completed: 1, total: 4, visits: 1, source: "AutomaticQuick", status: { type: "Running" },
  };
  const manual: WholeGameProgress = { ...automatic, visits: 500, source: "Manual" };
  mock.setScript({ commands: [
    { command: "settings", result: { version: 1, engine: null, pondering: true, auto_quick_analyze_on_load: false } },
    { command: "setAutoQuickAnalyzeOnLoad", result: false },
    { command: "openBytes", result: 1, events: [{ type: "WholeGameProgress", data: automatic }] },
    { command: "startWholeGameAnalysis", result: manual, events: [{ type: "WholeGameProgress", data: manual }] },
  ] });
  expect((await mock.settings()).auto_quick_analyze_on_load).toBe(false);
  expect(await mock.autoQuickAnalyzeOnLoad()).toBe(false);
  expect(await mock.setAutoQuickAnalyzeOnLoad(false)).toBe(false);
  expect(await mock.autoQuickAnalyzeOnLoad()).toBe(false);
  const listener = vi.fn();
  const id = await mock.subscribeWholeGameProgress(listener);
  await mock.openBytes([]);
  expect(await mock.wholeGameProgress()).toEqual(automatic);
  await mock.startWholeGameAnalysis({ visits: 500 });
  expect(await mock.wholeGameProgress()).toEqual(manual);
  expect(listener.mock.calls).toEqual([[automatic], [manual]]);
  await mock.unsubscribe(id);
});
