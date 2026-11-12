import { expect, test, type Page } from "@playwright/test";
import type { DocumentView, WireAppEvent } from "../src/api/backend";
import { documentChanged, midGame, recordedGame } from "./fixtures";
import type { MockCommand } from "./mock-backend";

// The window of the reference screenshots, in LizzieYzy Next's own language.
test.use({ viewport: { width: 1429, height: 941 }, deviceScaleFactor: 1, locale: "zh-CN" });

async function start(page: Page, view: DocumentView, commands: MockCommand[] = []) {
  await page.goto("/");
  await page.evaluate(
    ([events, scripted]) => window.__mockBackend?.setScript({ initialEvents: events, commands: scripted }),
    [[documentChanged(view, 7)], commands] as [WireAppEvent[], MockCommand[]],
  );
  await expect(page.getByTestId("board")).toHaveAttribute("data-ready", "true");
}

async function calls(page: Page) {
  return page.evaluate(() => window.__mockBackend?.recordedCalls() ?? []);
}

const row = (page: Page, id: string) => page.locator(`.menu-row[data-menu-id="${id}"]`);
const menu = (page: Page, name: string) => page.getByTestId("menubar").getByRole("menuitem", { name });

const NEW_BOARD = { width: 19, height: 19, komi: 7.5, handicap: 0, rules: "chinese", black_name: null, black_rank: null, white_name: null, white_rank: null };

test("a comment is edited where it is shown: a click starts, leaving saves, Escape gives up", async ({ page }) => {
  const view = midGame();
  view.comment = "旧评论";
  await start(page, view, [{ command: "setComment", result: 8 }]);
  await page.getByTestId("comment-text").click();
  const editor = page.getByTestId("comment-editor");
  await expect(editor).toBeFocused();
  await expect(editor).toHaveValue("旧评论");
  await editor.fill("新评论");
  // The keyboard belongs to the editor: the arrow keys do not step through the game.
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Space");
  await page.locator('[data-region="statistics"]').click();
  await expect(editor).toBeHidden();
  await expect.poll(() => calls(page)).toEqual([{ command: "setComment", args: ["新评论 "] }]);

  await page.getByTestId("comment-text").click();
  await page.getByTestId("comment-editor").fill("不要了");
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("comment-editor")).toBeHidden();
  await page.locator('[data-region="statistics"]').click();
  expect(await calls(page)).toHaveLength(1);
});

test("deleting a move or a branch, and undoing the edit, from Java's keys, toolbar and menu", async ({ page }) => {
  const { view, tree } = recordedGame(7);
  view.can_undo = true;
  tree.nodes[5]!.has_comment = true;
  await start(page, view, [
    { command: "variationTree", result: tree },
    { command: "deleteNode", result: 8 },
    { command: "deleteBranch", result: 9 },
    { command: "deleteNode", result: 10 },
    { command: "undo", result: 11 },
  ]);
  await expect(page.getByTestId("tree-comment")).toHaveCount(1);
  // On the main line there is nothing to promote.
  await expect(page.getByTestId("bottom-set-main")).toBeDisabled();
  await page.keyboard.press("Delete");
  await page.keyboard.press("Shift+Delete");
  await page.getByTestId("bottom-delete").click();
  await menu(page, "编辑").click();
  await expect(row(page, "redoDelete")).toHaveAttribute("aria-disabled", "true");
  await expect(row(page, "undoDelete")).toHaveText("撤销编辑");
  await row(page, "undoDelete").click();
  await expect.poll(() => calls(page)).toEqual([
    { command: "variationTree", args: [] },
    { command: "deleteNode", args: [20] },
    { command: "deleteBranch", args: [20] },
    { command: "deleteNode", args: [20] },
    { command: "undo", args: [] },
  ]);
});

test("off the main line, L and both toolbars make the branch the main line", async ({ page }) => {
  const { view, tree } = recordedGame(7);
  view.tree.current = 22;
  await start(page, view, [
    { command: "variationTree", result: tree },
    { command: "promoteToMain", result: 8 },
    { command: "promoteToMain", result: 9 },
    { command: "promoteToMain", result: 10 },
  ]);
  await expect(page.getByTestId("bottom-set-main")).toBeEnabled();
  await page.keyboard.press("l");
  await page.getByTestId("bottom-set-main").click();
  await page.getByTestId("tool-set-main").click();
  await expect.poll(() => calls(page)).toEqual([
    { command: "variationTree", args: [] },
    { command: "promoteToMain", args: [22] },
    { command: "promoteToMain", args: [22] },
    { command: "promoteToMain", args: [22] },
  ]);
});

