import { expect, test, type Page } from "@playwright/test";
import type { EngineDiagnostics, JavaImportPreview, OfficialWeight, Settings } from "../src/api/backend";
import { documentChanged, midGame } from "./fixtures";
import type { MockCommand, MockScript } from "./mock-backend";

test.use({ viewport: { width: 1280, height: 820 }, deviceScaleFactor: 1, locale: "zh-CN" });

const B18 = { program: "/opt/katago", model: "/opt/weights/kata1-b18c384nbt.bin.gz", config: "", extra_args: [] };
const B28 = { program: "/opt/katago", model: "/opt/weights/kata1-b28c512nbt.bin.gz", config: "", extra_args: [] };
const ENGINES = [
  { id: "e1", name: "B18 均衡", profile: B18 },
  { id: "e2", name: "B28 强力", profile: B28 },
];
const TWO: Settings = {
  version: 1,
  engine: B18,
  pondering: false,
  fox_history: [],
  move_rank: { mode: "Auto", winrate_thresholds: [1, 3, 6, 12, 24], score_thresholds: [0.5, 1.5, 3, 6, 12] },
  tsumego: { to_play: "Inherit", attacker: "Auto", ko_threat: "Both", wall_distance: 1 },
  engines: ENGINES,
  main_engine_id: "e1",
};
const DEFAULT_OPTIONS = { rules: { type: "FollowSgf" }, read_komi: true, default_komi: 7.5, auto_check_alive: true, report_interval_ms: 100 } as const;

async function start(page: Page, commands: MockCommand[], settings: Settings = TWO) {
  const script: MockScript = { initialEvents: [documentChanged(midGame(), 3)], commands: [{ command: "settings", result: settings }, ...commands] };
  await page.addInitScript((initial) => {
    window.__mockInitialScript = initial;
  }, script);
  await page.goto("/");
  await expect(page.getByTestId("board")).toHaveAttribute("data-ready", "true");
}

async function calls(page: Page) {
  return page.evaluate(() => window.__mockBackend?.recordedCalls() ?? []);
}

const row = (page: Page, id: string) => page.locator(`.menu-row[data-menu-id="${id}"]`);
const engineMenu = (page: Page) => page.getByTestId("menubar").locator('[data-menu-id="engineMenu"]');
const settingsMenu = (page: Page) => page.getByTestId("menubar").locator('[data-menu-id="settings"]');

test("the engine menu lists every engine; a failed switch says why and keeps the main engine", async ({ page }) => {
  await start(page, [
    { command: "selectMainEngine", result: TWO, error: { type: "App", data: "engine exited with status 1" } },
    { command: "selectMainEngine", result: { ...TWO, engine: B28, main_engine_id: "e2" } },
  ]);
  await engineMenu(page).click();
  await expect(row(page, "engine0")).toContainText("[1] B18 均衡");
  await expect(row(page, "engine1")).toContainText("[2] B28 强力");
  await expect(row(page, "engine0")).toHaveAttribute("aria-checked", "true");
  await expect(page.locator(".menu-popup").last()).toHaveScreenshot("engine-menu.png");

  await row(page, "engine1").click();
  await expect(page.getByTestId("engine-failed")).toContainText("engine exited with status 1");
  await page.getByTestId("engine-failed").getByRole("button").click();
  await engineMenu(page).click();
  await expect(row(page, "engine0")).toHaveAttribute("aria-checked", "true");

  await row(page, "engine1").click();
  await expect(engineMenu(page)).toHaveText(/^\[2\]: /);
  expect((await calls(page)).filter((call) => call.command === "selectMainEngine").map((call) => call.args)).toEqual([["e2"], ["e2"]]);
});

