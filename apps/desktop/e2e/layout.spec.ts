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

async function box(page: Page, region: string) {
  const rect = await page.locator(`[data-region="${region}"]`).boundingBox();
  if (!rect) throw new Error(`no ${region} region`);
  return rect;
}

test("the window is cut as LizzieYzy Next cuts it: info and graph left, board square in the middle, tree and list over the sub-board", async ({ page }) => {
  const analysis = midGameAnalysis(7, 20);
  await start(page, [documentChanged(analysedGame(), 7), analysisUpdated(analysis)]);

  await expect(page.locator('[data-region="captured"]').getByTestId("to-play")).toHaveText("Black to play");
  await expect(page.locator('[data-region="sub-board"]').getByTestId("sub-board")).toBeVisible();
  await expect(page.locator('[data-region="graph"]').getByTestId("winrate-graph")).toBeVisible();
  await expect(page.locator('[data-region="board"]').getByTestId("board")).toBeVisible();
  await expect(page.locator('[data-region="tree"]').getByTestId("move-tree")).toBeVisible();
  // 选点列表面板, shown by default as in Java, lists the engine's candidates.
  await expect(page.locator('[data-region="list"]').getByTestId("candidate-row")).toHaveCount(analysis.candidates.length);

  const panel = await page.getByTestId("main-panel").boundingBox();
  const [captured, graph, comment, board, tree, list, sub] = await Promise.all([
    box(page, "captured"),
    box(page, "graph"),
    box(page, "comment"),
    box(page, "board"),
    box(page, "tree"),
    box(page, "list"),
    box(page, "sub-board"),
  ]);
  expect(panel).not.toBeNull();
  // The board is the panel's height, square, with the left column ending a margin before it.
  expect(board.height).toBe(panel!.height);
  expect(board.width).toBe(board.height);
  expect(captured.x + captured.width).toBeLessThan(board.x);
  expect(graph.y).toBeGreaterThan(captured.y);
  expect(comment.y).toBe(graph.y + graph.height);
  // Right column: the tree over the list, halves of the room above a square sub-board that reaches the bottom.
  expect(tree.x).toBeGreaterThan(board.x + board.width);
  expect(list.y).toBe(tree.y + tree.height);
  expect(list.height).toBe(tree.height);
  // Halving an odd height leaves Java's one-pixel gap.
  expect(sub.y - (list.y + list.height)).toBeGreaterThanOrEqual(0);
  expect(sub.y - (list.y + list.height)).toBeLessThanOrEqual(1);
  expect(sub.width).toBe(sub.height);
  await expect(page.locator("[data-testid='tree-node'][data-current='true']")).toHaveAttribute("data-id", "20");
});

test("a narrow window keeps the working controls clickable and reaches the engine list from the menu", async ({ page }) => {
  await page.setViewportSize({ width: 1080, height: 760 });
  await start(page, [documentChanged(analysedGame(), 7)], [
    { command: "step", result: 8 },
    { command: "togglePondering", result: { pondering: false, pause_reason: "User" } },
  ]);
  await page.getByTestId("nav-next").click();
  await page.getByTestId("toggle-pondering").click();
  await page.getByTestId("toggle-ownership").click();
  await page.getByTestId("menubar").getByRole("menuitem", { name: "Settings" }).click();
  await page.locator('.menu-row[data-menu-id="engineConfig"]').click();
  await expect(page.getByTestId("engine-list")).toBeVisible();
  await expect.poll(() => calls(page)).toEqual(expect.arrayContaining([
    { command: "step", args: [1] },
    { command: "togglePondering", args: [] },
  ]));
});
