import { beforeEach, expect, test, vi } from "vitest";
import type { AnalysisView, WholeGameProgress, WireAppEvent } from "./backend";

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

const progress: WholeGameProgress = {
  completed: 2, total: 3, visits: 1000, status: { type: "Running" },
};
const result: AnalysisView = {
  node_id: 2, revision: 7, winrate: 0.6, score_lead: 1.5, visits: 1000,
  summary: { winrate: 0.6, score_lead: 1.5, visits: 1000 },
  visits_per_second: null,
  candidates: [{
    coordinate: "pass", point: null, visits: 700, winrate: 0.6, score_lead: 1.5,
    prior: 0.2, order: 0, pv: [{ type: "Pass", data: { color: "Black" } }],
  }],
  ownership: null, is_during_search: false, source: "Cache",
};

test("review IPC forwards default/custom options, controls, full results and errors", async () => {
  const backend = await import("./backend");
  const cases: [() => Promise<unknown>, string, Record<string, unknown> | undefined, unknown][] = [
    [() => backend.startWholeGameAnalysis(), "start_whole_game_analysis", { options: { visits: 1000 } }, progress],
    [() => backend.startWholeGameAnalysis({ visits: 3000 }), "start_whole_game_analysis", { options: { visits: 3000 } }, progress],
    [() => backend.pauseWholeGameAnalysis(), "pause_whole_game_analysis", undefined, progress],
    [() => backend.resumeWholeGameAnalysis(), "resume_whole_game_analysis", undefined, progress],
    [() => backend.cancelWholeGameAnalysis(), "cancel_whole_game_analysis", undefined, progress],
    [() => backend.wholeGameProgress(), "whole_game_progress", undefined, null],
    [() => backend.wholeGameResult(2), "whole_game_result", { nodeId: 2 }, result],
  ];
  for (const [call, name, args, reply] of cases) {
    ipc.invoke.mockResolvedValueOnce(reply);
    expect(await call()).toEqual(reply);
    expect(ipc.invoke).toHaveBeenLastCalledWith(...(args ? [name, args] : [name]));
  }
  ipc.invoke.mockRejectedValueOnce("visits out of range");
  await expect(backend.startWholeGameAnalysis({ visits: 499 })).rejects.toBe("visits out of range");
});

test("review and existing UI listeners share one channel and receive their own event types", async () => {
  const backend = await import("./backend");
  ipc.invoke.mockImplementation(async (command) => {
    // Rust can replay current progress before the subscribe reply arrives.
    if (command === "subscribe") {
      ipc.channels[0]!.onmessage({ type: "WholeGameProgress", data: progress });
      return 11;
    }
  });
  const ui = vi.fn();
  const review = vi.fn();
  const ids = await Promise.all([backend.subscribe(ui), backend.subscribeWholeGameProgress(review)]);
  expect(ipc.channels).toHaveLength(1);
  const engine = { type: "EngineStatusChanged", data: { type: "Stopped" } } as const;
  ipc.channels[0]!.onmessage(engine);
  expect(review.mock.calls).toEqual([[progress]]);
  expect(ui.mock.calls).toEqual([[engine]]);
  await backend.unsubscribe(ids[0]!);
  expect(ipc.invoke).toHaveBeenCalledTimes(1);
  const completed: WholeGameProgress = { ...progress, completed: 3, status: { type: "Completed" } };
  ipc.channels[0]!.onmessage({ type: "WholeGameProgress", data: completed });
  expect(review).toHaveBeenLastCalledWith(completed);
  await backend.unsubscribe(ids[1]!);
  expect(ipc.invoke).toHaveBeenLastCalledWith("unsubscribe", { id: 11 });
});

test("mock returns scripted review state and complete PV results without analysis logic", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const paused: WholeGameProgress = { ...progress, status: { type: "Paused" } };
  const cancelled: WholeGameProgress = { ...progress, status: { type: "Cancelled" } };
  mock.setScript({ commands: [
    { command: "startWholeGameAnalysis", result: progress, events: [{ type: "WholeGameProgress", data: progress }] },
    { command: "pauseWholeGameAnalysis", result: paused, events: [{ type: "WholeGameProgress", data: paused }] },
    { command: "resumeWholeGameAnalysis", result: progress },
    { command: "wholeGameResult", result },
    { command: "wholeGameProgress", result: progress },
    { command: "cancelWholeGameAnalysis", result: cancelled, events: [{ type: "WholeGameProgress", data: cancelled }] },
  ] });
  const ui = vi.fn();
  const review = vi.fn();
  const uiId = await mock.subscribe(ui);
  const reviewId = await mock.subscribeWholeGameProgress(review);
  expect(await mock.startWholeGameAnalysis()).toEqual(progress);
  expect(await mock.pauseWholeGameAnalysis()).toEqual(paused);
  expect(await mock.resumeWholeGameAnalysis()).toEqual(progress);
  expect(await mock.wholeGameResult(2)).toEqual(result);
  expect(await mock.wholeGameProgress()).toEqual(progress);
  expect(await mock.cancelWholeGameAnalysis()).toEqual(cancelled);
  expect(ui).not.toHaveBeenCalled();
  expect(review.mock.calls).toEqual([[progress], [paused], [cancelled]]);
  expect(mock.recordedCalls()).toEqual([
    { command: "startWholeGameAnalysis", args: [{ visits: 1000 }] },
    { command: "pauseWholeGameAnalysis", args: [] },
    { command: "resumeWholeGameAnalysis", args: [] },
    { command: "wholeGameResult", args: [2] },
    { command: "wholeGameProgress", args: [] },
    { command: "cancelWholeGameAnalysis", args: [] },
  ]);
  await mock.unsubscribe(uiId);
  await mock.unsubscribe(reviewId);
});