test("设置 → 引擎 lists the engines: reorder, rename and delete them", async ({ page }) => {
  const swapped = { ...TWO, engines: [ENGINES[1]!, ENGINES[0]!] };
  const renamed = { ...swapped, engines: [ENGINES[1]!, { ...ENGINES[0]!, name: "B18 快速" }] };
  await start(page, [
    { command: "reorderEngines", result: swapped },
    { command: "detectEngines", result: { programs: [B18.program], models: [B18.model, B28.model], configs: [] } },
    { command: "updateEngine", result: renamed },
    { command: "removeEngine", result: { ...renamed, engines: [renamed.engines[1]!] } },
  ]);
  await settingsMenu(page).click();
  await row(page, "engineConfig").click();
  const list = page.getByTestId("engine-list");
  await expect(list.getByTestId("engine-row")).toHaveCount(2);
  await expect(list.getByTestId("engine-row").first()).toHaveAttribute("data-main", "true");
  await expect(list).toHaveScreenshot("engine-list.png");

  await list.getByTestId("engine-row").nth(1).getByRole("button", { name: "上移" }).click();
  await expect(list.getByTestId("engine-row").first()).toContainText("B28 强力");
  await list.getByTestId("engine-row").nth(1).getByTestId("engine-edit").click();
  await page.getByTestId("setup-name").fill("B18 快速");
  await page.getByTestId("setup-start").click();
  await expect(page.getByTestId("engine-setup")).toBeHidden();
  await expect(list.getByTestId("engine-row").nth(1)).toContainText("B18 快速");
  await list.getByTestId("engine-row").first().getByTestId("engine-remove").click();
  await expect(list.getByTestId("engine-row")).toHaveCount(1);
  expect((await calls(page)).slice(1).map((call) => [call.command, call.args])).toEqual([
    ["reorderEngines", [["e2", "e1"]]],
    ["detectEngines", []],
    ["updateEngine", ["e1", "B18 快速", { ...B18, config: null }]],
    ["removeEngine", ["e2"]],
  ]);
});

test("设置 → 引擎规则 and 综合设置's engine items are saved", async ({ page }) => {
  // The app read the limits at start; the mock answered from its defaults.
  const limits = { limit_time: true, max_analyze_time_seconds: 600, limit_playout: false, limit_playouts: 100000 };
  await start(page, [
    { command: "setEngineSettings", result: { ...DEFAULT_OPTIONS, rules: { type: "Japanese" } } },
    { command: "setAnalysisLimits", result: limits },
    { command: "setEngineSettings", result: { ...DEFAULT_OPTIONS, rules: { type: "Japanese" }, read_komi: false, default_komi: 6.5, report_interval_ms: 250 } },
  ]);
  await settingsMenu(page).click();
  await row(page, "engineRules").click();
  await expect(page.getByTestId("rules-FollowSgf")).toBeChecked();
  await page.getByTestId("rules-Japanese").check();
  await page.getByTestId("rules-save").click();
  await expect(page.getByTestId("rules-dialog")).toBeHidden();

  await settingsMenu(page).click();
  await row(page, "comprehensiveSettings").click();
  await page.getByTestId("settings-read-komi").uncheck();
  await page.getByTestId("settings-default-komi").fill("6.5");
  await page.getByTestId("settings-interval").fill("5");
  await expect(page.getByTestId("settings-save")).toBeDisabled();
  await page.getByTestId("settings-interval").fill("250");
  await page.getByTestId("settings-save").click();
  await expect(page.getByTestId("settings-dialog")).toBeHidden();
  expect((await calls(page)).filter((call) => call.command === "setEngineSettings").map((call) => call.args[0])).toEqual([
    { ...DEFAULT_OPTIONS, rules: { type: "Japanese" } },
    { ...DEFAULT_OPTIONS, rules: { type: "Japanese" }, read_komi: false, default_komi: 6.5, report_interval_ms: 250 },
  ]);
});

test("导入 Java 版配置 previews what it would import and writes it only when confirmed", async ({ page }) => {
  const preview: JavaImportPreview = {
    preview_id: "p1",
    directory: "/Users/me/LizzieYzy",
    engines: [
      { source_index: 0, entry: { id: "j1", name: "KataGo 40b", profile: B18 }, issues: [] },
      { source_index: 1, entry: { id: "j2", name: "远程 GPU", profile: B28 }, issues: ["程序不存在：/opt/remote/katago"] },
    ],
    imported_main_engine_id: "j1",
    issues: ["SSH 引擎的密码没有导入，需要重新输入。"],
  };
  const imported = { ...TWO, engines: [...ENGINES, preview.engines![0]!.entry!, preview.engines![1]!.entry!] };
  await start(page, [
    { command: "pickDirectory", result: "/Users/me/LizzieYzy" },
    { command: "previewJavaImport", result: preview },
    { command: "confirmJavaImport", result: imported as Settings },
  ]);
  await settingsMenu(page).click();
  await row(page, "importJavaConfig").click();
  const dialog = page.getByTestId("java-import");
  await expect(dialog.getByTestId("java-import-engine")).toHaveCount(2);
  await expect(dialog.getByTestId("java-import-issues")).toContainText("SSH");
  await expect(dialog).toHaveScreenshot("java-import.png");
  await dialog.getByTestId("java-import-confirm").click();
  await expect(dialog).toBeHidden();
  await engineMenu(page).click();
  await expect(row(page, "engine3")).toContainText("[4] 远程 GPU");
  expect((await calls(page)).slice(1).map((call) => [call.command, call.args])).toEqual([
    ["pickDirectory", []],
    ["previewJavaImport", ["/Users/me/LizzieYzy"]],
    ["confirmJavaImport", ["p1"]],
  ]);
});

