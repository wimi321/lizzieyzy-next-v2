import { act, cleanup, fireEvent, render, renderHook, screen, within } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { StrengthEstimateView, WholeGameProgress } from "@/api/backend";
import { emptyStrengthEstimate, strengthView } from "../../e2e/fixtures";
import en from "../locales/en.json";
import zh from "../locales/zh-CN.json";
import { StrengthDialog } from "./StrengthDialog";
import { TIER_ORDER, bandText, percent } from "./strength";
import { useStrength } from "./useStrength";

afterEach(cleanup);

const TIERS = TIER_ORDER.map((rank) => zh.problems.ranks[rank]);

function dialog(
  view: StrengthEstimateView | null,
  progress: WholeGameProgress | null = null,
  chinese = true,
  bounds: [number, number] | null = null,
) {
  const handlers = { onAnalyze: vi.fn(), onClose: vi.fn(), onBounds: vi.fn() };
  render(
    <StrengthDialog
      view={view}
      progress={progress}
      black={{ name: "听涛观弈", rank: "9段" }}
      white={{ name: "夜半听雨", rank: "8段" }}
      chinese={chinese}
      labels={chinese ? zh.strength : en.strength}
      tierNames={TIERS}
      bounds={bounds}
      {...handlers}
    />,
  );
  return handlers;
}

test("bands read like LizzieYzy Next's", () => {
  expect(bandText("3-4d", zh.strength.bands)).toBe("业余3-4段");
  expect(bandText("7d", zh.strength.bands)).toBe("业余7段");
  expect(bandText("11-15k", zh.strength.bands)).toBe("业余11-15级");
  expect(bandText("Beginner", zh.strength.bands)).toBe("入门");
  expect(bandText("10d职业", zh.strength.bands)).toBe("职业棋手");
  expect(bandText("11d一线职业", zh.strength.bands)).toBe("一线职业");
  expect(bandText("12d AI", zh.strength.bands)).toBe("AI 水平");
  expect(bandText("1-2k", en.strength.bands)).toBe("1-2 kyu");
  expect(bandText("-", zh.strength.bands)).toBeNull();
  expect(percent(0.736)).toBe("74%");
});

test("a full report shows both sides and the note", () => {
  dialog(strengthView(3));
  const black = screen.getByTestId("strength-black");
  expect(screen.getByTestId("strength-black-rank").textContent).toBe("业余5.3段");
  expect(within(black).getByText("±0.9")).toBeTruthy();
  expect(within(black).getByText("业余5-6段")).toBeTruthy();
  expect(within(black).getByText("高")).toBeTruthy();
  expect(within(black).getByText("听涛观弈")).toBeTruthy();
  expect(within(black).getByText("106 手")).toBeTruthy();
  expect(screen.getByTestId("strength-black-match").textContent).toBe("74%");
  expect(screen.getByTestId("strength-white-match").textContent).toBe("66%");
  expect(within(black).getByText("0.84 目")).toBeTruthy();
  const tiers = within(black).getAllByTestId("strength-black-tier").map((item) => item.textContent);
  expect(tiers).toEqual(["最佳51", "好棋27", "普通14", "疑问手7", "问题手5", "恶手2"]);

  expect(screen.getByTestId("strength-tab-assessment").getAttribute("aria-selected")).toBe("true");
  expect(screen.queryByTestId("strength-phases")).toBeNull();
  expect(screen.queryByTestId("strength-partial")).toBeNull();
  expect(screen.getByText(zh.strength.note)).toBeTruthy();
});

test("English uses Rust's English rank label", () => {
  dialog(strengthView(3), null, false);
  expect(screen.getByTestId("strength-black-rank").textContent).toBe("5.3 dan");
  expect(screen.getByText("5-6 dan")).toBeTruthy();
});

test("partial coverage offers the whole-game analysis", () => {
  const { onAnalyze } = dialog(strengthView(3, 120));
  expect(screen.getByTestId("strength-partial").textContent).toContain("只分析了 120 / 211 手");
  fireEvent.click(screen.getByTestId("strength-analyze"));
  expect(onAnalyze).toHaveBeenCalled();
});

test("while the whole game is analysed the window says how far it got", () => {
  dialog(strengthView(3, 120), { completed: 121, total: 212, visits: 1000, status: { type: "Running" } });
  expect(screen.getByTestId("strength-running").textContent).toBe("正在分析整盘棋：已分析 121 / 212，结果会随分析自动更新。");
  expect(screen.queryByTestId("strength-analyze")).toBeNull();
});

test("without analysis, with too few moves, or for a short game it explains instead", () => {
  const { onAnalyze } = dialog({ ...emptyStrengthEstimate(1), total_moves: 150 });
  expect(screen.getByTestId("strength-empty").textContent).toContain("这盘棋还没有分析");
  fireEvent.click(screen.getByTestId("strength-analyze"));
  expect(onAnalyze).toHaveBeenCalled();
  cleanup();

  dialog({ ...emptyStrengthEstimate(1), total_moves: 150, analyzed_moves: 4 });
  expect(screen.getByTestId("strength-few").textContent).toContain("分析过的手数还太少");
  cleanup();

  dialog({ ...emptyStrengthEstimate(1), total_moves: 5, analyzed_moves: 5 });
  expect(screen.getByTestId("strength-short").textContent).toContain("这盘棋太短，还估算不了");
  expect(screen.queryByTestId("strength-analyze")).toBeNull();
  cleanup();

  dialog(emptyStrengthEstimate(1));
  expect(screen.getByTestId("strength-short")).toBeTruthy();
  cleanup();

  dialog(null);
  expect(screen.getByTestId("strength-loading")).toBeTruthy();
});

test("Escape and the close button close the window", () => {
  const { onClose } = dialog(strengthView(3));
  fireEvent.keyDown(window, { key: "Escape" });
  fireEvent.click(screen.getByTestId("strength-close"));
  expect(onClose).toHaveBeenCalledTimes(2);
});

test("the hook re-reads on new results and keeps only the newest reply", async () => {
  const replies: Array<(view: StrengthEstimateView) => void> = [];
  const backend = {
    strengthEstimate: vi.fn(() => new Promise<StrengthEstimateView>((resolve) => replies.push(resolve))),
  };
  const { result, rerender } = renderHook((signals) => useStrength(backend, signals), {
    initialProps: { revision: 1, progress: 0, live: 0 },
  });
  rerender({ revision: 1, progress: 5, live: 0 });
  expect(backend.strengthEstimate).toHaveBeenCalledTimes(2);

  await act(async () => replies[1]?.(strengthView(1, 50)));
  await act(async () => replies[0]?.(emptyStrengthEstimate(1)));
  expect(result.current?.analyzed_moves).toBe(50);
});
