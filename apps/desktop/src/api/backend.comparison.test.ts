import { beforeEach, expect, test, vi } from "vitest";
import type { ComparisonView, Settings, WireAppEvent } from "./backend";
import { analysisView, boardView, candidate, documentChanged, documentView } from "../../e2e/fixtures";

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

test("comparison IPC preserves selection, settings replies and typed conflicts", async () => {
  const api = await import("./backend");
  const cases: [() => Promise<unknown>, string, object?][] = [
    [api.comparisonState, "comparison_state"],
    [() => api.selectComparisonEngine("second"), "select_comparison_engine", { id: "second" }],
    [() => api.selectComparisonEngine(null), "select_comparison_engine", { id: null }],
    [api.restartComparisonEngine, "restart_comparison_engine"],
    [api.closeComparisonEngine, "close_comparison_engine"],
    [api.closeOtherEngines, "close_other_engines"],
    [api.closeAllEngines, "close_all_engines"],
  ];
  for (const [call, name, args] of cases) {
    const reply = { marker: name };
    ipc.invoke.mockResolvedValueOnce(reply);
    expect(await call()).toBe(reply);
    expect(ipc.invoke.mock.lastCall).toEqual(args === undefined ? [name] : [name, args]);
  }
  const sameMain = { type: "SameAsMain" };
  const sameComparison = { type: "SameAsComparison" };
  ipc.invoke.mockRejectedValueOnce(sameMain);
  await expect(api.selectComparisonEngine("main")).rejects.toBe(sameMain);
  ipc.invoke.mockRejectedValueOnce(sameComparison);
  await expect(api.selectMainEngine("second")).rejects.toBe(sameComparison);
});

test("comparison status and analysis use separate listeners on the existing channel", async () => {
  const api = await import("./backend");
  ipc.invoke.mockResolvedValue(96);
  const ui = vi.fn();
  const status = vi.fn();
  const analysis = vi.fn();
  const ids = await Promise.all([
    api.subscribe(ui), api.subscribeComparisonChanged(status), api.subscribeComparisonAnalysisUpdated(analysis),
  ]);
  const frame = analysisView(3, 5, []);
  const view: ComparisonView = { engine_id: "second", status: { type: "Ready", data: { version: "fake" } }, analysis: frame };
  ipc.channels[0]!.onmessage({ type: "ComparisonChanged", data: view });
  ipc.channels[0]!.onmessage({ type: "ComparisonAnalysisUpdated", data: frame });
  expect(ipc.channels).toHaveLength(1);
  expect(status.mock.calls).toEqual([[view]]);
  expect(analysis.mock.calls).toEqual([[frame]]);
  expect(ui).not.toHaveBeenCalled();
  ipc.channels[0]!.onmessage({ type: "AnalysisUpdated", data: frame });
  expect(ui).toHaveBeenCalledTimes(1);
  expect(analysis).toHaveBeenCalledTimes(1);
  for (const id of ids) await api.unsubscribe(id);
  expect(ipc.invoke).toHaveBeenLastCalledWith("unsubscribe", { id: 96 });
});

test("mock comparison state is a query and every comparison control requires a script", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const old = { version: 1, engine: null, pondering: true } as Settings;
  const selected = { ...old, comparison_engine_id: "second" };
  const operations: ["selectComparisonEngine" | "restartComparisonEngine" | "closeComparisonEngine" | "closeOtherEngines" | "closeAllEngines", () => Promise<Settings>][] = [
    ["selectComparisonEngine", () => mock.selectComparisonEngine("second")],
    ["restartComparisonEngine", mock.restartComparisonEngine],
    ["closeComparisonEngine", mock.closeComparisonEngine],
    ["closeOtherEngines", mock.closeOtherEngines],
    ["closeAllEngines", mock.closeAllEngines],
  ];
  mock.setScript({ commands: [
    { command: "settings", result: old },
    ...operations.map(([command]) => ({ command, result: selected })),
    { command: "comparisonState", result: { engine_id: "second", status: { type: "Starting" }, analysis: null } },
  ] });
  // Unscripted reads do not consume the next settings/control reply.
  expect(await mock.comparisonState()).toEqual({ engine_id: null, status: { type: "Stopped" }, analysis: null });
  expect((await mock.settings()).comparison_engine_id).toBeNull();
  for (const [, call] of operations) expect(await call()).toEqual(selected);
  expect((await mock.comparisonState()).status).toEqual({ type: "Starting" });
  for (const [, call] of operations) await expect(call()).rejects.toThrow("Expected mock command");
  expect(await mock.comparisonState()).toEqual({ engine_id: null, status: { type: "Stopped" }, analysis: null });
});

test("mock comparison events stay isolated and preserve saved analysis and document flags", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  mock.setScript({});
  const ui = vi.fn();
  const status = vi.fn();
  const analysis = vi.fn();
  const ids = await Promise.all([
    mock.subscribe(ui), mock.subscribeComparisonChanged(status), mock.subscribeComparisonAnalysisUpdated(analysis),
  ]);
  const board = boardView(19, 19, []);
  const frame = analysisView(0, 7, [candidate({ x: 3, y: 3 }, 0, 0.6, 2, 100, [], board)]);
  frame.source = "Saved";
  mock.pushEvents([
    { type: "ComparisonChanged", data: { engine_id: null, status: { type: "Stopped" }, analysis: frame } },
    { type: "ComparisonAnalysisUpdated", data: frame },
  ]);
  expect(ui).not.toHaveBeenCalled();
  expect(status.mock.lastCall?.[0].analysis.source).toBe("Saved");
  expect(analysis.mock.lastCall?.[0].source).toBe("Saved");
  expect(analysis.mock.lastCall?.[0].candidates[0].lcb).toBeNull();
  const view = documentView(board, 0);
  mock.pushEvents([documentChanged(view, 7)]);
  expect(ui.mock.lastCall?.[0].data.view.has_comparison_analysis).toBe(false);
  view.has_comparison_analysis = true;
  mock.pushEvents([documentChanged(view, 8)]);
  expect(ui.mock.lastCall?.[0].data.view.has_comparison_analysis).toBe(true);
  for (const id of ids) await mock.unsubscribe(id);
});
