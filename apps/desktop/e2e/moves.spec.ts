import { expect, test, type Page } from "@playwright/test";
import type { DocumentView, PointView, WireAppEvent } from "../src/api/backend";
import { analysisUpdated, documentChanged, midGame, midGameAnalysis } from "./fixtures";
import type { MockCommand } from "./mock-backend";

// The window of the reference screenshots, in LizzieYzy Next's own language.
test.use({ viewport: { width: 1429, height: 941 }, deviceScaleFactor: 1, locale: "zh-CN" });

async function start(page: Page, events: WireAppEvent[], commands: MockCommand[] = []) {
  await page.goto("/");
  await page.evaluate(([initial, scripted]) => window.__mockBackend?.setScript({ initialEvents: initial, commands: scripted }), [events, commands] as const);
  const board = page.getByTestId("board");
  await expect(board).toHaveAttribute("data-ready", "true");
  // Assets load independently of layout: the pointer tests read the geometry, so wait for it.
  await expect(board).not.toHaveAttribute("data-cell", "0.000");
}

async function calls(page: Page) {
  return page.evaluate(() => window.__mockBackend?.recordedCalls() ?? []);
}

async function pixelOf(page: Page, point: PointView) {
  const board = page.getByTestId("board");
  const read = async (name: string) => Number(await board.getAttribute(name));
  const [cell, originX, originY] = await Promise.all([read("data-cell"), read("data-origin-x"), read("data-origin-y")]);
  return { x: originX + point.x * cell, y: originY + point.y * cell };
}

const row = (page: Page, id: string) => page.locator(`.menu-row[data-menu-id="${id}"]`);
const menu = (page: Page, name: string) => page.getByTestId("menubar").getByRole("menuitem", { name });

const D16 = { x: 3, y: 3 };
const C3 = { x: 2, y: 16 };
const play = (color: "Black" | "White", point: PointView) => ({ type: "Play", data: { color, point } }) as const;

test("right-clicking a stone offers Java's stone menu: switch, delete, move, go to its move, review", async ({ page }) => {
  await start(page, [documentChanged(midGame(), 7)], [
    { command: "switchStone", result: 8 },
    { command: "deleteStone", result: 9 },
    { command: "moveStone", result: 10 },
    { command: "stoneOrigin", result: { revision: 7, node_id: 1, move_number: 1, review_parent: 0 } },
    { command: "goTo", result: 11 },
    { command: "stoneOrigin", result: { revision: 7, node_id: 1, move_number: 1, review_parent: 0 } },
    { command: "goTo", result: 12 },
  ]);
  const board = page.getByTestId("board");
  await board.click({ button: "right", position: await pixelOf(page, D16) });
  const popup = page.getByTestId("board-menu");
  await expect(popup.getByRole("menuitem")).toHaveText(["跳到此步", "移动", "反色", "删除", "回看"]);
  await expect(popup).toHaveScreenshot("board-menu-stone.png");
  await row(page, "switch-stone").click();
  await expect(popup).toBeHidden();
  await board.click({ button: "right", position: await pixelOf(page, D16) });
  await row(page, "delete-stone").click();
  // 移动: pick the stone up, put it down elsewhere.
  await board.click({ button: "right", position: await pixelOf(page, D16) });
  await row(page, "move-stone").click();
  await expect(page.getByTestId("move-chip")).toBeVisible();
  await board.click({ position: await pixelOf(page, C3) });
  await expect(page.getByTestId("move-chip")).toBeHidden();
  await board.click({ button: "right", position: await pixelOf(page, D16) });
  await row(page, "find-stone").click();
  await expect.poll(async () => (await calls(page)).length).toBe(5);
  await board.click({ button: "right", position: await pixelOf(page, D16) });
  await row(page, "review-stone").click();
  await expect.poll(() => calls(page)).toEqual([
    { command: "switchStone", args: [D16] },
    { command: "deleteStone", args: [D16] },
    { command: "moveStone", args: [D16, C3] },
    { command: "stoneOrigin", args: [D16] },
    { command: "goTo", args: [1] },
    { command: "stoneOrigin", args: [D16] },
    // 回看 starts just before the stone was played.
    { command: "goTo", args: [0] },
  ]);
});

