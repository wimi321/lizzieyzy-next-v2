import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";
import type { StrengthPhasesReport } from "@/api/backend";
import { strengthPhases, strengthView } from "../../e2e/fixtures";
import en from "../locales/en.json";
import zh from "../locales/zh-CN.json";
import { StrengthDialog } from "./StrengthDialog";
import { reviewLine, verdict } from "./phases";

afterEach(cleanup);

const NAMES = { black: "听涛观弈", white: "夜半听雨" };
const PHASE_NAMES = ["布局", "中盘", "官子"];

function phasesPage(bounds: [number, number] | null = null, chinese = true) {
  const onBounds = vi.fn();
  render(
    <StrengthDialog
      view={strengthView(3)}
      progress={null}
      black={{ name: NAMES.black, rank: null }}
      white={{ name: NAMES.white, rank: null }}
      chinese={chinese}
      labels={chinese ? zh.strength : en.strength}
      tierNames={[]}
      bounds={bounds}
      onBounds={onBounds}
      onAnalyze={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  fireEvent.click(screen.getByTestId("strength-tab-phases"));
  return onBounds;
}

describe("the phases page, as LizzieYzy Next's 分阶段", () => {
  test("each phase shows both sides' level, the best and weakest phase, and who played it better", () => {
    phasesPage();
    const opening = screen.getByTestId("phase-card-0");
    expect(within(opening).getByText("第 1–60 手")).toBeTruthy();
    expect(screen.getByTestId("phase-0-black-rank").textContent).toBe("业余6.4段");
    expect(screen.getByTestId("phase-0-white-rank").textContent).toBe("业余4.2段");
    expect(within(screen.getByTestId("phase-0-black")).getByText("最好")).toBeTruthy();
    expect(within(screen.getByTestId("phase-1-black")).getByText("最弱")).toBeTruthy();
    expect(within(screen.getByTestId("phase-1-white")).getByText("最好")).toBeTruthy();
    expect(within(screen.getByTestId("phase-2-white")).getByText("最弱")).toBeTruthy();
    expect(screen.getByTestId("phase-verdict-0").textContent).toBe("听涛观弈明显更好");
    expect(screen.getByTestId("phase-verdict-1").textContent).toBe("夜半听雨明显更好");
    expect(screen.getByTestId("phase-review").textContent).toBe("复盘时可以重点看看：听涛观弈的中盘和夜半听雨的官子。");
    expect(screen.queryByTestId("phase-reset")).toBeNull();
  });

  test("dividers move by keys within Java's limits, and reset goes back to the defaults", () => {
    const onBounds = phasesPage();
    const first = screen.getByTestId("phase-divider-0");
    const second = screen.getByTestId("phase-divider-1");
    expect(first.getAttribute("aria-valuenow")).toBe("60");
    expect(first.getAttribute("aria-label")).toBe("布局与中盘的分界：第 60 手");
    fireEvent.keyDown(first, { key: "ArrowRight" });
    expect(onBounds).toHaveBeenLastCalledWith([61, 150]);
    fireEvent.keyDown(first, { key: "ArrowLeft", shiftKey: true });
    expect(onBounds).toHaveBeenLastCalledWith([50, 150]);
    fireEvent.keyDown(first, { key: "End" });
    expect(onBounds).toHaveBeenLastCalledWith([140, 150]);
    // The game has 211 moves; the endgame keeps at least ten of them.
    fireEvent.keyDown(second, { key: "End" });
    expect(onBounds).toHaveBeenLastCalledWith([60, 201]);
    fireEvent.keyDown(second, { key: "Home" });
    expect(onBounds).toHaveBeenLastCalledWith([60, 70]);
    cleanup();

    const again = phasesPage([80, 170]);
    expect(screen.getByTestId("phase-divider-1").getAttribute("aria-valuenow")).toBe("170");
    fireEvent.click(screen.getByTestId("phase-reset"));
    expect(again).toHaveBeenLastCalledWith(null);
  });

  test("English puts the phase names in the sentence", () => {
    phasesPage(null, false);
    expect(screen.getByTestId("phase-review").textContent).toBe(
      "Worth a closer look in review: 听涛观弈 in the middle game and 夜半听雨 in the endgame.",
    );
  });
});

function withRanks(black: [number | null, number | null, number | null], white: [number | null, number | null, number | null]) {
  const report = strengthPhases();
  report.phases.forEach((phase, index) => {
    phase.black = { ...phase.black, rank_value: black[index] ?? null };
    phase.white = { ...phase.white, rank_value: white[index] ?? null };
    phase.black_ranked = black[index] !== null;
    phase.white_ranked = white[index] !== null;
  });
  return report;
}

describe("Java's verdict and review sentences", () => {
  test("verdicts", () => {
    const report = withRanks([5, 5.5, null], [4.8, 5, 3]);
    expect(verdict(report.phases[0], NAMES, zh.strength.phase)).toBe("双方水平相当");
    expect(verdict(report.phases[1], NAMES, zh.strength.phase)).toBe("听涛观弈略好一些");
    expect(verdict(report.phases[2], NAMES, zh.strength.phase)).toBe("手数太少");
    expect(verdict({ ...report.phases[2], played: false }, NAMES, zh.strength.phase)).toBe("这盘棋没有下到这个阶段");
  });

  test("review lines", () => {
    const steady: StrengthPhasesReport = {
      ...withRanks([5, 5.1, 5], [4, 4.1, 4]),
      black: { best: null, worst: null, worth_reviewing: false },
      white: { best: null, worst: null, worth_reviewing: false },
    };
    expect(reviewLine(steady, NAMES, PHASE_NAMES, zh.strength.phase)).toBe("双方各阶段的发挥都比较平稳。");
    expect(reviewLine({ ...steady, phases: withRanks([5, null, null], [4, null, null]).phases }, NAMES, PHASE_NAMES, zh.strength.phase)).toBeNull();
    const same = { ...steady, black: { best: 0, worst: 2, worth_reviewing: true }, white: { best: 1, worst: 2, worth_reviewing: true } };
    expect(reviewLine(same, NAMES, PHASE_NAMES, zh.strength.phase)).toBe("复盘时可以重点看看：双方的官子。");
    const one = { ...steady, white: { best: 1, worst: 0, worth_reviewing: true } };
    expect(reviewLine(one, NAMES, PHASE_NAMES, zh.strength.phase)).toBe("复盘时可以重点看看：夜半听雨的布局。");
  });
});
