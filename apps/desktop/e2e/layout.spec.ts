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

test("the window follows LizzieYzy Next: info and graph left, board centre, moves and candidates right", async ({ page }) => {
  const analysis = midGameAnalysis(7, 20);
  await start(page, [documentChanged(analysedGame(), 7), analysisUpdated(analysis)]);

  const left = page.locator(".side-left");
  const right = page.locator(".side-right");
  await expect(left.getByTestId("info-panel")).toBeVisible();
  await expect(left.getByTestId("winrate-graph")).toBeVisible();
  await expect(page.getByTestId("board")).toBeVisible();
  await expect(right.getByTestId("move-list")).toBeVisible();
  await expect(right.getByTestId("candidate-row")).toHaveCount(analysis.candidates.length);

  const [leftBox, boardBox, rightBox] = await Promise.all([left.boundingBox(), page.getByTestId("board").boundingBox(), right.boundingBox()]);
  expect(leftBox && boardBox && leftBox.x + leftBox.width <= boardBox.x).toBe(true);
  expect(boardBox && rightBox && boardBox.x + boardBox.width <= rightBox.x).toBe(true);
  await expect(page.locator(".move-list [aria-current='step']")).toHaveText(/20/);
});

test("menus run the same commands as the tool row", async ({ page }) => {
  await start(page, [documentChanged(analysedGame(), 7)], [
    { command: "pickOpenPath", result: null },
    { command: "step", result: 8 },
  ]);
  await page.getByTestId("menu-file").click();
  await page.getByTestId("menu-open").click();
  await expect(page.getByRole("menu")).toHaveCount(0);
  await page.getByTestId("menu-game").click();
  await page.getByTestId("menu-next").click();
  await expect.poll(() => calls(page)).toEqual([
    { command: "pickOpenPath", args: [] },
    { command: "step", args: [1] },
  ]);

  await page.getByTestId("menu-view").click();
  await page.getByTestId("menu-coordinates").click();
  await expect(page.getByTestId("toggle-coordinates")).toHaveAttribute("aria-pressed", "false");
});