test("right-clicking an empty point inserts a move; on a candidate it adds the line as a branch", async ({ page }) => {
  const analysis = midGameAnalysis(7, 20);
  await start(page, [documentChanged(midGame(), 7), analysisUpdated(analysis)], [
    { command: "insertMove", result: 8 },
    { command: "addVariation", result: 9 },
    { command: "step", result: 10 },
  ]);
  const board = page.getByTestId("board");
  await board.click({ button: "right", position: await pixelOf(page, C3) });
  await expect(page.getByTestId("board-menu").getByRole("menuitem")).toHaveText(["插入黑子", "插入白子", "回退一手"]);
  await row(page, "insert-white").click();
  const best = analysis.candidates[0]!;
  await board.click({ button: "right", position: await pixelOf(page, best.point!) });
  await expect(page.getByTestId("board-menu").getByRole("menuitem").first()).toHaveText("添加为分支");
  await row(page, "add-branch").click();
  await board.click({ button: "right", position: await pixelOf(page, C3) });
  // Esc closes the menu without a choice.
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("board-menu")).toBeHidden();
  await board.click({ button: "right", position: await pixelOf(page, C3) });
  await row(page, "previous-move").click();
  await expect.poll(() => calls(page)).toEqual([
    { command: "insertMove", args: [play("White", C3)] },
    { command: "addVariation", args: [best.pv] },
    { command: "step", args: [-1] },
  ]);
});

test("添加黑子 and 添加白子 play one color whatever the turn; 插入黑子 inserts on every click", async ({ page }) => {
  await start(page, [documentChanged(midGame(), 7)], [
    { command: "playColored", result: 8 },
    { command: "playColored", result: 9 },
    { command: "playColored", result: 10 },
    { command: "insertMove", result: 11 },
  ]);
  const board = page.getByTestId("board");
  await expect(page.getByTestId("tool-alternately")).toHaveAttribute("aria-pressed", "true");
  await page.getByTestId("tool-play-white").click();
  await expect(page.getByTestId("tool-play-white")).toHaveAttribute("aria-pressed", "true");
  await board.click({ position: await pixelOf(page, C3) });
  // As Java's tooltip says, a right click adds the other color.
  await board.click({ button: "right", position: await pixelOf(page, { x: 2, y: 15 }) });
  await expect(page.getByTestId("board-menu")).toBeHidden();
  await menu(page, "编辑").click();
  await row(page, "addBlack").click();
  await expect(page.getByTestId("tool-play-black")).toHaveAttribute("aria-pressed", "true");
  await board.click({ position: await pixelOf(page, { x: 2, y: 14 }) });
  await page.getByTestId("tool-alternately").click();
  await expect(page.getByTestId("color-chip")).toBeHidden();

  await menu(page, "编辑").click();
  await row(page, "insertBlack").click();
  await expect(page.getByTestId("insert-chip")).toBeVisible();
  await board.click({ position: await pixelOf(page, { x: 16, y: 16 }) });
  await expect.poll(() => calls(page)).toEqual([
    { command: "playColored", args: [play("White", C3)] },
    { command: "playColored", args: [play("Black", { x: 2, y: 15 })] },
    { command: "playColored", args: [play("Black", { x: 2, y: 14 })] },
    { command: "insertMove", args: [play("Black", { x: 16, y: 16 })] },
  ]);
});

test("试下 plays on a copy: the title says so, what would replace the game waits, 恢复 brings the game back", async ({ page }) => {
  const trying = (on: boolean): DocumentView => {
    const view = midGame();
    view.trial = on;
    return view;
  };
  await start(page, [documentChanged(trying(false), 7)], [
    { command: "startTrial", result: 8, events: [documentChanged(trying(true), 8)] },
    { command: "endTrial", result: 9, events: [documentChanged(trying(false), 9)] },
  ]);
  await page.getByTestId("tool-more").click();
  await page.getByTestId("tool-try-play").click();
  const chip = page.getByTestId("trial-chip");
  await expect(chip).toBeVisible();
  await expect(page).toHaveTitle("试下中...");
  await expect(chip).toHaveScreenshot("trial-chip.png");
  for (const id of ["tool-new", "open-file", "open-fox", "tool-tsumego", "tool-goban", "bottom-clear"]) await expect(page.getByTestId(id)).toBeDisabled();
  await menu(page, "文件").click();
  await expect(row(page, "open")).toHaveAttribute("aria-disabled", "true");
  await page.keyboard.press("Escape");
  await chip.getByTestId("trial-back").click();
  await expect(chip).toBeHidden();
  await expect(page).not.toHaveTitle("试下中...");
  await expect(page.getByTestId("tool-new")).toBeEnabled();
  await expect.poll(() => calls(page)).toEqual([
    { command: "startTrial", args: [] },
    { command: "endTrial", args: [] },
  ]);
});
