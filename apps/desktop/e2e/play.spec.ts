import { expect, test, type Page } from "@playwright/test";
import type { Settings } from "../src/api/backend";
import type { GtpConsoleLine } from "../src/api/gtp";
import type { GameView } from "../src/api/game";
import type { MatchView } from "../src/api/match-game";
import { documentChanged, midGame } from "./fixtures";
import type { MockCommand, MockScript } from "./mock-backend";

test.use({ viewport: { width: 1280, height: 820 }, deviceScaleFactor: 1, locale: "zh-CN" });

const B18 = { program: "/opt/katago", model: "/opt/weights/kata1-b18c384nbt.bin.gz", config: "", extra_args: [] };
const ZEN = { id: "gtp-1", name: "Zen 7", program: "/opt/zen/zen7", args: ["--gtp", "-t", "4"], working_dir: null, init_commands: [] };
const SETTINGS = {
  version: 1,
  engine: B18,
  pondering: false,
  engines: [{ id: "e1", name: "B18 均衡", profile: B18 }],
  main_engine_id: "e1",
  gtp_engines: [ZEN],
} as unknown as Settings;

const PLAYING: GameView = {
  phase: "Playing",
  black: { type: "Human" },
  white: { type: "Ai", data: { type: "KataGo", data: { seconds: 2 } } },
  to_play: "Black",
  ai_thinking: false,
  black_clock: { main_seconds: 754, periods: 3, byoyomi_seconds: 30 },
  move_number: 20,
};

