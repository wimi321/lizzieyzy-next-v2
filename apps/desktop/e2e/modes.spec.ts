import { expect, test, type Page } from "@playwright/test";
import type { DocumentView, PointView, WireAppEvent } from "../src/api/backend";
import { documentChanged, emptyNineteen, midGame } from "./fixtures";
import type { MockCommand } from "./mock-backend";

// The window of the reference screenshots, in LizzieYzy Next's own language.
test.use({ viewport: { width: 1429, height: 941 }, deviceScaleFactor: 1, locale: "zh-CN" });

async function start(page: Page, view: DocumentView, commands: MockCommand[] = []) {
  await page.goto("/");
  await page.evaluate(
    ([events, scripted]) => window.__mockBackend?.setScript({ initialEvents: events, commands: scripted }),
    [[documentChanged(view, 7)], commands] as [WireAppEvent[], MockCommand[]],
  );
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
const Q4 = { x: 15, y: 15 };

test("棋局 → 起始局面设置: the tools place and erase setup stones; the chip sets who plays first", async ({ page }) => {
  await start(page, emptyNineteen(), [
    { command: "setSetupStone", result: 8 },
    { command: "setSetupStone", result: 9 },
    { command: "setFirstPlayer", result: 10 },
    { command: "setSetupStone", result: 11 },
    { command: "clearSetup", result: 12 },
  ]);
  await menu(page, "棋局").click();
  await row(page, "startingPositionSetup").click();
  await row(page, "setupToolBlack").click();
  const chip = page.getByTestId("setup-chip");
  await expect(chip.getByTestId("setup-chip-black")).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByTestId("board")).toHaveAttribute("data-editing", "true");
  await expect(chip).toHaveScreenshot("setup-chip.png");

  const board = page.getByTestId("board");
  await board.click({ position: await pixelOf(page, D16) });
  await chip.getByTestId("setup-chip-white").click();
  await board.click({ position: await pixelOf(page, Q4) });
  await chip.getByTestId("setup-white-first").click();
  await chip.getByTestId("setup-chip-erase").click();
  await board.click({ position: await pixelOf(page, Q4) });
  await chip.getByTestId("setup-clear").click();
  await expect.poll(() => calls(page)).toEqual([
    { command: "setSetupStone", args: [D16, "Black"] },
    { command: "setSetupStone", args: [Q4, "White"] },
    { command: "setFirstPlayer", args: ["White"] },
    { command: "setSetupStone", args: [Q4, null] },
    { command: "clearSetup", args: [] },
  ]);

  // The menu shows the tool in hand; Esc puts it down.
  await menu(page, "棋局").click();
  await row(page, "startingPositionSetup").click();
  await expect(row(page, "setupModeToggle")).toHaveAttribute("aria-checked", "true");
  await expect(row(page, "setupToolErase")).toHaveAttribute("aria-checked", "true");
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await page.locator('[data-region="statistics"]').click();
  await page.keyboard.press("Escape");
  await expect(chip).toBeHidden();
  await expect(page.getByTestId("board")).not.toHaveAttribute("data-editing");
});

test("a game with moves becomes a starting position first, after Java's question", async ({ page }) => {
  await start(page, midGame(), [
    { command: "convertToStartPosition", result: 8, events: [documentChanged(emptyNineteen(), 8)] },
    { command: "setSetupStone", result: 9 },
  ]);
  await menu(page, "棋局").click();
  await row(page, "startingPositionSetup").click();
  // The game has moves: its setup stones cannot be edited until it is converted.
  await expect(row(page, "setupClearAll")).toHaveAttribute("aria-disabled", "true");
  await row(page, "setupModeToggle").click();
  const box = page.getByTestId("confirm-convert");
  await expect(box).toContainText("此操作将清除所有落子、停着和变着，是否继续？");
  await box.getByTestId("confirm-convert-ok").click();
  await expect(page.getByTestId("setup-chip")).toBeVisible();
  await page.getByTestId("board").click({ position: await pixelOf(page, D16) });
  await expect.poll(() => calls(page)).toEqual([
    { command: "convertToStartPosition", args: [] },
    { command: "setSetupStone", args: [D16, "Black"] },
  ]);
});

test("the markup tools mark the current move; the board draws its marks; 交换落子顺序 swaps who plays", async ({ page }) => {
  const view = midGame();
  // Every kind of mark, on black, white and empty points, all in the top-left quarter.
  view.markup = [
    { point: { x: 3, y: 3 }, kind: "Triangle", text: null },
    { point: { x: 4, y: 2 }, kind: "Square", text: null },
    { point: { x: 5, y: 3 }, kind: "Circle", text: null },
    { point: { x: 6, y: 6 }, kind: "Cross", text: null },
    { point: { x: 7, y: 8 }, kind: "Letter", text: "A" },
    { point: { x: 8, y: 8 }, kind: "Number", text: "12" },
    { point: { x: 2, y: 5 }, kind: "Letter", text: "B" },
  ];
  await start(page, view, [
    { command: "setMarkup", result: 8 },
    { command: "setMarkup", result: 9 },
    { command: "clearMarkup", result: 10 },
    { command: "toggleToPlay", result: 11 },
  ]);
  const board = page.getByTestId("board");
  // A quarter of the board keeps the baseline under the 1 MB file budget.
  const area = await board.boundingBox();
  if (!area) throw new Error("the board has no box");
  await expect(page).toHaveScreenshot("markup.png", { clip: { x: area.x, y: area.y, width: area.width / 2, height: area.height / 2 } });

  await page.getByTestId("tool-markup").click();
  await expect(page.getByTestId("tool-markup")).toHaveAttribute("aria-pressed", "true");
  const chip = page.getByTestId("markup-chip");
  // The tools are out, none in hand: the board still plays.
  await expect(board).not.toHaveAttribute("data-editing");
  await chip.getByTestId("markup-chip-triangle").click();
  await board.click({ position: await pixelOf(page, { x: 10, y: 10 }) });
  await chip.getByTestId("markup-chip-letter").click();
  await board.click({ position: await pixelOf(page, D16) });
  await chip.getByTestId("markup-clear").click();
  await page.getByTestId("tool-markup").click();
  await expect(chip).toBeHidden();
  await page.getByTestId("tool-change-turn").click();
  await expect.poll(() => calls(page)).toEqual([
    { command: "setMarkup", args: [{ x: 10, y: 10 }, "Triangle"] },
    { command: "setMarkup", args: [D16, "Letter"] },
    { command: "clearMarkup", args: [] },
    { command: "toggleToPlay", args: [] },
  ]);
});
