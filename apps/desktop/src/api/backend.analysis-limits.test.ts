import { beforeEach, expect, test, vi } from "vitest";
import type { AnalysisLimitSettings, AnalysisState, WireAppEvent } from "./backend";

interface FakeChannel { onmessage: (event: WireAppEvent) => void }
const ipc = vi.hoisted(() => ({ invoke: vi.fn(), channels: [] as FakeChannel[] }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: ipc.invoke,
  Channel: class implements FakeChannel {
    onmessage: (event: WireAppEvent) => void = () => {};
    constructor() { ipc.channels.push(this); }
  },
}));

const defaults: AnalysisLimitSettings = {
  limit_time: true, max_analyze_time_seconds: 600, limit_playout: false, limit_playouts: 100000,
};
const limited: AnalysisState = { pondering: false, pause_reason: "TimeLimit" };
const enabled: AnalysisState = { pondering: true, pause_reason: null };

beforeEach(() => {
  vi.resetModules();
  ipc.invoke.mockReset();
  ipc.channels.length = 0;
});

test("analysis limit commands forward settings, effective state and persistence failures", async () => {
  const backend = await import("./backend");
  ipc.invoke.mockResolvedValueOnce(defaults);
  expect(await backend.analysisLimits()).toEqual(defaults);
  expect(ipc.invoke).toHaveBeenLastCalledWith("analysis_limits");
  const changed = { ...defaults, limit_playout: true, limit_playouts: 2000 };
  ipc.invoke.mockResolvedValueOnce(changed);
  expect(await backend.setAnalysisLimits(changed)).toEqual(changed);
  expect(ipc.invoke).toHaveBeenLastCalledWith("set_analysis_limits", { settings: changed });
  ipc.invoke.mockResolvedValueOnce(limited);
  expect(await backend.analysisState()).toEqual(limited);
  expect(ipc.invoke).toHaveBeenLastCalledWith("analysis_state");
  ipc.invoke.mockResolvedValueOnce(enabled);
  expect(await backend.togglePondering()).toEqual(enabled);
  expect(ipc.invoke).toHaveBeenLastCalledWith("toggle_pondering");
  const error = { type: "App", data: "settings could not be saved" };
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(backend.setAnalysisLimits(defaults)).rejects.toBe(error);
});

test("analysis state uses the existing ordered channel and preserves independent subscriptions", async () => {
  const backend = await import("./backend");
  ipc.invoke.mockResolvedValueOnce(39);
  const base = vi.fn();
  const states = vi.fn();
  const baseId = await backend.subscribe(base);
  const stateId = await backend.subscribeAnalysisState(states);
  expect(ipc.channels).toHaveLength(1);
  for (const state of [limited, enabled, { pondering: false, pause_reason: "User" } as const]) {
    ipc.channels[0]!.onmessage({ type: "AnalysisStateChanged", data: state });
  }
  expect(states.mock.calls).toEqual([[limited], [enabled], [{ pondering: false, pause_reason: "User" }]]);
  expect(base).not.toHaveBeenCalled();
  await backend.unsubscribe(baseId);
  expect(ipc.invoke).toHaveBeenCalledTimes(1);
  await backend.unsubscribe(stateId);
  expect(ipc.invoke).toHaveBeenLastCalledWith("unsubscribe", { id: 39 });
});

test("mock reads supply Java defaults without consuming or recording existing commands", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  mock.setScript({ commands: [
    { command: "settings", result: { version: 1, engine: null, pondering: true } },
    { command: "openBytes", result: 1 },
  ] });
  expect(await mock.analysisLimits()).toEqual(defaults);
  expect(await mock.analysisState()).toEqual(enabled);
  expect(mock.recordedCalls()).toEqual([]);
  expect((await mock.settings()).analysis_limits).toEqual(defaults);
  expect(await mock.analysisLimits()).toEqual(defaults);
  expect(await mock.openBytes([])).toBe(1);
  expect(mock.recordedCalls().map((call) => call.command)).toEqual(["settings", "openBytes"]);
});

test("mock replays limit reasons and settings without implementing analysis logic", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const changed = { ...defaults, limit_time: false, limit_playout: true, limit_playouts: 75 };
  mock.setScript({ initialEvents: [{ type: "AnalysisStateChanged", data: limited }], commands: [
    { command: "setAnalysisLimits", result: changed },
    { command: "analysisLimits", result: defaults },
    { command: "togglePondering", result: enabled, events: [{ type: "AnalysisStateChanged", data: enabled }] },
    { command: "analysisState", result: { pondering: false, pause_reason: "PlayoutLimit" } },
  ] });
  const states = vi.fn();
  const id = await mock.subscribeAnalysisState(states);
  expect(await mock.analysisState()).toEqual(limited);
  expect(await mock.setAnalysisLimits(changed)).toEqual(changed);
  expect(await mock.analysisLimits()).toEqual(defaults);
  expect(await mock.analysisLimits()).toEqual(defaults);
  expect(await mock.togglePondering()).toEqual(enabled);
  expect(await mock.analysisState()).toEqual({ pondering: false, pause_reason: "PlayoutLimit" });
  expect(states.mock.calls).toEqual([[limited], [enabled]]);
  await mock.unsubscribe(id);
  mock.setScript({});
  expect(await mock.analysisState()).toEqual(enabled);
  expect(await mock.analysisLimits()).toEqual(defaults);
});
