import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { BenchmarkStatus } from "@/api/backend";
import zh from "../locales/zh-CN.json";
import { BenchmarkChip } from "./BenchmarkChip";
import { BenchmarkDialog } from "./BenchmarkDialog";

afterEach(cleanup);

const IDLE: BenchmarkStatus = { phase: "Idle", progress: null, measurements: [], last_result: null, error: null };
const MEASURED = [
  { threads: 4, completed_positions: 4, total_positions: 4, visits_per_second: 15.8, nn_evals_per_second: 16 },
  { threads: 6, completed_positions: 4, total_positions: 4, visits_per_second: 44.9, nn_evals_per_second: 44.3 },
];

function dialog(status: BenchmarkStatus | null) {
  const handlers = { onStart: vi.fn(), onStop: vi.fn(), onReset: vi.fn(), onClose: vi.fn() };
  render(<BenchmarkDialog status={status} model="kata1-tf3-b11c768" labels={zh.benchmark} {...handlers} />);
  return handlers;
}

test("before any run it says so, explains the wait and starts on request", () => {
  const handlers = dialog(IDLE);
  expect(screen.getByTestId("benchmark-state").textContent).toBe("尚未优化");
  expect(screen.getByTestId("benchmark-search-speed").textContent).toBe("—次/秒");
  expect(screen.getByText(zh.benchmark.duration)).toBeTruthy();
  expect(screen.getByText(zh.benchmark.b11)).toBeTruthy();
  fireEvent.click(screen.getByTestId("benchmark-start"));
  expect(handlers.onStart).toHaveBeenCalledTimes(1);
  expect(screen.getByTestId("benchmark-start").textContent).toBe("开始优化");
});

test("while running it shows the thread under test, what was tried, and stops; closing does not stop", () => {
  const handlers = dialog({ ...IDLE, phase: "Running", progress: { threads: 8, completed_positions: 2, total_positions: 4, visits_per_second: 41, nn_evals_per_second: 40 }, measurements: MEASURED });
  expect(screen.getByTestId("benchmark-progress").textContent).toContain("正在测试 8 线程：2/4 个局面");
  const rows = within(screen.getByTestId("benchmark-tried")).getAllByRole("listitem");
  expect(rows.map((row) => row.getAttribute("data-chosen"))).toEqual(["false", "true"]);
  expect(rows[1]?.textContent).toContain("6 线程");
  fireEvent.click(screen.getByTestId("benchmark-stop"));
  expect(handlers.onStop).toHaveBeenCalledTimes(1);
  fireEvent.click(screen.getByTestId("benchmark-close"));
  expect(handlers.onClose).toHaveBeenCalledTimes(1);
  expect(handlers.onStop).toHaveBeenCalledTimes(1);
});

test("a finished run shows the speeds of the settings in use, the threads and batch, and can be undone", () => {
  const handlers = dialog({
    ...IDLE,
    phase: "Completed",
    measurements: MEASURED,
    last_result: {
      engine_fingerprint: "f",
      recommended_threads: 4,
      nn_max_batch_size: 2,
      num_nn_server_threads_per_model: null,
      visits_per_second: 44.95,
      nn_evals_per_second: 44.27,
      measured_at_unix_ms: Date.UTC(2026, 9, 31, 11, 0),
      canonical_model_name: "kata1-tf3-b11c768-s12002M-d6304M.bin.gz",
    },
  });
  expect(screen.getByTestId("benchmark-state").textContent).toBe("优化完成");
  expect(screen.getByTestId("benchmark-model").textContent).toBe("kata1-tf3-b11c768");
  expect(screen.getByTestId("benchmark-search-speed").textContent).toBe("45.0次/秒");
  expect(screen.getByTestId("benchmark-nn-speed").textContent).toBe("44.3次/秒");
  expect(screen.getByTestId("benchmark-facts").textContent).toContain("推荐线程数4");
  expect(screen.getByTestId("benchmark-facts").textContent).toContain("批量大小2");
  expect(screen.getByTestId("benchmark-start").textContent).toBe("重新优化");
  // KataGo recommended 4 threads although 6 measured faster; the recommendation is marked.
  const marked = within(screen.getByTestId("benchmark-tried")).getAllByRole("listitem").map((row) => row.getAttribute("data-chosen"));
  expect(marked).toEqual(["true", "false"]);
  fireEvent.click(screen.getByTestId("benchmark-reset"));
  expect(handlers.onReset).toHaveBeenCalledTimes(1);
});

test("a failure keeps its reason", () => {
  dialog({ ...IDLE, phase: "Failed", error: "KataGo exited with status 1" });
  expect(screen.getByTestId("benchmark-state").textContent).toBe("暂时无法完成 KataGo 速度优化");
  expect(screen.getByTestId("benchmark-error").textContent).toContain("KataGo exited with status 1");
});

test("the tray chip shows a run the window no longer does, reopens it and stops it", () => {
  const onOpen = vi.fn();
  const onStop = vi.fn();
  const running: BenchmarkStatus = { ...IDLE, phase: "Running", progress: { threads: 6, completed_positions: 1, total_positions: 4, visits_per_second: null, nn_evals_per_second: null } };
  const { rerender } = render(<BenchmarkChip status={running} labels={zh.benchmark} onOpen={onOpen} onStop={onStop} />);
  expect(screen.getByTestId("benchmark-chip").textContent).toContain("6 线程 · 1/4");
  fireEvent.click(screen.getByText("正在优化 KataGo"));
  expect(onOpen).toHaveBeenCalledTimes(1);
  fireEvent.click(screen.getByTestId("benchmark-chip-stop"));
  expect(onStop).toHaveBeenCalledTimes(1);
  rerender(<BenchmarkChip status={{ ...running, phase: "Completed" }} labels={zh.benchmark} onOpen={onOpen} onStop={onStop} />);
  expect(screen.queryByTestId("benchmark-chip")).toBeNull();
});
