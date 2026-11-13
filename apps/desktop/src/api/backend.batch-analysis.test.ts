import { beforeEach, expect, test, vi } from "vitest";
import type { BatchAnalysisStatus, BatchOptions, Settings, WireAppEvent } from "./backend";

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

const progress: BatchAnalysisStatus = {
  phase: "Running", current_index: 0, reason: null,
  items: [{ id: 3, path: "/games/first.sgf", state: { type: "Running", data: { progress: null } } }],
};
const options: BatchOptions = {
  files: ["/games/first.sgf"], visits: 17, scope: { type: "Range", data: { first_move: 2, last_move: null } },
  force: true, output: { suffix: "已分析", directory: null },
};

test("batch IPC forwards options, controls, queue edits and typed failures", async () => {
  const backend = await import("./backend");
  const cases: [() => Promise<unknown>, string, object | undefined][] = [
    [() => backend.startBatchAnalysis(options), "start_batch_analysis", { options }],
    [backend.pauseBatchAnalysis, "pause_batch_analysis", undefined],
    [backend.resumeBatchAnalysis, "resume_batch_analysis", undefined],
    [backend.stopBatchAnalysis, "stop_batch_analysis", undefined],
    [() => backend.addBatchAnalysisFiles(["next.gib"]), "add_batch_analysis_files", { files: ["next.gib"] }],
    [() => backend.removeBatchAnalysisItem(3), "remove_batch_analysis_item", { id: 3 }],
    [() => backend.moveBatchAnalysisItem(3, "Up"), "move_batch_analysis_item", { id: 3, direction: "Up" }],
    [() => backend.moveBatchAnalysisItem(3, "Down"), "move_batch_analysis_item", { id: 3, direction: "Down" }],
    [() => backend.moveBatchAnalysisItem(3, "First"), "move_batch_analysis_item", { id: 3, direction: "First" }],
    [backend.clearBatchAnalysis, "clear_batch_analysis", undefined],
    [backend.batchAnalysisStatus, "batch_analysis_status", undefined],
    [backend.flashAnalysis, "flash_analysis", undefined],
    [() => backend.setFlashAnalysis({ visits: 17, always_override: true }), "set_flash_analysis", { settings: { visits: 17, always_override: true } }],
  ];
  for (const [call, name, args] of cases) {
    ipc.invoke.mockResolvedValueOnce(progress);
    expect(await call()).toBe(progress);
    expect(ipc.invoke).toHaveBeenLastCalledWith(...(args ? [name, args] : [name]));
  }
  const error = { type: "BatchAnalysisRunning" };
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(backend.openPath("another.sgf")).rejects.toBe(error);
});

test("batch progress uses the existing ordered connection and independent listeners", async () => {
  const backend = await import("./backend");
  ipc.invoke.mockImplementation(async (command) => {
    if (command === "subscribe") {
      ipc.channels[0]!.onmessage({ type: "BatchAnalysisProgress", data: progress });
      return 70;
    }
  });
  const ui = vi.fn();
  const batch = vi.fn();
  const review = vi.fn();
  const ids = await Promise.all([
    backend.subscribe(ui), backend.subscribeBatchAnalysisProgress(batch), backend.subscribeWholeGameProgress(review),
  ]);
  expect(ipc.channels).toHaveLength(1);
  expect(batch.mock.calls).toEqual([[progress]]);
  expect(ui).not.toHaveBeenCalled();
  expect(review).not.toHaveBeenCalled();
  await backend.unsubscribe(ids[0]!);
  await backend.unsubscribe(ids[2]!);
  const complete: BatchAnalysisStatus = { ...progress, phase: "Completed", current_index: null };
  ipc.channels[0]!.onmessage({ type: "BatchAnalysisProgress", data: complete });
  expect(batch).toHaveBeenLastCalledWith(complete);
  await backend.unsubscribe(ids[1]!);
  expect(ipc.invoke).toHaveBeenLastCalledWith("unsubscribe", { id: 70 });
});

