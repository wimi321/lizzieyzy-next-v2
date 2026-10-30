import { beforeEach, expect, test, vi } from "vitest";
import { emptyStrengthEstimate } from "../../e2e/fixtures";

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
  expect(ipc.invoke).toHaveBeenLastCalledWith("strength_estimate");
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
