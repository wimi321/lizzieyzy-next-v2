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

test("文件 → 更多保存 saves the board as a real PNG under a name taken from the game, and a clean record", async ({ page }) => {
  await start(page, [
    { command: "pickImagePath", result: "/pictures/game-3.png" },
    { command: "savePng", result: undefined },
    { command: "pickSavePath", result: "/games/game-raw.sgf" },
    { command: "saveCleanSgf", result: undefined },
  ]);
  await menu(page, "fileMenu").click();
  await row(page, "saveMore").hover();
  const popups = page.getByTestId("menu-popup");
  await expect(popups).toHaveCount(2);
  await expect(row(page, "saveWinrate")).not.toHaveAttribute("aria-disabled");
  await expect(popups.nth(1)).toHaveScreenshot("more-save.png");
  await row(page, "saveMainBoardScreen").click();
  await expect.poll(async () => (await calls(page)).map((call) => call.command)).toEqual(["pickImagePath", "savePng"]);
  // The bytes Rust receives are the board as drawn: a PNG of the board's size in device pixels.
  const png = await page.evaluate(() => {
    const call = window.__mockBackend?.recordedCalls().find((recorded) => recorded.command === "savePng");
    const bytes = call?.args[1] as Uint8Array;
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    return { path: call?.args[0], signature: Array.from(bytes.slice(0, 8)), width: view.getUint32(16), height: view.getUint32(20) };
  });
  const canvas = await page.getByTestId("board").locator("canvas").first().evaluate((node: HTMLCanvasElement) => [node.width, node.height]);
  expect(png).toEqual({ path: "/pictures/game-3.png", signature: [137, 80, 78, 71, 13, 10, 26, 10], width: canvas[0], height: canvas[1] });

  await page.keyboard.press("Control+Shift+KeyS");
  await expect.poll(async () => (await calls(page)).map((call) => call.command).slice(2)).toEqual(["pickSavePath", "saveCleanSgf"]);
  expect((await calls(page)).slice(2).map((call) => call.args)).toEqual([["LizzieYzy-raw.sgf"], ["/games/game-raw.sgf", false, false]]);
});

test("the winrate graph is saved as a PNG; Java's keys copy the game and turn the board", async ({ page }) => {
  await start(page, [
    { command: "pickImagePath", result: "/pictures/winrate.png" },
    { command: "savePng", result: undefined },
    { command: "copySgf", result: undefined },
    { command: "transformBoard", result: 4 },
  ]);
  await menu(page, "fileMenu").click();
  await row(page, "saveMore").hover();
  await row(page, "saveWinrate").click();
  await expect.poll(async () => (await calls(page)).map((call) => call.command)).toEqual(["pickImagePath", "savePng"]);
  const signature = await page.evaluate(() => {
    const call = window.__mockBackend?.recordedCalls().find((recorded) => recorded.command === "savePng");
    return Array.from((call?.args[1] as Uint8Array).slice(0, 4));
  });
  expect(signature).toEqual([137, 80, 78, 71]);

  await page.keyboard.press("Control+KeyC");
  await page.keyboard.press("Control+Alt+ArrowRight");
  await expect.poll(async () => (await calls(page)).map((call) => [call.command, call.args]).slice(2)).toEqual([
    ["copySgf", []],
    ["transformBoard", ["RotateRight"]],
  ]);
});

test("pasting with nothing usable on the clipboard says so", async ({ page }) => {
  await start(page, [{ command: "pasteSgf", result: 0, error: { type: "Clipboard", data: { type: "EmptyText" } } }]);
  await menu(page, "fileMenu").click();
  await row(page, "pasteSgf").click();
  await expect(page.getByTestId("everyday-failed")).toContainText("剪贴板里没有文字。");
  await page.getByTestId("everyday-failed-ok").click();
  await expect(page.getByTestId("everyday-failed")).toHaveCount(0);
});

test("设置 → 语言 → 日本語 switches the window to Japanese", async ({ page }) => {
  await start(page, [{ command: "setLanguage", result: "Ja" }]);
  await menu(page, "settings").click();
  await row(page, "language").hover();
  await row(page, "languageJP").click();
  await expect(menu(page, "fileMenu")).toHaveText("ファイル");
  await expect(page).toHaveScreenshot("japanese-window.png", { clip: { x: 0, y: 0, width: 1280, height: 300 } });
  expect((await calls(page)).map((call) => [call.command, call.args])).toEqual([["setLanguage", ["Ja"]]]);
});

test("设置 → 界面字号 → 大 enlarges the whole window and saves the size", async ({ page }) => {
  await start(page, [{ command: "setUiScale", result: 1.3 }]);
  await menu(page, "settings").click();
  await row(page, "frameFontSize").hover();
  await row(page, "frameFontSizeBig").click();
  await expect.poll(() => page.evaluate(() => document.documentElement.style.zoom)).toBe("1.3");
  await expect(page).toHaveScreenshot("large-font-window.png", { clip: { x: 0, y: 0, width: 1280, height: 300 } });
  await menu(page, "settings").click();
  await row(page, "frameFontSize").hover();
  await expect(row(page, "frameFontSizeBig")).toHaveAttribute("aria-checked", "true");
  expect((await calls(page)).map((call) => [call.command, call.args])).toEqual([["setUiScale", [1.3]]]);
});
