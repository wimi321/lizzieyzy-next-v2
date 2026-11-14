import { beforeEach, expect, test, vi } from "vitest";
import type { WireAppEvent } from "./backend";
import { analysisUpdated, analysisView, boardView, candidate, defaultDisplaySettings, documentView } from "../../e2e/fixtures";

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, Channel: class {} }));
beforeEach(() => { vi.resetModules(); ipc.invoke.mockReset(); });

test("transient analysis controls forward to IPC and preserve typed conflicts", async () => {
  const backend = await import("./backend");
  ipc.invoke.mockResolvedValue(undefined);
  await backend.setAnalysisExtras({ policy: true, moves_ownership: false });
  expect(ipc.invoke).toHaveBeenLastCalledWith("set_analysis_extras", { extras: { policy: true, moves_ownership: false } });
  await backend.setPureNetwork(true);
  expect(ipc.invoke).toHaveBeenLastCalledWith("set_pure_network", { enabled: true });
  const error = { type: "WholeGameAnalysisActive" };
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(backend.setPureNetwork(true)).rejects.toBe(error);
});

test("mock new controls require scripts and never derive analysis", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  mock.setScript({ commands: [
    { command: "setAnalysisExtras", result: undefined },
    { command: "setPureNetwork", result: undefined, events: [{ type: "AnalysisStateChanged", data: { pondering: false, pause_reason: null, pure_network: true } }] },
  ] });
  await mock.subscribeAnalysisState(() => {});
  await mock.setAnalysisExtras({ policy: true });
  await mock.setPureNetwork(true);
  expect((await mock.analysisState()).pure_network).toBe(true);
  expect(mock.recordedCalls()).toEqual([
    { command: "setAnalysisExtras", args: [{ policy: true }] },
    { command: "setPureNetwork", args: [true] },
  ]);
  await expect(mock.setPureNetwork(false)).rejects.toThrow("Expected mock command (none)");
});

test("mock fills all display, state and analysis additions without replacing scripted values", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const view = analysisView(0, 0, [candidate({ x: 0, y: 1 }, 0, 0.6, 1, 2, [], boardView(2, 2, []))]);
  const explicit = { ...view, policy: [-1, 0.4, 0.3, 0.2, 0.1], candidates: view.candidates.map((value) => ({ ...value, ownership: [1, -1, 0, 0] })) };
  const events: WireAppEvent[] = [
    { type: "DocumentChanged", data: { revision: 0, view: documentView(boardView(2, 2, []), 0) } },
    analysisUpdated(view),
    { type: "AnalysisStateChanged", data: { pondering: true, pause_reason: null } },
  ];
  mock.setScript({ initialEvents: events, commands: [
    { command: "settings", result: { version: 1, engine: null, pondering: true, display: { show_coordinates: false } } },
    { command: "settings", result: { version: 1, engine: null, pondering: true, display: { estimate_location: "Both", moves_ownership: false, top_toolbar_hidden: ["undo"] } } },
    { command: "wholeGameResult", result: explicit },
    { command: "setDisplaySettings", result: { estimate_location: "Main" } },
  ] });
  const received: WireAppEvent[] = [];
  await mock.subscribe((event) => received.push(event));
  await mock.subscribeAnalysisState((state) => { expect(state.pure_network).toBe(false); });
  const analysis = received.find((event) => event.type === "AnalysisUpdated");
  expect(analysis?.type === "AnalysisUpdated" && analysis.data.policy).toBeNull();
  expect(analysis?.type === "AnalysisUpdated" && analysis.data.candidates[0]!.ownership).toBeNull();
  const old = (await mock.settings()).display!;
  for (const key of Object.getOwnPropertyNames(defaultDisplaySettings) as (keyof typeof defaultDisplaySettings)[]) {
    expect(old[key]).toEqual(key === "show_coordinates" ? false : defaultDisplaySettings[key]);
  }
  const scripted = (await mock.settings()).display!;
  expect(scripted.estimate_location).toBe("Both");
  expect(scripted.moves_ownership).toBe(false);
  expect(scripted.top_toolbar_hidden).toEqual(["undo"]);
  expect(await mock.wholeGameResult(0)).toEqual(explicit);
  const saved = await mock.setDisplaySettings({ estimate_location: "Main" });
  expect(saved.estimate_location).toBe("Main");
  expect(saved.pure_estimate_style).toBe("BigSquare");
});
