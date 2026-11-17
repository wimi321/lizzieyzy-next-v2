import { expect, test, type Page } from "@playwright/test";
import type { AnalysisView, ComparisonView, DisplaySettings, DocumentView, Settings, WireAppEvent } from "../src/api/backend";
import { analysisUpdated, defaultDisplaySettings, documentChanged, midGame, midGameAnalysis } from "./fixtures";
import type { MockCommand, MockScript } from "./mock-backend";

// Small enough that the whole main panel stays under the file budget.
test.use({ viewport: { width: 1000, height: 640 }, deviceScaleFactor: 1, locale: "zh-CN" });

type Display = Required<DisplaySettings>;
const saved = (change: Partial<Display> = {}): Display => ({ ...defaultDisplaySettings, ...change });
const B18 = { program: "/opt/katago", model: "/opt/weights/kata1-b18c384nbt.bin.gz", config: "", extra_args: [] };
const B28 = { program: "/opt/katago", model: "/opt/weights/kata1-b28c512nbt.bin.gz", config: "", extra_args: [] };
const SETTINGS: Settings = {
  version: 1,
  engine: B18,
  pondering: false,
  fox_history: [],
  move_rank: { mode: "Auto", winrate_thresholds: [1, 3, 6, 12, 24], score_thresholds: [0.5, 1.5, 3, 6, 12] },
  tsumego: { to_play: "Inherit", attacker: "Auto", ko_threat: "Both", wall_distance: 1 },
  engines: [
    { id: "e1", name: "B18 均衡", profile: B18 },
    { id: "e2", name: "B28 强力", profile: B28 },
  ],
  main_engine_id: "e1",
};
const MAIN = midGameAnalysis(3, 20);
/** The comparison engine reads the position a little differently. */
function second(source: AnalysisView["source"] = "Live"): AnalysisView {
  const base = midGameAnalysis(3, 20, 2);
  return {
    ...base,
    source,
    winrate: 0.481,
    candidates: base.candidates.map((candidate, index) => ({ ...candidate, winrate: candidate.winrate - 0.08, order: (index + 1) % base.candidates.length })),
  };
}
const changed = (view: ComparisonView): WireAppEvent => ({ type: "ComparisonChanged", data: view });
const updated = (analysis: AnalysisView): WireAppEvent => ({ type: "ComparisonAnalysisUpdated", data: analysis });

async function start(page: Page, display: Partial<Display>, commands: MockCommand[], document: DocumentView = midGame()) {
  const script: MockScript = {
    initialEvents: [documentChanged(document, 3), analysisUpdated(MAIN)],
    commands: [{ command: "settings", result: { ...SETTINGS, display: saved(display) } }, ...commands],
  };
  await page.addInitScript((initial) => {
    window.__mockInitialScript = initial;
  }, script);
  await page.goto("/");
  await expect(page.getByTestId("board").first()).toHaveAttribute("data-ready", "true");
}

const push = (page: Page, events: WireAppEvent[]) => page.evaluate((list) => window.__mockBackend?.pushEvents(list), events);
const calls = (page: Page) => page.evaluate(() => (window.__mockBackend?.recordedCalls() ?? []).map((call) => call.command));
const row = (page: Page, id: string) => page.locator(`.menu-row[data-menu-id="${id}"]`);

