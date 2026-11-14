import { expect, test, type Page } from "@playwright/test";
import type { Settings } from "../src/api/backend";
import type { TencentGame, TencentPage } from "../src/api/tencent";
import { analysedGame, documentChanged, emptyNineteen } from "./fixtures";
import type { MockCommand, MockScript } from "./mock-backend";

// Tencent sends Unix seconds; the list shows Beijing time on every machine that renders baselines.
test.use({ viewport: { width: 1280, height: 820 }, deviceScaleFactor: 1, locale: "zh-CN", timezoneId: "Asia/Shanghai" });

const ENGINE = { program: "/opt/homebrew/bin/katago", model: "/opt/katago/kata1.bin.gz", config: "", extra_args: [] };
const SETTINGS = { version: 1, engine: ENGINE, pondering: false, fox_history: [], tencent_history: ["910001"] } as unknown as Settings;
const ME = { uid: "910001", nickname: "样本棋手910001", rank: { type: "Dan", data: 5 } } as const;

function game(id: string, minutes: number, opponent: string, meBlack: boolean, result: TencentGame["result"], moves: number): TencentGame {
  const other = { uid: `9100${id.slice(-2)}`, nickname: opponent, rank: { type: "Kyu", data: 1 } } as const;
  return { id, start_time: 1_794_100_000 - minutes * 60, black: meBlack ? ME : other, white: meBlack ? other : ME, result, move_count: moves };
}

const PAGE: TencentPage = {
  next_cursor: null,
  games: [
    game("810011", 0, "样本棋手910011", true, { winner: "Black", kind: "Resignation", score: null, unit: null }, 187),
    game("810012", 95, "样本棋手910012", false, { winner: "Black", kind: "Score", score: 3.5, unit: "Points" }, 262),
    game("810013", 300, "样本棋手910013", false, { winner: "White", kind: "Timeout", score: null, unit: null }, 141),
  ],
};

const row = (page: Page, id: string) => page.locator(`.menu-row[data-menu-id="${id}"]`);

async function start(page: Page, commands: MockCommand[]) {
  const script: MockScript = { initialEvents: [documentChanged(emptyNineteen(), 1)], commands: [{ command: "settings", result: SETTINGS }, ...commands] };
  await page.addInitScript((initial) => {
    window.__mockInitialScript = initial;
  }, script);
  await page.goto("/");
}

async function calls(page: Page) {
  return page.evaluate(() => window.__mockBackend?.recordedCalls() ?? []);
}

test("腾讯棋谱 lists an account's games in Java's columns, its wins in red; a double-click opens one", async ({ page }) => {
  await start(page, [
    { command: "tencentSearch", result: PAGE },
    { command: "settings", result: { ...SETTINGS, tencent_history: ["样本棋手910001", "910001"] } },
    { command: "tencentOpenGame", result: 2, events: [documentChanged(analysedGame(), 2)] },
  ]);
  await page.getByTestId("open-tencent").click();
  const dialog = page.getByTestId("tencent-dialog");
  await expect(dialog.getByTestId("tencent-recent")).toHaveText(["910001"]);
  await dialog.getByTestId("tencent-input").fill("样本棋手910001");
  await dialog.getByTestId("tencent-input").press("Enter");
  await expect(dialog.getByTestId("tencent-game")).toHaveCount(3);
  await expect(dialog.getByTestId("tencent-recent")).toHaveText(["样本棋手910001", "910001"]);
  await expect(dialog.getByTestId("tencent-game").first()).toContainText("2026-11-08 09:06");
  await expect(dialog.locator(".tencent-row.won")).toHaveCount(2);
  await page.mouse.move(0, 0);
  await expect(dialog).toHaveScreenshot("tencent-games.png");

  await dialog.getByTestId("tencent-game").nth(1).dblclick();
  await expect(dialog).toBeHidden();
  expect(await calls(page)).toEqual([
    { command: "settings", args: [] },
    { command: "tencentSearch", args: ["样本棋手910001"] },
    { command: "settings", args: [] },
    { command: "tencentOpenGame", args: ["810012"] },
  ]);
});

test("文件 → 打开在线链接 starts with the clipboard's link, says why it was refused, then opens a good one; Q opens it too", async ({ page }) => {
  await start(page, [
    { command: "clipboardLink", result: "https://home.yikeweiqi.com/#/live/room/1" },
    { command: "openUrl", result: 0, error: { type: "OpenUrl", data: { type: "UnsupportedYike" } } },
    { command: "openUrl", result: 2, events: [documentChanged(analysedGame(), 2)] },
  ]);
  await page.getByTestId("menubar").locator('[data-menu-id="fileMenu"]').click();
  await row(page, "openUrl").click();
  const dialog = page.getByTestId("url-dialog");
  await expect(dialog.getByTestId("url-input")).toHaveValue("https://home.yikeweiqi.com/#/live/room/1");
  await dialog.getByTestId("url-open").click();
  await expect(dialog.getByTestId("url-error")).toHaveText("暂不支持弈客链接");
  await expect(dialog).toHaveScreenshot("open-url-error.png");

  await dialog.getByTestId("url-input").fill("https://example.org/games/demo.sgf");
  await dialog.getByTestId("url-input").press("Enter");
  await expect(dialog).toBeHidden();
  expect(await calls(page)).toEqual([
    { command: "settings", args: [] },
    { command: "clipboardLink", args: [] },
    { command: "openUrl", args: ["https://home.yikeweiqi.com/#/live/room/1"] },
    { command: "openUrl", args: ["https://example.org/games/demo.sgf"] },
  ]);
  await page.keyboard.press("q");
  await expect(dialog).toBeVisible();
});
