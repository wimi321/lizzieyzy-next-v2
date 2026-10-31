import { expect, test, type Page } from "@playwright/test";
import type { ProblemMovesView, StrengthEstimateView, WireAppEvent, WholeGameProgress } from "../src/api/backend";
import { analysedGame, documentChanged, strengthView } from "./fixtures";
import type { MockCommand } from "./mock-backend";

test.use({ viewport: { width: 1280, height: 820 } });

const running = (completed: number): WholeGameProgress => ({ completed, total: 121, visits: 3000, status: { type: "Running" } });

async function start(page: Page, events: WireAppEvent[], commands: MockCommand[] = []) {
  await page.goto("/");
  await page.evaluate(
    ([initialEvents, scripted]) => window.__mockBackend?.setScript({ initialEvents, commands: scripted }),
    [events, commands] as const,
  );
}

async function calls(page: Page) {
  return page.evaluate(() => window.__mockBackend?.recordedCalls() ?? []);
}

test("auto analysis starts with the chosen preset, shows progress and pauses", async ({ page }) => {
  await start(page, [documentChanged(analysedGame(), 7)], [
    { command: "startWholeGameAnalysis", result: running(0), events: [{ type: "WholeGameProgress", data: running(40) }] },
    { command: "pauseWholeGameAnalysis", result: { ...running(40), status: { type: "Paused" } } },
  ]);
  await page.getByTestId("auto-analysis").click();
  await expect(page.getByTestId("auto-analysis-panel")).toContainText("121");
  await page.getByTestId("auto-analysis-deep").click();
  await page.getByTestId("auto-analysis-start").click();

  const chip = page.getByTestId("auto-analysis");
  await expect(chip).toContainText("40 / 121");
  await chip.click();
  await page.getByTestId("auto-analysis-pause").click();
  await expect(chip).toHaveAttribute("data-state", "Paused");
  await expect.poll(() => calls(page)).toEqual([
    { command: "startWholeGameAnalysis", args: [{ visits: 3000 }] },
    { command: "pauseWholeGameAnalysis", args: [] },
  ]);
});

function problemView(points: Array<{ node_id: number; move_number: number }>): ProblemMovesView {
  const loss = (rank: "Mistake" | "Blunder", winrate: number, score: number) =>
    ({ winrate_loss: winrate, score_loss: score, rank, is_best: false, candidate_rank: 4, visits: 1000, before_visits: 1000 });
  const losses = new Map([[37, loss("Blunder", 24.6, 6.3)], [52, loss("Mistake", 8.1, 2.2)]]);
  return {
    revision: 7,
    settings: { mode: "Auto", winrate_thresholds: [1, 3, 6, 12, 24], score_thresholds: [0.5, 1.5, 3, 6, 12] },
    query: { minimum_rank: "Inaccuracy", color: null, sort: "LossDesc" },
    entries: [...losses].map(([move, value]) => ({
      node_id: move,
      move_number: move,
      color: move % 2 === 1 ? "Black" : "White",
      coordinate: move === 37 ? "Q16" : "D4",
      loss: value,
    })),
    error_bar: points.map((point) => ({ node_id: point.node_id, move_number: point.move_number, mv: null, loss: losses.get(point.move_number) ?? null })),
    current_index: 20,
  };
}

test("problem moves list the worst moves, mark the graph and jump on click", async ({ page }) => {
  const game = analysedGame();
  await page.goto("/");
  await page.evaluate(
    ([events, problems]) => {
      window.__mockBackend?.setProblemMoves(problems);
      window.__mockBackend?.setScript({ initialEvents: events, commands: [{ command: "goTo", result: 8 }] });
    },
    [[documentChanged(game, 7)], problemView(game.graph.points)] as [WireAppEvent[], ProblemMovesView],
  );
  await expect(page.getByTestId("problem-row")).toHaveCount(2);
  await expect(page.getByTestId("tab-problems")).toContainText("2");
  await expect(page.getByTestId("graph-mistake")).toHaveCount(2);
  await page.getByTestId("problem-row").first().click();
  await expect.poll(() => calls(page)).toEqual([{ command: "goTo", args: [37] }]);
});

test("the strength window starts the whole-game analysis and fills in as results arrive", async ({ page }) => {
  await start(page, [documentChanged(analysedGame(), 7)], [
    { command: "startWholeGameAnalysis", result: running(0), events: [{ type: "WholeGameProgress", data: running(12) }] },
  ]);
  await page.getByTestId("open-strength").click();
  const dialog = page.getByTestId("strength-dialog");
  await expect(dialog.getByTestId("strength-empty")).toBeVisible();
  await expect(page.getByTestId("player-match-black")).toHaveCount(0);

  await dialog.getByTestId("strength-analyze").click();
  await expect(dialog.getByTestId("strength-running")).toContainText("12 / 121");

  // The next finished position brings enough results for an estimate.
  await page.evaluate(
    ([view, next]) => {
      window.__mockBackend?.setStrengthEstimate(view);
      window.__mockBackend?.pushEvents([{ type: "WholeGameProgress", data: next }]);
    },
    [strengthView(7, 60), running(61)] as [StrengthEstimateView, WholeGameProgress],
  );
  await expect(dialog.getByTestId("strength-report")).toBeVisible();
  await expect(dialog.getByTestId("strength-black-match")).toHaveText("74%");
  await expect(dialog.getByTestId("strength-partial")).toBeVisible();
  await expect(page.getByTestId("player-match-black")).toHaveText("Match 74%");
  await expect.poll(() => calls(page)).toEqual([{ command: "startWholeGameAnalysis", args: [{ visits: 1000 }] }]);

  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
});

test("a finished estimate", async ({ page }) => {
  await page.goto("/");
  await page.evaluate(
    ([events, view]) => {
      window.__mockBackend?.setStrengthEstimate(view);
      window.__mockBackend?.setScript({ initialEvents: events });
    },
    [[documentChanged(analysedGame(), 7)], strengthView(7)] as [WireAppEvent[], StrengthEstimateView],
  );
  await page.getByTestId("open-strength").click();
  const dialog = page.getByTestId("strength-dialog");
  await expect(dialog.getByTestId("strength-black-rank")).toHaveText("5.3 dan");
  await expect(dialog.getByTestId("strength-white-rank")).toHaveText("3.8 dan");
  await expect(dialog).toHaveScreenshot("strength-report.png");
});

test("the phases page", async ({ page }) => {
  await page.goto("/");
  await page.evaluate(
    ([events, view]) => {
      window.__mockBackend?.setStrengthEstimate(view);
      window.__mockBackend?.setScript({ initialEvents: events });
    },
    [[documentChanged(analysedGame(), 7)], strengthView(7)] as [WireAppEvent[], StrengthEstimateView],
  );
  await page.getByTestId("open-strength").click();
  const dialog = page.getByTestId("strength-dialog");
  await dialog.getByTestId("strength-tab-phases").click();
  await expect(dialog.getByTestId("phase-0-black-rank")).toHaveText("6.4 dan");
  await expect(dialog.getByTestId("phase-review")).toContainText("in the middle game");
  await expect(dialog).toHaveScreenshot("strength-phases.png");
});
