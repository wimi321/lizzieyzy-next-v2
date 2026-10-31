import { expect, test, type Page } from "@playwright/test";
import type { AppEvent } from "../src/api/backend";
import { analysedGame, analysisUpdated, documentChanged, midGameAnalysis } from "./fixtures";
import type { MockCommand } from "./mock-backend";

test.use({ viewport: { width: 1280, height: 800 } });

async function start(page: Page, events: AppEvent[], commands: MockCommand[] = []) {
  await page.goto("/");
  await page.evaluate(
    ([initialEvents, scripted]) => window.__mockBackend?.setScript({ initialEvents, commands: scripted }),
    [events, commands] as const,
  );
}

async function calls(page: Page) {
  return page.evaluate(() => window.__mockBackend?.recordedCalls() ?? []);
}

test("the window follows LizzieYzy Next: info and graph left, board centre, the game tree right", async ({ page }) => {
  const analysis = midGameAnalysis(7, 20);
  await start(page, [documentChanged(analysedGame(), 7), analysisUpdated(analysis)]);

  const left = page.locator(".side-left");
  const right = page.locator(".side-right");
  await expect(left.getByTestId("info-panel")).toBeVisible();
  await expect(left.getByTestId("winrate-graph")).toBeVisible();
  await expect(page.getByTestId("board")).toBeVisible();
  await expect(right.getByTestId("move-tree")).toBeVisible();
  // Candidates are drawn on the board only; the right column is the tree.
  await expect(page.getByTestId("candidate-row")).toHaveCount(0);

  const [leftBox, boardBox, rightBox] = await Promise.all([left.boundingBox(), page.getByTestId("board").boundingBox(), right.boundingBox()]);
  expect(leftBox && boardBox && leftBox.x + leftBox.width <= boardBox.x).toBe(true);
  expect(boardBox && rightBox && boardBox.x + boardBox.width <= rightBox.x).toBe(true);
  // The mock tree is the main line; the current node is ringed.
  await expect(page.locator("[data-testid='tree-node'][data-current='true']")).toHaveAttribute("data-id", "20");
});

test("a narrow window keeps every top-bar control clickable", async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 760 });
  await start(page, [documentChanged(analysedGame(), 7)], [
    { command: "step", result: 8 },
    { command: "setPondering", result: undefined },
  ]);
  await page.getByTestId("nav-next").click();
  await page.getByTestId("toggle-pondering").click();
  await page.getByTestId("toggle-ownership").click();
  await page.getByTestId("open-setup").click();
  await expect(page.getByTestId("engine-setup")).toBeVisible();
  await expect.poll(() => calls(page)).toEqual(expect.arrayContaining([
    { command: "step", args: [1] },
    { command: "setPondering", args: [false] },
  ]));
});
