import { beforeEach, expect, test, vi } from "vitest";
import type { BenchmarkStatus, WireAppEvent } from "./backend";

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

const running: BenchmarkStatus = {
  phase: "Running", measurements: [], last_result: null, error: null,
  progress: { threads: 2, completed_positions: 3, total_positions: 4, visits_per_second: null, nn_evals_per_second: null },
};

test("manual benchmark commands pass through results and errors without frontend tuning", async () => {
  const backend = await import("./backend");
  const cases: [() => Promise<BenchmarkStatus>, string][] = [
    [backend.startBenchmark, "start_benchmark"],
    [backend.cancelBenchmark, "cancel_benchmark"],
    [backend.benchmarkStatus, "benchmark_status"],
    [backend.resetBenchmark, "reset_benchmark"],
  ];
  for (const [call, name] of cases) {
    ipc.invoke.mockResolvedValueOnce(running);
    expect(await call()).toEqual(running);
    expect(ipc.invoke).toHaveBeenLastCalledWith(name);
  }
  const error = { type: "App", data: "benchmark is already running" };
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(backend.startBenchmark()).rejects.toBe(error);
});

test("benchmark subscriptions share the ordered connection and do not reach board consumers", async () => {
  const backend = await import("./backend");
  ipc.invoke.mockImplementation(async (command) => {
    if (command === "subscribe") {
      ipc.channels[0]!.onmessage({ type: "BenchmarkProgress", data: running });
      return 36;
    }
  });
  const ui = vi.fn();
  const progress = vi.fn();
  const review = vi.fn();
  const ids = await Promise.all([
    backend.subscribe(ui), backend.subscribeBenchmarkProgress(progress), backend.subscribeWholeGameProgress(review),
  ]);
  expect(ipc.channels).toHaveLength(1);
  expect(progress.mock.calls).toEqual([[running]]);
  expect(ui).not.toHaveBeenCalled();
  expect(review).not.toHaveBeenCalled();
  await backend.unsubscribe(ids[0]!);
  await backend.unsubscribe(ids[2]!);
  expect(ipc.invoke).toHaveBeenCalledTimes(1);
  const cancelled = { ...running, phase: "Cancelled" } as const;
  ipc.channels[0]!.onmessage({ type: "BenchmarkProgress", data: cancelled });
  expect(progress).toHaveBeenLastCalledWith(cancelled);
  await backend.unsubscribe(ids[1]!);
  expect(ipc.invoke).toHaveBeenLastCalledWith("unsubscribe", { id: 36 });
  ipc.channels[0]!.onmessage({ type: "BenchmarkProgress", data: running });
  expect(progress).toHaveBeenCalledTimes(2);
});

test.each(["Completed", "Cancelled", "Failed"] as const)("mock benchmark demo replays %s, progress and reset", async (outcome) => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  mock.setScript(mock.benchmarkDemoScript(outcome));
  const ui = vi.fn();
  const progress = vi.fn();
  const progressId = await mock.subscribeBenchmarkProgress(progress);
  const uiId = await mock.subscribe(ui);
  expect((await mock.benchmarkStatus()).phase).toBe("Idle");
  expect((await mock.startBenchmark()).phase).toBe("Running");
  if (outcome === "Cancelled") {
    expect((await mock.benchmarkStatus()).phase).toBe("Running");
    expect((await mock.cancelBenchmark()).phase).toBe("Cancelled");
  }
  const terminal = await mock.benchmarkStatus();
  expect(terminal.phase).toBe(outcome);
  expect(progress.mock.calls.some(([status]) => (status as BenchmarkStatus).progress?.completed_positions === 3)).toBe(true);
  expect(progress.mock.calls.every(([status]) => !(status as BenchmarkStatus).progress || (status as BenchmarkStatus).progress?.total_positions === 4)).toBe(true);
  if (outcome === "Completed") {
    expect(terminal.last_result?.recommended_threads).toBe(4);
    expect(terminal.last_result?.visits_per_second).toBe(2448.9);
    expect(terminal.last_result?.nn_evals_per_second).toBe(2370.2);
    expect(terminal.measurements.find((row) => row.threads === 4)?.visits_per_second).toBe(1832.6);
    expect(terminal.last_result?.nn_max_batch_size).toBe(2);
    expect(terminal.last_result?.num_nn_server_threads_per_model).toBe(2);
    expect(terminal.last_result?.canonical_model_name).toContain("b11");
  } else {
    expect(terminal.last_result).toBeNull();
    expect(terminal.error === null).toBe(outcome !== "Failed");
  }
  expect((await mock.resetBenchmark()).phase).toBe("Idle");
  expect((await mock.benchmarkStatus()).last_result).toBeNull();
  expect(ui).not.toHaveBeenCalled();
  await mock.unsubscribe(uiId);
  await mock.unsubscribe(progressId);
});

test("mock without additional advice displays the recommended main-search measurement", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  mock.setScript(mock.benchmarkDemoScript("Completed", false));
  const subscription = await mock.subscribeBenchmarkProgress(() => {});
  await mock.startBenchmark();
  const status = await mock.benchmarkStatus();
  expect(status.phase).toBe("Completed");
  expect(status.last_result).toMatchObject({
    recommended_threads: 4, visits_per_second: 1832.6, nn_evals_per_second: 1774.3,
    nn_max_batch_size: null, num_nn_server_threads_per_model: null,
  });
  expect(status.measurements.find((row) => row.threads === 4)).toMatchObject({
    visits_per_second: status.last_result?.visits_per_second,
    nn_evals_per_second: status.last_result?.nn_evals_per_second,
  });
  await mock.unsubscribe(subscription);
});

test("benchmark IPC preserves backend-selected final-setting speeds and optional recommendations", async () => {
  const backend = await import("./backend");
  const result = {
    engine_fingerprint: "b11-profile", recommended_threads: 4,
    visits_per_second: 44.95, nn_evals_per_second: 44.27,
    measured_at_unix_ms: 1793404800000, canonical_model_name: null,
    nn_max_batch_size: 2, num_nn_server_threads_per_model: null,
  };
  const completed: BenchmarkStatus = { ...running, phase: "Completed", last_result: result };
  ipc.invoke.mockResolvedValueOnce(completed);
  expect(await backend.benchmarkStatus()).toEqual(completed);
  expect(ipc.invoke).toHaveBeenCalledWith("benchmark_status");

  // Missing fields in a snapshot made by an older UI remain compatible.
  const legacy = {
    engine_fingerprint: "older-profile", recommended_threads: 4,
    visits_per_second: 15.75, nn_evals_per_second: 15.99,
    measured_at_unix_ms: 1793404800000, canonical_model_name: null,
  };
  const oldStatus: BenchmarkStatus = { ...completed, last_result: legacy };
  ipc.invoke.mockResolvedValueOnce(oldStatus);
  expect(await backend.benchmarkStatus()).toEqual(oldStatus);
});
