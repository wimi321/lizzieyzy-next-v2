import { expect, test, type Page } from "@playwright/test";
import type { AppEvent, ProblemMovesView } from "../src/api/backend";
import { analysisUpdated, defaultDisplaySettings, documentChanged, midGame, recordedGame } from "./fixtures";
import type { MockCommand, MockScript } from "./mock-backend";

test.use({ viewport: { width: 1280, height: 820 }, deviceScaleFactor: 1, locale: "zh-CN" });

async function start(page: Page, events: AppEvent[], commands: MockCommand[], problems: ProblemMovesView | null = null) {
  await page.goto("/");
  await page.evaluate(
    ([initialEvents, scripted, problemMoves]) => {
      window.__mockBackend?.setProblemMoves(problemMoves);
      window.__mockBackend?.setScript({ initialEvents, commands: scripted });
    },
    [events, commands, problems] as const,
  );
  await expect(page.getByTestId("board")).toHaveAttribute("data-ready", "true");
}

async function calls(page: Page) {
  return page.evaluate(() => window.__mockBackend?.recordedCalls() ?? []);
}

const row = (page: Page, id: string) => page.locator(`.menu-row[data-menu-id="${id}"]`);

test("选点列表面板 lists the candidates under the tree; a row previews its line and a double click plays it", async ({ page }) => {
  const { view, tree, problems, analysis } = recordedGame(7);
  await start(
    page,
    [documentChanged(view, 7), analysisUpdated(analysis)],
    [
      { command: "variationTree", result: tree },
      { command: "play", result: 8 },
      { command: "setDisplaySettings", result: { ...defaultDisplaySettings, show_list_pane: false } },
    ],
    problems,
  );
  const list = page.getByTestId("candidate-list");
  await expect(list.getByTestId("candidate-row")).toHaveCount(6);
  await expect(list.getByTestId("candidate-row").first()).toContainText("L13");
  const treeBox = (await page.locator('[data-region="tree"]').boundingBox())!;
  const region = (await page.locator('[data-region="list"]').boundingBox())!;
  expect(Math.round(region.y)).toBe(Math.round(treeBox.y + treeBox.height));
  await expect(list).toHaveScreenshot("candidate-list.png");

  // The third row previews its line on the board, as hovering the candidate there does.
  const board = page.getByTestId("board");
  await expect(board).toHaveAttribute("data-preview", "");
  await list.getByTestId("candidate-row").nth(2).hover();
  await expect(board).toHaveAttribute("data-preview", "13,12");
  await list.getByTestId("candidate-row").nth(2).dblclick();
  await expect
    .poll(async () => (await calls(page)).filter((call) => call.command === "play").map((call) => call.args[0]))
    .toEqual([{ type: "Play", data: { color: "Black", point: { x: 13, y: 12 } } }]);
  await page.mouse.move(5, 5);
  await expect(board).toHaveAttribute("data-preview", "");

  // 显示 → 面板 → 选点列表面板 hides it, and the tree takes its room.
  await page.getByTestId("menubar").locator('[data-menu-id="viewMenu"]').click();
  await row(page, "panel").hover();
  await expect(row(page, "listPane")).toHaveAttribute("aria-checked", "true");
  await row(page, "listPane").click();
  await expect(page.locator('[data-region="list"]')).toHaveCount(0);
  expect((await calls(page)).find((call) => call.command === "setDisplaySettings")?.args).toEqual([{ ...defaultDisplaySettings, show_list_pane: false }]);
  await expect.poll(async () => (await page.locator('[data-region="tree"]').boundingBox())!.height).toBeGreaterThan(treeBox.height);
});

test("超级鹰眼 opens the 问题手 page, from the toolbar or Y", async ({ page }) => {
  const { view, tree, problems, analysis } = recordedGame(7);
  await start(page, [documentChanged(view, 7), analysisUpdated(analysis)], [{ command: "variationTree", result: tree }], problems);
  await expect(page.getByTestId("tab-comment")).toHaveAttribute("aria-selected", "true");
  await page.getByTestId("tool-hawkeye").click();
  await expect(page.getByTestId("tab-problems")).toHaveAttribute("aria-selected", "true");
  await expect(page.getByTestId("tool-hawkeye")).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator('[data-region="comment"]')).toHaveScreenshot("hawkeye.png");
  // Y closes it again, back to the comment.
  await page.locator('[data-region="statistics"]').click();
  await page.keyboard.press("y");
  await expect(page.getByTestId("tab-comment")).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("y");
  await expect(page.getByTestId("tab-problems")).toHaveAttribute("aria-selected", "true");
});

test("综合设置 → 启用缓存 is saved; 分析 clears this move's analysis at once and the whole cache after asking", async ({ page }) => {
  const limits = { limit_time: false, max_analyze_time_seconds: 600, limit_playout: false, limit_playouts: 100000 };
  const script: MockScript = {
    initialEvents: [documentChanged(midGame(), 3)],
    commands: [
      {
        command: "settings",
        result: { version: 1, engine: { program: "/opt/katago", model: "/opt/model.bin.gz", config: "", extra_args: [] }, pondering: true, cache_enabled: true },
      },
      { command: "analysisLimits", result: limits },
      { command: "setAnalysisLimits", result: limits },
      { command: "setCacheEnabled", result: false },
      { command: "clearAnalysis", result: 4 },
      { command: "clearCache", result: undefined },
    ],
  };
  await page.addInitScript((initial) => {
    window.__mockInitialScript = initial;
  }, script);
  await page.goto("/");
  await page.getByTestId("menubar").locator('[data-menu-id="settings"]').click();
  await row(page, "comprehensiveSettings").click();
  await expect(page.getByTestId("settings-cache")).toBeChecked();
  await page.getByTestId("settings-cache").uncheck();
  await page.getByTestId("settings-save").click();
  await expect(page.getByTestId("settings-dialog")).toBeHidden();

  await page.getByTestId("menubar").locator('[data-menu-id="analyzeMenu"]').click();
  await row(page, "clearThisLizzieBestmoves").click();
  await page.getByTestId("menubar").locator('[data-menu-id="analyzeMenu"]').click();
  await row(page, "clearAllLizzieCache").click();
  await expect(page.getByTestId("confirm-clear-cache")).toContainText("清除本机保存的所有分析缓存");
  await page.getByTestId("confirm-clear-cache-ok").click();
  await expect
    .poll(async () => (await calls(page)).map((call) => [call.command, call.args]))
    .toEqual([
      ["settings", []],
      ["analysisLimits", []],
      ["setAnalysisLimits", [limits]],
      ["setCacheEnabled", [false]],
      ["clearAnalysis", [false]],
      ["clearCache", [true]],
    ]);
});
