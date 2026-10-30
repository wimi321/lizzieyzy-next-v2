import { expect, test, type Page } from "@playwright/test";
import type { FoxGame } from "../src/api/backend";
import { analysedGame, documentChanged, emptyNineteen, foxSearchResult } from "./fixtures";
import type { MockCommand, MockScript } from "./mock-backend";

test.use({ viewport: { width: 1280, height: 820 }, locale: "zh-CN" });

const ENGINE = { program: "/opt/homebrew/bin/katago", model: "/opt/katago/kata1.bin.gz", config: "", extra_args: [] };
const ACCOUNT = foxSearchResult().account;

function olderGame(id: string, opponent: string, result: FoxGame["result"], moves: number): FoxGame {
  return {
    id,
    start_time: "2026-10-28 21:14:09",
    black: { uid: "777", nickname: opponent, rank: "8段" },
    white: { uid: ACCOUNT.uid, nickname: ACCOUNT.nickname, rank: "9段" },
    result,
    move_count: moves,
  };
}

/** The app reads its settings as soon as it mounts, so they lead the script. */
async function start(page: Page, commands: MockCommand[]) {
  const script: MockScript = {
    initialEvents: [documentChanged(emptyNineteen(), 1)],
    commands: [
      { command: "settings", result: { version: 1, engine: ENGINE, pondering: false, fox_history: [ACCOUNT] } },
      ...commands,
    ],
  };
  await page.addInitScript((initial) => {
    window.__mockInitialScript = initial;
  }, script);
  await page.goto("/");
}

async function calls(page: Page) {
  return page.evaluate(() => window.__mockBackend?.recordedCalls() ?? []);
}

test("search a nickname, page through the games and open one", async ({ page }) => {
  const settled = { version: 1, engine: ENGINE, pondering: false, fox_history: [ACCOUNT] };
  await start(page, [
    { command: "foxSearch", result: foxSearchResult() },
    { command: "settings", result: settled },
    {
      command: "foxNextPage",
      result: {
        games: [
          olderGame("202610280002", "夜半听雨", { winner: "White", kind: "Score", score: 2.5, unit: "Points" }, 247),
          olderGame("202610280001", "清风徐来", { winner: "Black", kind: "Timeout", score: null, unit: null }, 96),
        ],
        next_cursor: null,
      },
    },
    { command: "foxOpenGame", result: 2, events: [documentChanged(analysedGame(), 2)] },
  ]);

  await page.getByTestId("open-fox").click();
  const dialog = page.getByTestId("fox-dialog");
  await expect(dialog.getByRole("heading")).toHaveText("野狐棋谱");
  await expect(dialog.getByTestId("fox-recent")).toHaveText(ACCOUNT.nickname);

  await dialog.getByTestId("fox-input").fill(ACCOUNT.nickname);
  await dialog.getByTestId("fox-input").press("Enter");
  await expect(dialog.getByTestId("fox-account")).toHaveText(`${ACCOUNT.nickname} · UID ${ACCOUNT.uid}`);
  await expect(dialog.getByTestId("fox-game")).toHaveCount(1);
  await expect(dialog.getByTestId("fox-game").first()).toContainText("黑中盘胜");

  await dialog.getByTestId("fox-more").click();
  await expect(dialog.getByTestId("fox-game")).toHaveCount(3);
  await expect(dialog.getByText("没有更多棋谱了")).toBeVisible();
  await expect(dialog.getByTestId("fox-game").nth(1)).toContainText("白胜 2.5 目");
  await expect(dialog.getByTestId("fox-game").nth(2)).toContainText("黑超时胜");
  await expect(dialog).toHaveScreenshot("fox-games.png");

  await dialog.getByTestId("fox-game").nth(1).click();
  await expect(dialog).toBeHidden();
  await expect(page.getByTestId("board")).toBeVisible();
  await expect.poll(() => calls(page)).toEqual([
    { command: "settings", args: [] },
    { command: "foxSearch", args: [{ type: "Nickname", value: ACCOUNT.nickname }] },
    { command: "settings", args: [] },
    { command: "foxNextPage", args: [ACCOUNT.uid, foxSearchResult().page.next_cursor] },
    { command: "foxOpenGame", args: ["202610280002"] },
  ]);
});

test("a recent account searches by UID and a Fox failure is explained", async ({ page }) => {
  await start(page, [
    { command: "foxSearch", result: foxSearchResult(), error: { type: "Fox", data: { type: "Timeout" } } },
  ]);

  await page.getByTestId("open-fox").click();
  const dialog = page.getByTestId("fox-dialog");
  await dialog.getByTestId("fox-recent").click();
  await expect(dialog.getByTestId("fox-error")).toHaveText("野狐服务器响应太慢，请稍后重试");
  await expect(dialog.getByTestId("fox-input")).toHaveValue(ACCOUNT.uid);
  await expect(dialog.getByTestId("fox-mode-uid")).toHaveAttribute("aria-checked", "true");
  await expect.poll(() => calls(page)).toEqual([
    { command: "settings", args: [] },
    { command: "foxSearch", args: [{ type: "Uid", value: ACCOUNT.uid }] },
  ]);

  await dialog.getByTestId("fox-close").click();
  await expect(dialog).toBeHidden();
});
