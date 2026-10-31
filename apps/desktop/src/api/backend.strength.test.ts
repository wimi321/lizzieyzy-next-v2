import { beforeEach, expect, test, vi } from "vitest";
import { emptyStrengthEstimate, strengthView } from "../../e2e/fixtures";

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, Channel: class {} }));

beforeEach(() => {
  vi.resetModules();
  ipc.invoke.mockReset();
});

test("strength IPC forwards snapshots and errors without client calculation", async () => {
  const backend = await import("./backend");
  const view = emptyStrengthEstimate(7);
  ipc.invoke.mockResolvedValueOnce(view);
  expect(await backend.strengthEstimate()).toBe(view);
  expect(ipc.invoke).toHaveBeenLastCalledWith("strength_estimate", { bounds: null });
  const custom = strengthView(7);
  ipc.invoke.mockResolvedValueOnce(custom);
  expect(await backend.strengthEstimate([80, 20])).toBe(custom);
  expect(ipc.invoke).toHaveBeenLastCalledWith("strength_estimate", { bounds: [80, 20] });
  ipc.invoke.mockRejectedValueOnce("application closed");
  await expect(backend.strengthEstimate()).rejects.toBe("application closed");
});

test("mock strength snapshots follow scripts and preserve insufficient-data results", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const view = emptyStrengthEstimate(8);
  const next = { ...emptyStrengthEstimate(9), total_moves: 12, analyzed_moves: 4 };
  const progress = { completed: 4, total: 13, visits: 500, status: { type: "Running" } } as const;
  mock.setScript({ commands: [
    { command: "strengthEstimate", result: view, events: [{ type: "WholeGameProgress", data: progress }] },
    { command: "strengthEstimate", result: next },
    { command: "strengthEstimate", result: view, error: "application closed" },
  ] });
  const updates = vi.fn();
  const id = await mock.subscribeWholeGameProgress(updates);
  const received = await mock.strengthEstimate();
  expect(received).toEqual(view);
  expect(received.report.has_enough_data).toBe(false);
  expect(received.report.black.rank_label_zh).toBe("-");
  expect(received.report.white.rank_label_en).toBe("-");
  expect(received.phases.phases.every((phase) => !phase.played && !phase.black_ranked && !phase.white_ranked)).toBe(true);
  expect(updates).toHaveBeenLastCalledWith(progress);
  received.revision = 999;
  expect(await mock.strengthEstimate()).toEqual(next);
  await expect(mock.strengthEstimate()).rejects.toThrow("application closed");
  expect(mock.recordedCalls()).toEqual([
    { command: "strengthEstimate", args: [] },
    { command: "strengthEstimate", args: [] },
    { command: "strengthEstimate", args: [] },
  ]);
  await mock.unsubscribe(id);
});

test("mock forwards phase queries and returns scripted ranks without recalculating", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const view = strengthView(10);
  // The script models a backend reply to reversed bounds, not a client-side split.
  const custom = structuredClone(view);
  custom.phases.bounds = [80, 90];
  custom.phases.phases[0].last = 80;
  custom.phases.phases[1].first = 81;
  custom.phases.phases[1].last = 90;
  custom.phases.phases[2].first = 91;
  mock.setScript({ commands: [
    { command: "strengthEstimate", result: view },
    { command: "strengthEstimate", result: custom },
  ] });
  const received = await mock.strengthEstimate();
  expect(received.phases.phases.map((phase) => phase.black.rank_value)).toEqual([6.4, 4.1, 5.7]);
  expect(received.phases.phases.map((phase) => phase.white.rank_value)).toEqual([4.2, 5.2, 3.7]);
  expect(received.phases.black).toEqual({ best: 0, worst: 1, worth_reviewing: true });
  expect(received.phases.white).toEqual({ best: 1, worst: 2, worth_reviewing: true });
  expect(await mock.strengthEstimate([80, 20])).toEqual(custom);
  expect(mock.recordedCalls()).toEqual([
    { command: "strengthEstimate", args: [] },
    { command: "strengthEstimate", args: [[80, 20]] },
  ]);

  mock.setStrengthEstimate(view);
  const fallback = await mock.strengthEstimate([40, 120]);
  expect(fallback).toEqual(view);
  fallback.phases.phases[0].black.rank_value = -1;
  expect(await mock.strengthEstimate()).toEqual(view);
  mock.setStrengthEstimate(null);
});