test("mock batch mutations are scripted, status queries do not consume commands", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const paused: BatchAnalysisStatus = { ...progress, phase: "Paused" };
  mock.setScript({ commands: [
    { command: "startBatchAnalysis", result: progress, events: [{ type: "BatchAnalysisProgress", data: progress }] },
    { command: "pauseBatchAnalysis", result: paused, events: [{ type: "BatchAnalysisProgress", data: paused }] },
    { command: "resumeBatchAnalysis", result: progress },
    { command: "addBatchAnalysisFiles", result: progress },
    { command: "removeBatchAnalysisItem", result: progress },
    { command: "moveBatchAnalysisItem", result: progress },
    { command: "clearBatchAnalysis", result: progress },
    { command: "stopBatchAnalysis", result: { phase: "Stopped" } },
    { command: "setFlashAnalysis", result: { visits: 27, always_override: true } },
  ] });
  expect(await mock.batchAnalysisStatus()).toEqual({ phase: "Idle", items: [], current_index: null, reason: null });
  expect(await mock.flashAnalysis()).toEqual({ visits: 1, always_override: false });
  expect(mock.recordedCalls()).toEqual([]);
  const ui = vi.fn(); const batch = vi.fn();
  const uiId = await mock.subscribe(ui); const batchId = await mock.subscribeBatchAnalysisProgress(batch);
  expect(await mock.startBatchAnalysis(options)).toEqual(progress);
  expect(await mock.batchAnalysisStatus()).toEqual(progress);
  expect(await mock.pauseBatchAnalysis()).toEqual(paused);
  expect(await mock.batchAnalysisStatus()).toEqual(paused);
  await mock.resumeBatchAnalysis();
  await mock.addBatchAnalysisFiles(["second.gib"]);
  await mock.removeBatchAnalysisItem(8);
  await mock.moveBatchAnalysisItem(4, "First");
  await mock.clearBatchAnalysis();
  const stopped = await mock.stopBatchAnalysis();
  expect(stopped.phase).toBe("Stopped"); expect(stopped.items).toEqual([]);
  expect(stopped.current_index).toBeNull(); expect(stopped.reason).toBeNull();
  await mock.setFlashAnalysis({ visits: 27, always_override: true });
  expect(await mock.flashAnalysis()).toEqual({ visits: 27, always_override: true });
  expect(batch.mock.calls).toEqual([[progress], [paused]]);
  expect(ui).not.toHaveBeenCalled();
  expect(mock.recordedCalls()).toContainEqual({ command: "moveBatchAnalysisItem", args: [4, "First"] });
  for (const mutation of [
    () => mock.startBatchAnalysis(options), mock.pauseBatchAnalysis, mock.resumeBatchAnalysis,
    mock.stopBatchAnalysis, () => mock.addBatchAnalysisFiles([]), () => mock.removeBatchAnalysisItem(3),
    () => mock.moveBatchAnalysisItem(3, "Down"), mock.clearBatchAnalysis, () => mock.setFlashAnalysis({ visits: 1 }),
  ]) await expect(mutation()).rejects.toThrow("Expected mock command");
  await mock.unsubscribe(batchId); await mock.unsubscribe(uiId);
});

test("mock fills optional settings and view fields while preserving explicit values", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  mock.setScript({ commands: [
    { command: "settings", result: { engine: null } as Settings },
    { command: "settings", result: { engine: null, flash_analysis: { visits: 13, always_override: true } } as Settings },
    { command: "flashAnalysis", result: { always_override: true } },
    { command: "batchAnalysisStatus", result: { phase: "Paused", items: [{ path: "one.sgf" }] } },
  ] });
  const old = await mock.settings();
  expect(old.flash_analysis).toEqual({ visits: 1, always_override: false });
  expect((await mock.settings()).flash_analysis).toEqual({ visits: 13, always_override: true });
  const flash = await mock.flashAnalysis();
  expect(flash.visits).toBe(1); expect(flash.always_override).toBe(true);
  const status = await mock.batchAnalysisStatus();
  expect(status.items?.[0]?.id).toBe(0);
  expect(status.items?.[0]?.state).toEqual({ type: "Waiting" });
  expect(status.items?.[0]?.path).toBe("one.sgf");
  expect(status.phase).toBe("Paused");
  const observed = vi.fn(); const id = await mock.subscribeBatchAnalysisProgress(observed);
  mock.pushEvents([{ type: "BatchAnalysisProgress", data: { phase: "Completed" } }]);
  expect(observed.mock.calls[0]?.[0].items).toEqual([]);
  expect((await mock.batchAnalysisStatus()).phase).toBe("Completed");
  await mock.unsubscribe(id);
});
