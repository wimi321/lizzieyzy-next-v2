import { expect, test, type Page } from "@playwright/test";
import type { Settings } from "../src/api/backend";
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

test("帮助 → 检查更新 shows the new version, downloads it with progress and can cancel", async ({ page }) => {
  const downloading = { phase: "Downloading", downloaded_bytes: 13_107_200, total_bytes: 41_943_040, version: "0.3.0", error: null } as const;
  await start(page, [
    {
      command: "checkUpdate",
      result: { available: true, version: "0.3.0", notes: "· 引擎菜单可以切换多个引擎\n· 引擎一键设置新增权重管理和路径诊断\n· 修复 Java 版分析数据在棋盘上不显示", published_at: "2026-11-20T08:00:00Z" },
    },
    { command: "downloadUpdate", result: downloading, events: [{ type: "UpdateDownloadProgress", data: downloading }] },
    { command: "cancelUpdateDownload", result: { ...downloading, phase: "Cancelled" } },
  ]);
  await menu(page, "helpMenu").click();
  await row(page, "checkUpdate").click();
  const dialog = page.getByTestId("update-dialog");
  await expect(dialog.getByTestId("update-state")).toHaveText("有新版本 0.3.0");
  await expect(dialog).toHaveScreenshot("update-available.png");
  await dialog.getByTestId("update-download").click();
  await expect(dialog.getByTestId("update-progress")).toContainText("已下载 12.5 / 40.0 MB");
  await dialog.getByTestId("update-cancel").click();
  await expect(dialog.getByTestId("update-progress")).toBeHidden();
  expect((await calls(page)).map((call) => call.command)).toEqual(["checkUpdate", "downloadUpdate", "cancelUpdateDownload"]);
});

test("帮助 → 诊断与日志 starts the full log, exports a bundle and shows it in its folder; 停止完整日志 follows it", async ({ page }) => {
  await start(page, [
    { command: "startFullLog", result: { enabled: true, remaining_seconds: 1800 } },
    { command: "exportDiagnostics", result: "/Users/me/Library/Application Support/LizzieYzy Next 2/diagnostics/lizzieyzy-diagnostics-20261112-0930.zip" },
    { command: "revealInFolder", result: undefined },
    { command: "stopFullLog", result: { enabled: false, remaining_seconds: 0 } },
  ]);
  await menu(page, "helpMenu").click();
  await expect(row(page, "stopFullTrace")).toHaveAttribute("aria-disabled", "true");
  await row(page, "diagnosticsAndLogs").click();
  const dialog = page.getByTestId("diagnostics-dialog");
  await expect(dialog.getByTestId("diagnostics-log-dir")).toHaveText("/Users/me/Library/Logs/LizzieYzy Next 2");
  await dialog.getByTestId("diagnostics-start").click();
  await expect(dialog.getByTestId("diagnostics-full-log")).toHaveAttribute("data-on", "true");
  await dialog.getByTestId("diagnostics-export").click();
  await expect(dialog.getByTestId("diagnostics-bundle")).toContainText("lizzieyzy-diagnostics-20261112-0930.zip");
  // The countdown moves each second; the picture is taken with it hidden.
  await expect(dialog).toHaveScreenshot("diagnostics.png", { mask: [dialog.getByTestId("diagnostics-full-log")] });
  await dialog.getByTestId("diagnostics-reveal").click();
  await page.keyboard.press("Escape");

  await menu(page, "helpMenu").click();
  await row(page, "stopFullTrace").click();
  await expect.poll(async () => (await calls(page)).map((call) => call.command)).toEqual(["startFullLog", "exportDiagnostics", "revealInFolder", "stopFullLog"]);
  expect((await calls(page))[2]?.args).toEqual(["/Users/me/Library/Application Support/LizzieYzy Next 2/diagnostics/lizzieyzy-diagnostics-20261112-0930.zip"]);
});

test("设置 → 语言 switches the whole window to English and back, and saves the choice", async ({ page }) => {
  await start(page, [
    { command: "setLanguage", result: "En" },
    { command: "setLanguage", result: "System" },
  ]);
  await expect(menu(page, "fileMenu")).toHaveText("文件");
  await menu(page, "settings").click();
  await row(page, "language").hover();
  await expect(row(page, "languageDefault")).toHaveAttribute("aria-checked", "true");
  await row(page, "languageEN").click();
  await expect(menu(page, "fileMenu")).toHaveText("File");
  await expect(page.locator('[data-region="captured"]').getByTestId("to-play")).toHaveText("Black to play");
  // The menus, the toolbar and the information panel, in English.
  await expect(page).toHaveScreenshot("english-window.png", { clip: { x: 0, y: 0, width: 1280, height: 300 } });

  await menu(page, "settings").click();
  await row(page, "language").hover();
  await expect(row(page, "languageEN")).toHaveAttribute("aria-checked", "true");
  await row(page, "languageDefault").click();
  await expect(menu(page, "fileMenu")).toHaveText("文件");
  expect((await calls(page)).map((call) => [call.command, call.args])).toEqual([
    ["setLanguage", ["En"]],
    ["setLanguage", ["System"]],
  ]);
});

test("a saved English choice applies at start, whatever the system language", async ({ page }) => {
  await start(page, [], { ...SETTINGS, language: "En" } as Settings);
  await expect(menu(page, "fileMenu")).toHaveText("File");
});
