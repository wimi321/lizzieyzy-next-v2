import { expect, test, type Page } from "@playwright/test";
import type { AppEvent } from "../src/api/backend";
import { analysedGame, analysisUpdated, documentChanged, emptyNineteen, graphView, midGame, midGameAnalysis } from "./fixtures";
import type { MockCommand } from "./mock-backend";

const JUMP = 1_000_000;

test.use({ viewport: { width: 1000, height: 900 }, deviceScaleFactor: 1 });

async function start(page: Page, events: AppEvent[], commands: MockCommand[] = []) {
  await page.goto("/");
  await page.evaluate(
    ([initialEvents, scripted]) => window.__mockBackend?.setScript({ initialEvents, commands: scripted }),
    [events, commands] as const,
  );
  const graph = page.getByTestId("winrate-graph");
  await expect(graph.locator("svg")).toBeVisible();
  return graph;
}

/** CSS x of a point inside the graph, from the layout the component reports. */
async function xOf(page: Page, index: number) {
  const graph = page.getByTestId("winrate-graph");
  const read = async (name: string) => Number(await graph.getAttribute(name));
  const [count, left, right] = await Promise.all([read("data-count"), read("data-plot-left"), read("data-plot-right")]);
  const box = await graph.boundingBox();
  if (!box) throw new Error("graph has no layout");
  const width = box.width - left - right;
  return box.x + left + (count <= 1 ? 0 : (index / (count - 1)) * width);
}

async function calls(page: Page) {
  return page.evaluate(() => window.__mockBackend?.recordedCalls() ?? []);
}

test.describe("screenshots", () => {
  test("graph with analysis", async ({ page }) => {
    const graph = await start(page, [documentChanged(analysedGame(), 7), analysisUpdated(midGameAnalysis(7, 20))]);
    await expect(page.getByTestId("graph-current")).toContainText("20");
    await expect(graph).toHaveScreenshot("graph-analysed.png");
  });

  test("graph before any analysis", async ({ page }) => {
    const graph = await start(page, [documentChanged(emptyNineteen(), 1)]);
    await expect(page.getByTestId("graph-empty")).toBeVisible();
    await expect(graph).toHaveScreenshot("graph-empty.png");
  });
});

test("an analysed move with no analysed neighbour is drawn as a dot", async ({ page }) => {
  await start(page, [documentChanged(midGame(graphView(121, 20, { 5: [0.62, 4], 40: [0.4, -3], 41: [0.42, -2] })), 7)]);
  const dots = page.getByTestId("graph-dot");
  await expect(dots).toHaveCount(1);
  await expect(dots.first()).toHaveAttribute("data-index", "5");
});

test("clicking the graph jumps to that move", async ({ page }) => {
  const graph = await start(page, [documentChanged(analysedGame(), 7)], [{ command: "goTo", result: 8 }]);
  const box = await graph.boundingBox();
  await page.mouse.click(await xOf(page, 60), (box?.y ?? 0) + 60);
  await expect.poll(() => calls(page)).toEqual([{ command: "goTo", args: [60] }]);
});

test("dragging along the graph jumps through every move it passes", async ({ page }) => {
  const graph = await start(
    page,
    [documentChanged(analysedGame(), 7)],
    [30, 31, 32, 33].map((): MockCommand => ({ command: "goTo", result: 8 })),
  );
  const y = ((await graph.boundingBox())?.y ?? 0) + 60;
  await page.mouse.move(await xOf(page, 30), y);
  await page.mouse.down();
  await page.mouse.move(await xOf(page, 33), y, { steps: 24 });
  await page.mouse.up();
  await expect.poll(() => calls(page)).toEqual([30, 31, 32, 33].map((nodeId) => ({ command: "goTo", args: [nodeId] })));
});

test("the engine light shows each state and explains failures", async ({ page }) => {
  await start(page, [documentChanged(analysedGame(), 7)]);
  const light = page.getByTestId("engine-status");
  const push = (event: AppEvent) => page.evaluate((e) => window.__mockBackend?.pushEvents([e]), event);
  await push({ type: "EngineStatusChanged", data: { type: "Starting" } });
  await expect(light).toHaveAttribute("data-state", "Starting");
  await push({ type: "EngineStatusChanged", data: { type: "Ready", data: { version: "KataGo 1.16.5" } } });
  await expect(light).toHaveAttribute("data-state", "Ready");
  await expect(light).toHaveAttribute("title", "KataGo 1.16.5");
  await push({ type: "EngineStatusChanged", data: { type: "Restarting", data: { attempt: 1, reason: "engine stopped responding" } } });
  await expect(light).toHaveAttribute("data-state", "Restarting");
  await expect(light).toHaveAttribute("title", "engine stopped responding");
  await push({ type: "EngineStatusChanged", data: { type: "Failed", data: { reason: "model file not found", stderr_tail: [] } } });
  await expect(light).toHaveAttribute("data-state", "Failed");
  await expect(light).toHaveAttribute("title", "model file not found");
});

test("toolbar buttons navigate, pause, open and save", async ({ page }) => {
  await start(
    page,
    [documentChanged(analysedGame(), 7)],
    [
      { command: "step", result: 8 },
      { command: "step", result: 9 },
      { command: "step", result: 10 },
      { command: "step", result: 11 },
      { command: "setPondering", result: undefined },
      { command: "pickOpenPath", result: "/games/game.sgf" },
      { command: "openPath", result: 12 },
      { command: "pickSavePath", result: null },
    ],
  );
  for (const id of ["nav-first", "nav-previous", "nav-next", "nav-last"]) await page.getByTestId(id).click();
  await page.getByTestId("toggle-pondering").click();
  await expect(page.getByTestId("toggle-pondering")).toHaveAttribute("aria-pressed", "true");
  await page.getByTestId("open-file").click();
  await expect.poll(async () => (await calls(page)).length).toBe(7);
  await page.getByTestId("save-file").click();
  await expect.poll(() => calls(page)).toEqual([
    { command: "step", args: [-JUMP] },
    { command: "step", args: [-1] },
    { command: "step", args: [1] },
    { command: "step", args: [JUMP] },
    { command: "setPondering", args: [false] },
    { command: "pickOpenPath", args: [] },
    { command: "openPath", args: ["/games/game.sgf"] },
    { command: "pickSavePath", args: [] },
  ]);
});
