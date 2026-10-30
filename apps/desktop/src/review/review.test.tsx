import { act, cleanup, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { WholeGameProgress, WholeGameProgressListener } from "@/api/backend";
import zh from "../locales/zh-CN.json";
import { AutoAnalysis } from "./AutoAnalysis";
import { useWholeGame } from "./useWholeGame";

afterEach(cleanup);

function progress(completed: number, total: number, type: "Running" | "Paused" | "Completed" | "Cancelled"): WholeGameProgress {
  return { completed, total, visits: 1000, status: { type } };
}

function panel(props: Partial<Parameters<typeof AutoAnalysis>[0]> = {}) {
  const handlers = { onStart: vi.fn(), onPause: vi.fn(), onResume: vi.fn(), onCancel: vi.fn() };
  render(<AutoAnalysis progress={null} positions={121} labels={zh.review} {...handlers} {...props} />);
  return handlers;
}

test("at rest it offers the presets and starts with the chosen depth", () => {
  const { onStart } = panel();
  fireEvent.click(screen.getByTestId("auto-analysis"));
  expect(screen.getByText("逐手分析当前这条线，共 121 个局面。分析时可以照常翻看。")).toBeTruthy();
  expect(screen.getByTestId("auto-analysis-standard").getAttribute("aria-checked")).toBe("true");

  fireEvent.click(screen.getByTestId("auto-analysis-deep"));
  fireEvent.click(screen.getByTestId("auto-analysis-start"));
  expect(onStart).toHaveBeenCalledWith(3000);
  expect(screen.queryByTestId("auto-analysis-panel")).toBeNull();
});

test("while running it shows progress and can pause or cancel", () => {
  const { onPause, onCancel } = panel({ progress: progress(12, 120, "Running") });
  const chip = screen.getByTestId("auto-analysis");
  expect(chip.textContent).toContain("12 / 120");
  expect((chip.querySelector(".progress-fill") as HTMLElement).style.width).toBe("10%");

  fireEvent.click(chip);
  fireEvent.click(screen.getByTestId("auto-analysis-pause"));
  expect(onPause).toHaveBeenCalled();
  fireEvent.click(screen.getByTestId("auto-analysis-cancel"));
  expect(onCancel).toHaveBeenCalled();
});

test("a paused run resumes; Escape closes the panel", () => {
  const { onResume } = panel({ progress: progress(30, 120, "Paused") });
  fireEvent.click(screen.getByTestId("auto-analysis"));
  fireEvent.click(screen.getByTestId("auto-analysis-resume"));
  expect(onResume).toHaveBeenCalled();
  fireEvent.keyDown(window, { key: "Escape" });
  expect(screen.queryByTestId("auto-analysis-panel")).toBeNull();
});

test("finished and failed runs say so next to the start button", () => {
  panel({ progress: progress(121, 121, "Completed") });
  fireEvent.click(screen.getByTestId("auto-analysis"));
  expect(screen.getByText("已完成 121 个局面")).toBeTruthy();
  cleanup();

  panel({ progress: { completed: 3, total: 121, visits: 1000, status: { type: "Error", data: { reason: "engine stopped" } } } });
  fireEvent.click(screen.getByTestId("auto-analysis"));
  expect(screen.getByText("出错：engine stopped")).toBeTruthy();
});

function fakeBackend(initial: WholeGameProgress | null) {
  let listener: WholeGameProgressListener | null = null;
  const order: string[] = [];
  const backend = {
    subscribeWholeGameProgress: vi.fn(async (next: WholeGameProgressListener) => {
      order.push("subscribe");
      listener = next;
      return 7;
    }),
    unsubscribe: vi.fn(async () => {}),
    wholeGameProgress: vi.fn(async () => {
      order.push("read");
      return initial;
    }),
    startWholeGameAnalysis: vi.fn(async () => progress(0, 50, "Running")),
    pauseWholeGameAnalysis: vi.fn(async () => progress(5, 50, "Paused")),
    resumeWholeGameAnalysis: vi.fn(async () => progress(5, 50, "Running")),
    cancelWholeGameAnalysis: vi.fn(async () => progress(5, 50, "Cancelled")),
  };
  return { backend, order, emit: (next: WholeGameProgress) => listener?.(next) };
}

test("the hook subscribes before reading, then follows events and command replies", async () => {
  const { backend, order, emit } = fakeBackend(progress(2, 50, "Paused"));
  const { result, unmount } = renderHook(() => useWholeGame(backend));
  await act(async () => {});
  expect(order).toEqual(["subscribe", "read"]);
  expect(result.current.progress).toEqual(progress(2, 50, "Paused"));

  act(() => emit(progress(9, 50, "Running")));
  expect(result.current.progress?.completed).toBe(9);

  await act(async () => result.current.start(3000));
  expect(backend.startWholeGameAnalysis).toHaveBeenCalledWith({ visits: 3000 });
  expect(result.current.progress).toEqual(progress(0, 50, "Running"));

  // An event that overtakes a command's reply is newer and wins.
  backend.pauseWholeGameAnalysis.mockImplementation(async () => {
    emit(progress(20, 50, "Running"));
    return progress(5, 50, "Paused");
  });
  await act(async () => result.current.pause());
  expect(result.current.progress).toEqual(progress(20, 50, "Running"));

  unmount();
  expect(backend.unsubscribe).toHaveBeenCalledWith(7);
});

test("a custom depth replaces the presets and must be in the backend's range", () => {
  const { onStart } = panel();
  fireEvent.click(screen.getByTestId("auto-analysis"));
  const input = screen.getByTestId("auto-analysis-custom");
  fireEvent.change(input, { target: { value: "300" } });
  expect(screen.getByTestId("auto-analysis-standard").getAttribute("aria-checked")).toBe("false");
  expect(screen.getByTestId("auto-analysis-custom-hint").textContent).toBe("请输入 500 到 1,000,000 之间的整数");
  expect((screen.getByTestId("auto-analysis-start") as HTMLButtonElement).disabled).toBe(true);

  fireEvent.change(input, { target: { value: " 2500 " } });
  expect(screen.queryByTestId("auto-analysis-custom-hint")).toBeNull();
  fireEvent.click(screen.getByTestId("auto-analysis-start"));
  expect(onStart).toHaveBeenCalledWith(2500);
});

test("choosing a preset again leaves the custom depth", () => {
  const { onStart } = panel();
  fireEvent.click(screen.getByTestId("auto-analysis"));
  fireEvent.change(screen.getByTestId("auto-analysis-custom"), { target: { value: "abc" } });
  fireEvent.click(screen.getByTestId("auto-analysis-quick"));
  expect(screen.queryByTestId("auto-analysis-custom-hint")).toBeNull();
  fireEvent.click(screen.getByTestId("auto-analysis-start"));
  expect(onStart).toHaveBeenCalledWith(500);
});