test("引擎一键设置 → 权重管理 downloads with progress and can cancel", async ({ page }) => {
  const weights: OfficialWeight[] = [
    { id: "b11-flagship", name: "Transformer 11B 旗舰版", size_bytes: 260_046_848, published_at: "2026-09-25", downloaded: true, path: "/data/weights/b11.bin.gz" },
    { id: "b10-balanced", name: "Transformer 10B 均衡版", size_bytes: 178_257_920, published_at: "2026-08-02", downloaded: false, path: null },
  ];
  const downloading = { phase: "Downloading", asset_id: "b10-balanced", downloaded_bytes: 71_303_168, total_bytes: 178_257_920, path: null, error: null } as const;
  await start(page, [
    { command: "officialWeights", result: weights },
    { command: "startAssetDownload", result: downloading, events: [{ type: "AssetDownloadProgress", data: downloading }] },
    { command: "cancelAssetDownload", result: { ...downloading, phase: "Cancelled" } },
  ]);
  await settingsMenu(page).click();
  await row(page, "autoSetup").click();
  const dialog = page.getByTestId("benchmark-dialog");
  await dialog.getByTestId("tools-tab-weights").click();
  await expect(dialog.getByTestId("tools-weight")).toHaveCount(2);
  await dialog.getByTestId("tools-download").click();
  await expect(dialog.getByTestId("tools-progress")).toContainText("68.0 / 170.0 MB");
  await expect(dialog).toHaveScreenshot("weights-downloading.png");
  await dialog.getByTestId("tools-cancel").click();
  await expect(dialog.getByTestId("tools-progress")).toBeHidden();
  expect((await calls(page)).map((call) => call.command)).toEqual(["settings", "officialWeights", "startAssetDownload", "cancelAssetDownload"]);
});

test("引擎一键设置 → 概览 shows each check; a repair runs and the checks come back", async ({ page }) => {
  const broken: EngineDiagnostics = {
    profile: B18,
    checks: [
      { kind: "ProgramExists", status: "Ok", path: B18.program, detail: null, repairs: [] },
      { kind: "ProgramVersion", status: "Ok", path: null, detail: "KataGo v1.18.2", repairs: [] },
      { kind: "ModelExists", status: "Error", path: B18.model, detail: "找不到权重文件", repairs: ["UseBundledModel"] },
      { kind: "ConfigValid", status: "Ok", path: null, detail: null, repairs: [] },
    ],
  };
  const fixed: EngineDiagnostics = { ...broken, checks: broken.checks!.map((check) => ({ ...check, status: "Ok", detail: null, repairs: [] })) };
  await start(page, [
    { command: "diagnoseEngine", result: broken },
    { command: "repairEngine", result: fixed },
  ]);
  await settingsMenu(page).click();
  await row(page, "autoSetup").click();
  const dialog = page.getByTestId("benchmark-dialog");
  await expect(dialog.getByTestId("tools-check")).toHaveCount(4);
  await expect(dialog.locator('[data-testid="tools-check"][data-status="Error"]')).toContainText("找不到权重文件");
  await expect(dialog).toHaveScreenshot("overview-error.png");
  await dialog.getByTestId("tools-repair-UseBundledModel").click();
  await expect(dialog.locator('[data-testid="tools-check"][data-status="Error"]')).toHaveCount(0);
  expect((await calls(page)).slice(1).map((call) => [call.command, call.args])).toEqual([
    ["diagnoseEngine", []],
    ["repairEngine", ["UseBundledModel"]],
  ]);
});
