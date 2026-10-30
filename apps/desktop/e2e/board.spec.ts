import { expect, test, type Page } from "@playwright/test";
import type { AppEvent, PointView } from "../src/api/backend";
import {
  analysisUpdated,
  documentChanged,
  emptyNineteen,
  midGame,
  midGameAnalysis,
  nineByNine,
} from "./fixtures";
import type { MockCommand } from "./mock-backend";

const JUMP = 1_000_000;

test.use({ viewport: { width: 900, height: 860 }, deviceScaleFactor: 1 });

async function start(page: Page, events: AppEvent[], commands: MockCommand[] = []) {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/");
  await page.evaluate(
    ([initialEvents, scripted]) => window.__mockBackend?.setScript({ initialEvents, commands: scripted }),
    [events, commands] as const,
  );
  const board = page.getByTestId("board");
  await expect(board).toBeVisible();
  await expect(board).not.toHaveAttribute("data-cell", "0.000");
  return { board, errors };
}

async function pixelOf(page: Page, point: PointView) {
  const board = page.getByTestId("board");
  const read = async (name: string) => Number(await board.getAttribute(name));
  const [cell, originX, originY] = await Promise.all([read("data-cell"), read("data-origin-x"), read("data-origin-y")]);
  return { x: originX + point.x * cell, y: originY + point.y * cell };
}

async function calls(page: Page) {
  return page.evaluate(() => window.__mockBackend?.recordedCalls() ?? []);
}

test.describe("screenshots", () => {
  test("empty 19x19 board", async ({ page }) => {
    const { board, errors } = await start(page, [documentChanged(emptyNineteen(), 1)]);
    await expect(board).toHaveScreenshot("empty-19.png");
    expect(errors).toEqual([]);
  });

  test("middle game with candidates", async ({ page }) => {
    const { board } = await start(page, [documentChanged(midGame(), 7), analysisUpdated(midGameAnalysis(7, 20))]);
    await expect(page.getByTestId("analysis-move")).toHaveAttribute("data-revision", "7");
    await expect(board).toHaveScreenshot("mid-game.png");
  });

  test("territory overlay", async ({ page }) => {
    const { board } = await start(page, [documentChanged(midGame(), 7), analysisUpdated(midGameAnalysis(7, 20, 1, true))]);
    await page.getByTestId("toggle-ownership").click();
    await expect(page.getByTestId("toggle-ownership")).toHaveAttribute("aria-pressed", "true");
    await expect(board).toHaveScreenshot("ownership.png");
  });

  test("hovering a candidate previews its variation", async ({ page }) => {
    const { board } = await start(page, [documentChanged(midGame(), 7), analysisUpdated(midGameAnalysis(7, 20))]);
    await board.hover({ position: await pixelOf(page, { x: 10, y: 6 }) });
    await expect(board).toHaveScreenshot("variation.png");
  });

  test("9x9 board", async ({ page }) => {
    const { board } = await start(page, [documentChanged(nineByNine(), 3)]);
    await expect(board).toHaveScreenshot("nine.png");
  });
});

test("clicking an empty point plays there; occupied points are ignored", async ({ page }) => {
  const { board } = await start(
    page,
    [documentChanged(midGame(), 7)],
    [{ command: "play", result: 8, events: [] }],
  );
  await board.click({ position: await pixelOf(page, { x: 3, y: 3 }) });
  await board.click({ position: await pixelOf(page, { x: 16, y: 16 }) });
  await expect.poll(() => calls(page)).toEqual([
    { command: "play", args: [{ type: "Play", data: { color: "Black", point: { x: 16, y: 16 } } }] },
  ]);
});

test("arrow keys, Home and End step through the game", async ({ page }) => {
  await start(
    page,
    [documentChanged(midGame(), 7)],
    [
      { command: "step", result: 8, events: [] },
      { command: "step", result: 9, events: [] },
      { command: "step", result: 10, events: [] },
      { command: "step", result: 11, events: [] },
    ],
  );
  for (const key of ["ArrowRight", "ArrowLeft", "Home", "End"]) await page.keyboard.press(key);
  await expect.poll(() => calls(page)).toEqual([
    { command: "step", args: [1] },
    { command: "step", args: [-1] },
    { command: "step", args: [-JUMP] },
    { command: "step", args: [JUMP] },
  ]);
});

test("analysis of another revision is never shown", async ({ page }) => {
  await start(page, [documentChanged(midGame(), 7), analysisUpdated(midGameAnalysis(6, 20))]);
  const label = page.getByTestId("analysis-move");
  await expect(label).not.toHaveAttribute("data-revision");
  await page.evaluate((event) => window.__mockBackend?.pushEvents([event]), analysisUpdated(midGameAnalysis(7, 20)));
  await expect(label).toHaveAttribute("data-revision", "7");
  await page.evaluate((event) => window.__mockBackend?.pushEvents([event]), documentChanged(midGame(), 8));
  await expect(label).not.toHaveAttribute("data-revision");
});

test("200 analysis frames at 20 Hz cause no long tasks", async ({ page }) => {
  test.setTimeout(30_000);
  await start(page, [documentChanged(midGame(), 7)]);
  await page.getByTestId("toggle-ownership").click();
  const frames = Array.from({ length: 200 }, (_, i) => analysisUpdated(midGameAnalysis(7, 20, 1 + i / 20, true)));
  const longTasks = await page.evaluate(async (events) => {
    const durations: number[] = [];
    // Only streaming counts: page start-up work can be reported late, so filter by start time.
    const streamingStarted = performance.now();
    const observer = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        if (entry.startTime >= streamingStarted) durations.push(entry.duration);
      }
    });
    observer.observe({ type: "longtask" });
    for (const event of events) {
      window.__mockBackend?.pushEvents([event]);
      // Pacing reproduces the coordinator's 20 Hz cap; it is not synchronisation.
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    observer.disconnect();
    return durations.filter((duration) => duration > 50);
  }, frames);
  expect(longTasks).toEqual([]);
});