test("双引擎模式: the second engine menu picks the comparison engine; both boards show their own candidates", async ({ page }) => {
  await start(page, { layout_mode: "DoubleEngine" }, [
    {
      command: "selectComparisonEngine",
      result: { ...SETTINGS, comparison_engine_id: "e2" },
      events: [changed({ engine_id: "e2", status: { type: "Ready", data: { version: "1.16.0" } }, analysis: null }), updated(second())],
    },
  ]);
  const board = page.getByTestId("comparison-board");
  await expect(page.getByTestId("comparison-pick")).toHaveText("在「副引擎」菜单里选择对比引擎");
  await expect(page.getByTestId("comparison-sub-line")).toHaveAttribute("data-state", "None");

  await page.getByTestId("menubar").locator('[data-menu-id="engineMenu2"]').click();
  // The main engine cannot be the comparison engine (Java: 第一引擎不能与第二引擎相同).
  await expect(row(page, "comparisonEngine0")).toHaveAttribute("aria-disabled", "true");
  await row(page, "comparisonEngine1").click();
  await expect(board).toHaveAttribute("data-candidates", "6");
  await expect(page.getByTestId("comparison-pick")).toHaveCount(0);
  await expect(page.getByTestId("menubar").locator('[data-menu-id="engineMenu2"]')).toHaveText("[2]: B28 强力");
  await expect(page.getByTestId("comparison-main-line")).toContainText("主引擎：");
  await expect(page.getByTestId("comparison-main-line")).toContainText("黑胜率： 56.2%");
  await expect(page.getByTestId("comparison-sub-line")).toContainText("副引擎： B28 强力");
  await expect(page.getByTestId("comparison-sub-line")).toContainText("黑胜率： 48.1%");
  await expect(page.getByTestId("comparison-sub-line")).toHaveAttribute("data-state", "Ready");
  await expect(page.getByTestId("main-panel")).toHaveScreenshot("double-engine.png");
  expect((await calls(page)).filter((command) => command === "selectComparisonEngine")).toHaveLength(1);

  // A hovered candidate's line shows on that board only.
  const inner = board.getByTestId("board");
  const box = (await inner.boundingBox())!;
  const read = async (name: string) => Number(await inner.getAttribute(name));
  const cell = await read("data-cell");
  await page.mouse.move(box.x + (await read("data-origin-x")) + cell * 10, box.y + (await read("data-origin-y")) + cell * 6);
  await expect(inner).toHaveAttribute("data-preview", "10,6");
  await expect(page.locator('[data-region="board"] [data-testid="board"]')).toHaveAttribute("data-preview", "");
});

test("a comparison engine that cannot start says why under its board; the main engine carries on", async ({ page }) => {
  await start(page, { layout_mode: "DoubleEngine" }, [
    { command: "selectComparisonEngine", result: { ...SETTINGS, comparison_engine_id: "e2" }, events: [changed({ engine_id: "e2", status: { type: "Starting" }, analysis: null })] },
  ]);
  await page.getByTestId("menubar").locator('[data-menu-id="engineMenu2"]').click();
  await row(page, "comparisonEngine1").click();
  const line = page.getByTestId("comparison-sub-line");
  await expect(line).toHaveAttribute("data-state", "Starting");
  await expect(line).toContainText("启动中…");
  await push(page, [changed({ engine_id: "e2", status: { type: "Failed", data: { reason: "找不到模型文件", stderr_tail: [] } }, analysis: null })]);
  await expect(line).toHaveAttribute("data-state", "Failed");
  await expect(line).toContainText("没能启动：找不到模型文件");
  await expect(page.getByTestId("comparison-main-line")).toContainText("黑胜率： 56.2%");
});

test("a game that carries LZ2 offers 双引擎模式; entering shows the saved comparison analysis", async ({ page }) => {
  const game: DocumentView = { ...midGame(), file_path: "/games/double.sgf", file_name: "double.sgf", has_comparison_analysis: true };
  await start(page, {}, [{ command: "setDisplaySettings", result: saved({ layout_mode: "DoubleEngine" }), events: [updated(second("Saved"))] }, { command: "fitMainWindow", result: undefined }], game);
  const prompt = page.getByTestId("comparison-prompt");
  await expect(prompt).toContainText("此棋谱为双引擎分析棋谱，是否进入双引擎模式查看？");
  await prompt.getByTestId("comparison-enter").click();
  await expect(prompt).toHaveCount(0);
  const board = page.getByTestId("comparison-board");
  await expect(board).toHaveAttribute("data-source", "Saved");
  await expect(board).toHaveAttribute("data-candidates", "6");
  await expect(page.getByTestId("comparison-sub-line")).toContainText("副引擎： 未选择");
  await expect.poll(() => calls(page)).toEqual(["settings", "setDisplaySettings", "fitMainWindow"]);
});