test("a new board is asked about first; the board size follows Java's dialog", async ({ page }) => {
  await start(page, midGame(), [
    { command: "newGame", result: 8 },
    { command: "newGame", result: 9 },
    { command: "newGame", result: 10 },
  ]);
  await page.getByTestId("tool-new").click();
  const box = page.getByTestId("confirm-new-board");
  await expect(box).toContainText("此操作将会清空棋盘和所有分析,是否继续?");
  await expect(box).toHaveScreenshot("confirm-new-board.png");
  await box.getByTestId("confirm-new-board-cancel").click();
  await expect(box).toBeHidden();
  expect(await calls(page)).toEqual([]);

  await page.getByTestId("bottom-clear").click();
  await box.getByTestId("confirm-new-board-ok").click();
  await expect.poll(() => calls(page)).toEqual([{ command: "newGame", args: [NEW_BOARD] }]);

  // Ctrl+Home is Java's key; "do not ask again" unticks the switch in 显示 → 主界面设置.
  await page.keyboard.press("Control+Home");
  await box.getByTestId("confirm-new-board-no-again").check();
  await box.getByTestId("confirm-new-board-ok").click();
  await expect.poll(async () => (await calls(page)).length).toBe(2);
  await menu(page, "显示").click();
  await row(page, "mainPanelSettings").hover();
  await expect(row(page, "showNewBoardConfirmDialog")).toHaveAttribute("aria-checked", "false");
  await expect(row(page, "showReplaceFileConfirmDialog")).toHaveAttribute("aria-checked", "true");
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  // Closing a menu leaves the keyboard on the menu strip; the board's keys need the board.
  await page.locator('[data-region="statistics"]').click();

  await page.keyboard.press("Control+i");
  const size = page.getByTestId("board-size-dialog");
  await expect(size.getByTestId("board-size-19")).toHaveAttribute("aria-checked", "true");
  await expect(size).toHaveScreenshot("board-size.png");
  await size.getByTestId("board-size-9").click();
  await size.getByTestId("board-size-confirm").click();
  await expect(size).toBeHidden();
  await expect.poll(async () => (await calls(page))[2]).toEqual({ command: "newGame", args: [{ ...NEW_BOARD, width: 9, height: 9 }] });
});

test("the game's information is edited in Java's dialog, the komi also in the toolbar", async ({ page }) => {
  const { view } = recordedGame(7);
  view.game_info = { ...view.game_info, rules: "Chinese", rules_preset: "chinese", result: "B+R" };
  await start(page, view, [
    { command: "setGameInfo", result: 8 },
    { command: "setGameInfo", result: 9 },
  ]);
  await page.keyboard.press("i");
  const dialog = page.getByTestId("game-info-dialog");
  await expect(dialog.getByTestId("game-info-black_name")).toHaveValue("Black Player");
  await expect(dialog).toHaveScreenshot("game-info.png");
  await dialog.getByTestId("game-info-white_rank").fill("9段");
  await dialog.getByTestId("game-info-rules").selectOption("japanese");
  await dialog.getByTestId("game-info-confirm").click();
  await expect(dialog).toBeHidden();
  const unchanged = { black_name: "Black Player", black_rank: null, white_name: "White Player", result: "B+R", date: null, event: null };
  await expect.poll(() => calls(page)).toEqual([{ command: "setGameInfo", args: [{ ...unchanged, white_rank: "9段", komi: 7.5, rules: "japanese" }] }]);

  const komi = page.getByTestId("tool-komi");
  await komi.fill("6.5");
  await komi.press("Enter");
  await expect.poll(async () => (await calls(page))[1]).toEqual({ command: "setGameInfo", args: [{ ...unchanged, white_rank: null, komi: 6.5, rules: "chinese" }] });
  // Escape puts the game's own komi back.
  await komi.fill("99");
  await komi.press("Escape");
  await expect(komi).toHaveValue("7.5");
  expect(await calls(page)).toHaveLength(2);
});

test("保存 writes back to the game's file after asking; 另存为 asks where", async ({ page }) => {
  const view = midGame();
  view.file_name = "a.sgf";
  view.file_path = "/games/a.sgf";
  await start(page, view, [
    { command: "save", result: undefined },
    { command: "pickSavePath", result: "/games/b.sgf" },
    { command: "save", result: undefined },
  ]);
  await page.keyboard.press("Control+s");
  const box = page.getByTestId("confirm-replace");
  await expect(box).toContainText("/games/a.sgf");
  await expect(box).toHaveScreenshot("confirm-replace.png");
  await box.getByTestId("confirm-replace-ok").click();
  await expect(box).toBeHidden();
  await page.keyboard.press("s");
  await expect.poll(() => calls(page)).toEqual([
    { command: "save", args: ["/games/a.sgf"] },
    { command: "pickSavePath", args: [] },
    { command: "save", args: ["/games/b.sgf"] },
  ]);
});

test("文件 → 最近打开 lists the recent games, opens one and clears the list", async ({ page }) => {
  const view = midGame();
  view.file_name = "a.sgf";
  view.file_path = "/games/a.sgf";
  const recent = ["/games/a.sgf", "/games/名人战 第3局.sgf"];
  await start(page, view, [
    { command: "recentFiles", result: recent },
    { command: "openPath", result: 8 },
    { command: "recentFiles", result: recent },
    { command: "clearRecentFiles", result: undefined },
  ]);
  await menu(page, "文件").click();
  await row(page, "openRecent").click();
  await expect(row(page, "recent0")).toHaveText("a.sgf");
  await expect(row(page, "recent1")).toHaveText("名人战 第3局.sgf");
  await expect(row(page, "recent1")).toHaveAttribute("title", "/games/名人战 第3局.sgf");
  await row(page, "recent1").click();
  await expect.poll(async () => (await calls(page)).length).toBe(3);

  await menu(page, "文件").click();
  await row(page, "openRecent").click();
  await row(page, "clearRecent").click();
  await expect.poll(() => calls(page)).toEqual([
    { command: "recentFiles", args: [] },
    { command: "openPath", args: ["/games/名人战 第3局.sgf"] },
    { command: "recentFiles", args: [] },
    { command: "clearRecentFiles", args: [] },
  ]);
  await menu(page, "文件").click();
  await row(page, "openRecent").click();
  await expect(row(page, "recentEmpty")).toHaveAttribute("aria-disabled", "true");
});
