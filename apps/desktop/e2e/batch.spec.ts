import { expect, test, type Page } from "@playwright/test";
import type { BatchAnalysisStatus, Settings } from "../src/api/backend";
import { documentChanged, midGame } from "./fixtures";
import type { MockCommand, MockScript } from "./mock-backend";

test.use({ viewport: { width: 1280, height: 820 }, deviceScaleFactor: 1, locale: "zh-CN" });

// A saved engine, so the app does not try automatic setup at start.
const SETTINGS = { version: 1, engine: { program: "/opt/katago", model: "/opt/model.bin.gz", config: "", extra_args: [] }, pondering: false } as unknown as Settings;

async function start(page: Page, commands: MockCommand[], settings: Settings = SETTINGS) {
  const script: MockScript = {
    initialEvents: [documentChanged(midGame(), 3)],
    commands: [{ command: "settings", result: settings }, ...commands],
  };
  await page.addInitScript((initial) => {
    window.__mockInitialScript = initial;
  }, script);
  await page.goto("/");
  await expect(page.getByTestId("board")).toHaveAttribute("data-ready", "true");
}

async function calls(page: Page) {
  return (await page.evaluate(() => window.__mockBackend?.recordedCalls() ?? [])).filter((call) => call.command !== "settings");
}

const row = (page: Page, id: string) => page.locator(`.menu-row[data-menu-id="${id}"]`);
const menu = (page: Page, id: string) => page.getByTestId("menubar").locator(`[data-menu-id="${id}"]`);

const FILES = ["/Users/me/棋谱/2026-11-01 名人战 第一局.sgf", "/Users/me/棋谱/2026-11-02 名人战 第二局.sgf", "/Users/me/棋谱/野狐 1105.gib"];

const RUNNING: BatchAnalysisStatus = {
  phase: "Running",
  current_index: 1,
  reason: null,
  items: [
    { id: 0, path: FILES[0], state: { type: "Completed", data: { output: "/Users/me/棋谱/2026-11-01 名人战 第一局_已分析_20261113153012.sgf" } } },
    { id: 1, path: FILES[1], state: { type: "Running", data: { progress: { completed: 84, total: 212, visits: 1000, status: { type: "Running" } } } } },
    { id: 2, path: "/Users/me/棋谱/残局.sgf", state: { type: "Failed", data: { reason: "not an SGF or GIB game record" } } },
    { id: 3, path: FILES[2], state: { type: "Waiting" } },
  ],
};

test("分析 → 批量分析 picks games, sets depth, range and output, starts the batch and shows its progress", async ({ page }) => {
  await start(page, [
    { command: "pickOpenPaths", result: FILES },
    { command: "startBatchAnalysis", result: RUNNING, events: [{ type: "BatchAnalysisProgress", data: RUNNING }] },
    { command: "pauseBatchAnalysis", result: { ...RUNNING, phase: "Paused" } },
    { command: "stopBatchAnalysis", result: { ...RUNNING, phase: "Stopped", current_index: null } },
  ]);
  await menu(page, "analyzeMenu").click();
  await row(page, "batchAnalyze").click();
  const dialog = page.getByTestId("batch-dialog");
  await expect(dialog.getByTestId("batch-files")).toContainText("3 盘棋谱");
  await dialog.getByTestId("batch-deep").click();
  await dialog.getByTestId("batch-scope-range").click();
  await dialog.getByTestId("batch-first").fill("40");
  await expect(dialog).toHaveScreenshot("batch-dialog.png");
  await dialog.getByTestId("batch-start").click();

  const table = page.getByTestId("batch-table");
  await expect(table.getByTestId("batch-row")).toHaveCount(4);
  await expect(page.getByTestId("batch-chip")).toHaveText("批量 2/4");
  await expect(table).toHaveScreenshot("batch-table.png");
  await table.getByTestId("batch-pause").click();
  await expect(table.getByTestId("batch-phase")).toHaveText("已暂停");

  // 停止自动(批量)分析 also stops a batch.
  await menu(page, "analyzeMenu").click();
  await row(page, "stopAutoAnalyze").click();
  await expect(table.getByTestId("batch-phase")).toHaveText("已停止");
  const sent = await calls(page);
  expect(sent.map((call) => call.command)).toEqual(["pickOpenPaths", "startBatchAnalysis", "pauseBatchAnalysis", "stopBatchAnalysis"]);
  expect(sent[1]?.args).toEqual([
    { files: FILES, visits: 3000, scope: { type: "Range", data: { first_move: 40, last_move: null } }, force: false, output: { suffix: "已分析", directory: null } },
  ]);
});

test("while a batch runs, opening a game is explained instead of ignored", async ({ page }) => {
  await start(page, [{ command: "batchAnalysisStatus", result: RUNNING }]);
  await expect(page.getByTestId("batch-chip")).toHaveText("批量 2/4");
  await page.keyboard.press("KeyO");
  await expect(page.getByTestId("batch-message")).toContainText("批量分析进行中");
  expect((await calls(page)).map((call) => call.command)).not.toContain("pickOpenPath");
});

test("分析 → 精析设置 saves the depth; 全部分支精析 and 局部精析 use it", async ({ page }) => {
  await start(page, [
    { command: "setFlashAnalysis", result: { visits: 200, always_override: true } },
    { command: "startWholeGameAnalysis", result: { completed: 0, total: 40, visits: 200, status: { type: "Running" } } },
    { command: "cancelWholeGameAnalysis", result: { completed: 0, total: 40, visits: 200, status: { type: "Cancelled" } } },
    { command: "startWholeGameAnalysis", result: { completed: 0, total: 11, visits: 200, status: { type: "Running" } } },
  ]);
  await menu(page, "analyzeMenu").click();
  await row(page, "flashAnalyzeSettings").click();
  const settings = page.getByTestId("flash-settings");
  await settings.getByTestId("flash-visits").fill("200");
  await settings.getByTestId("flash-override").check();
  await expect(settings).toHaveScreenshot("flash-settings.png");
  await settings.getByTestId("flash-settings-save").click();
  await expect(settings).toHaveCount(0);

  await menu(page, "analyzeMenu").click();
  await row(page, "flashAnalyzeAllBranches").click();
  await menu(page, "analyzeMenu").click();
  await row(page, "stopAutoAnalyze").click();
  await menu(page, "analyzeMenu").click();
  await row(page, "flashAnalyzePartGame").click();
  await page.getByTestId("flash-first").fill("10");
  await page.getByTestId("flash-last").fill("20");
  await page.getByTestId("flash-range-start").click();
  await expect.poll(async () => (await calls(page)).filter((call) => call.command === "startWholeGameAnalysis").length).toBe(2);
  const starts = (await calls(page)).filter((call) => call.command === "startWholeGameAnalysis").map((call) => call.args[0]);
  expect(starts).toEqual([
    { visits: 200, scope: { type: "AllBranches" }, force: true },
    { visits: 200, scope: { type: "Range", data: { first_move: 10, last_move: 20 } }, force: true },
  ]);
});