async function start(page: Page, commands: MockCommand[], events: MockScript["initialEvents"] = []) {
  const script: MockScript = {
    initialEvents: [documentChanged(midGame(), 3), ...(events ?? [])],
    commands: [{ command: "settings", result: SETTINGS }, ...commands],
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

async function pick(page: Page, path: string[]) {
  await menu(page, path[0]!).click();
  for (const id of path.slice(1, -1)) await row(page, id).hover();
  await row(page, path.at(-1)!).click();
  await page.mouse.move(0, 0);
}

test("棋局 → 新对局 → 人机对局 starts a game with what the dialog shows", async ({ page }) => {
  await start(page, [{ command: "startGame", result: PLAYING, events: [{ type: "GameUpdated", data: PLAYING }] }]);
  await pick(page, ["gameMenu", "newGame", "newGenmoveGame"]);
  const dialog = page.getByTestId("play-dialog");
  await expect(dialog).toBeVisible();
  await page.getByTestId("play-clock").check();
  await expect(dialog).toHaveScreenshot("play-dialog.png");
  await page.getByTestId("play-begin").click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByTestId("play-chip")).toContainText("12:34");
  const [call] = await calls(page);
  expect(call?.command).toBe("startGame");
  expect(call?.args[0]).toMatchObject({ black: { type: "Human" }, white: { type: "Ai", data: { type: "KataGo" } }, human_clock: { main_seconds: 1200, byoyomi_seconds: 30, periods: 3 } });
});

test("during a game the toolbar takes back, resigns after asking, and the result shows in a corner", async ({ page }) => {
  const finished: GameView = { ...PLAYING, phase: "Finished", result: "W+R", end_reason: "Resignation" };
  await start(
    page,
    [
      { command: "takeBack", result: PLAYING },
      { command: "resignGame", result: finished, events: [{ type: "GameUpdated", data: finished }] },
    ],
    [{ type: "GameUpdated", data: PLAYING }],
  );
  const toolbar = page.getByTestId("top-toolbar");
  await expect(toolbar.getByTestId("tool-take-back")).toBeVisible();
  await expect(page.getByTestId("play-chip")).toHaveAttribute("data-state", "human");
  await expect(page).toHaveScreenshot("play-toolbar.png", { clip: { x: 0, y: 0, width: 1280, height: 96 } });
  await toolbar.getByTestId("tool-take-back").click();
  await toolbar.getByTestId("tool-resign").click();
  await page.getByTestId("play-confirm").getByRole("button", { name: "认输" }).click();
  const notice = page.getByTestId("play-notice");
  await expect(notice).toContainText("白胜 · 认输");
  await expect(notice).toHaveScreenshot("play-result.png");
  expect((await calls(page)).map((call) => call.command)).toEqual(["takeBack", "resignGame"]);
});

test("Alt+E opens 引擎对局; the score window follows the match", async ({ page }) => {
  const running: MatchView = { phase: "Running", games: 10, completed: 0, current_game: 1 };
  const scored: MatchView = {
    ...running,
    completed: 6,
    current_game: 7,
    a: { total: 4, black: 2, white: 2 },
    b: { total: 1, black: 1, white: 0 },
    draws: 0,
    unscored: { 超手数: 1 },
    elo: { elo: 240.8, lower: 12.4, upper: null },
  };
  await start(page, [{ command: "startMatch", result: running, events: [{ type: "MatchUpdated", data: scored }] }]);
  await page.keyboard.press("Alt+KeyE");
  const dialog = page.getByTestId("match-dialog");
  await expect(dialog).toBeVisible();
  await page.getByTestId("match-side-2-engine").selectOption("g:gtp-1");
  await page.getByTestId("match-games").fill("10");
  await expect(dialog).toHaveScreenshot("match-dialog.png");
  await page.getByTestId("match-begin").click();
  const panel = page.getByTestId("match-panel");
  await expect(panel.getByTestId("match-row-1")).toContainText("4");
  await expect(panel).toHaveScreenshot("match-panel.png");
  const [call] = await calls(page);
  expect(call?.args[0]).toMatchObject({ a: { type: "KataGo", data: { engine_id: "e1" } }, b: { type: "Gtp", data: { engine_id: "gtp-1" } }, games: 10, swap_colors: true });
});

test("设置 → GTP 引擎… lists the engines and tests one", async ({ page }) => {
  await start(page, [{ command: "testGtpEngine", result: { name: "Zen", version: "7.4", commands: [] } }]);
  await pick(page, ["settings", "gtpEngines"]);
  const dialog = page.getByTestId("gtp-engines");
  await dialog.getByTestId("gtp-test").click();
  await expect(dialog.getByTestId("gtp-test-result")).toHaveText("可用：Zen 7.4");
  await expect(dialog).toHaveScreenshot("gtp-engines.png");
});

test("显示 → 面板 → Gtp控制台 shows the transcript and sends a command", async ({ page }) => {
  const lines: GtpConsoleLine[] = [
    { engine_id: "gtp-1", sequence: 1, direction: "Sent", text: "name" },
    { engine_id: "gtp-1", sequence: 2, direction: "Received", text: "= Zen" },
    { engine_id: "gtp-1", sequence: 3, direction: "Sent", text: "kata-raw-nn 0" },
    { engine_id: "gtp-1", sequence: 4, direction: "Received", text: "? unknown command" },
    { engine_id: "gtp-1", sequence: 5, direction: "Stderr", text: "Zen: loading network…" },
  ];
  await start(page, [
    { command: "gtpConsole", result: lines },
    { command: "gtpConsoleSend", result: "= 7.4" },
  ]);
  await pick(page, ["viewMenu", "panel", "gtpPanel"]);
  const panel = page.getByTestId("gtp-console");
  await expect(panel.locator(".gtp-line")).toHaveCount(5);
  await expect(panel).toHaveScreenshot("gtp-console.png");
  await panel.getByTestId("gtp-console-input").fill("version");
  await panel.getByTestId("gtp-console-send").click();
  await expect(panel.getByTestId("gtp-console-input")).toHaveValue("");
  expect((await calls(page)).map((call) => [call.command, call.args])).toEqual([
    ["gtpConsole", ["gtp-1"]],
    ["gtpConsoleSend", ["gtp-1", "version"]],
  ]);
});
