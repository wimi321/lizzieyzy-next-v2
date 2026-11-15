import { expect, test, type Page } from "@playwright/test";
import type { DisplaySettings, DocumentView, SaveSlot } from "../src/api/backend";
import { defaultDisplaySettings, documentChanged, midGame } from "./fixtures";
import type { MockCommand, MockScript } from "./mock-backend";

// Java's window in the reference screenshots, wide enough for the whole bottom toolbar.
test.use({ viewport: { width: 1429, height: 941 }, deviceScaleFactor: 1, locale: "zh-CN", timezoneId: "UTC" });

type Display = Required<DisplaySettings>;
const saved = (change: Partial<Display>): Display => ({ ...defaultDisplaySettings, ...change });
const ENGINE = { program: "/opt/katago", model: "/opt/model.bin.gz", config: "", extra_args: [] };

/** The recorded middle game, shown at move 10 of 20 so there is something left to play. */
function atMoveTen(): DocumentView {
  const view = midGame();
  view.tree.current = 10;
  return view;
}

async function start(page: Page, commands: MockCommand[], view: DocumentView = midGame()) {
  const script: MockScript = {
    initialEvents: [documentChanged(view, 3)],
    commands: [{ command: "settings", result: { version: 1, engine: ENGINE, pondering: false } }, ...commands],
  };
  await page.addInitScript((initial) => {
    window.__mockInitialScript = initial;
  }, script);
  await page.goto("/");
  await expect(page.getByTestId("board")).toHaveAttribute("data-ready", "true");
}

function calls(page: Page) {
  return page.evaluate(() => window.__mockBackend?.recordedCalls() ?? []);
}

const row = (page: Page, id: string) => page.locator(`.menu-row[data-menu-id="${id}"]`);
const menu = (page: Page, id: string) => page.getByTestId("menubar").locator(`[data-menu-id="${id}"]`).click();

test("搜索功能 (Ctrl+K) browses Java's groups, finds an entry in Chinese or English and runs it", async ({ page }) => {
  await start(page, []);
  await page.keyboard.press("Control+K");
  const search = page.getByTestId("function-search");
  const input = page.getByTestId("search-input");
  await expect(input).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(search).toBeHidden();

  await page.getByTestId("tool-search").click();
  await page.getByTestId("search-category-file").click();
  await expect(search).toHaveScreenshot("search-browse.png");
  await input.fill("control panel");
  await expect(page.getByRole("option", { selected: true })).toHaveAttribute("data-entry", "ctrlPanel");
  await input.fill("播放");
  await expect(search).toHaveScreenshot("search-results.png");
  await input.fill("自动播放");
  await expect(page.getByRole("option", { selected: true })).toHaveAttribute("data-entry", "makeAutoPlay");
  await page.keyboard.press("Enter");
  await expect(search).toBeHidden();
  await expect(page.getByTestId("autoplay-dialog")).toBeVisible();
  expect((await calls(page)).map((call) => call.command)).toEqual(["settings"]);
});

test("自由涂鸦 draws over the board with the left button only; Esc throws the drawing away and the pen's colour is kept", async ({ page }) => {
  await start(page, [{ command: "setDisplaySettings", result: saved({ painting_color: "Blue" }) }]);
  await page.getByTestId("tool-markup").click();
  await page.getByTestId("tool-paint").click();
  const overlay = page.getByTestId("paint-overlay");
  const box = (await page.getByTestId("board-area").boundingBox())!;
  const x = box.x + box.width / 2;
  const y = box.y + box.height / 2;
  const stroke = async (button: "left" | "right", points: [number, number][]) => {
    await page.mouse.move(x + points[0]![0], y + points[0]![1]);
    await page.mouse.down({ button });
    for (const [dx, dy] of points.slice(1)) await page.mouse.move(x + dx, y + dy, { steps: 3 });
    await page.mouse.up({ button });
  };
  await stroke("left", [[-150, -80], [-40, 60], [90, -110]]);
  await stroke("right", [[-150, 120], [150, 120]]);
  await expect(overlay).toHaveAttribute("data-strokes", "1");
  await page.getByTestId("paint-blue").click();
  await stroke("left", [[-160, 150], [-60, 170], [60, 140], [170, 160]]);
  await expect(overlay).toHaveAttribute("data-strokes", "2");
  await expect(page.getByTestId("board-area")).toHaveScreenshot("painting.png");

  await page.keyboard.press("Escape");
  await expect(overlay).toBeHidden();
  // Esc was the sheet's: the markup tools are still out.
  await expect(page.getByTestId("tool-paint")).toBeVisible();
  expect(await calls(page)).toEqual([
    { command: "settings", args: [] },
    { command: "setDisplaySettings", args: [saved({ painting_color: "Blue" })] },
  ]);
});

test("独立棋局控制面板 opens from 显示 → 面板, steps the game, and is saved where it was dragged", async ({ page }) => {
  const moved = { x: 190, y: 80, width: 500, height: 460 };
  await start(page, [{ command: "step", result: 4 }, { command: "step", result: 5 }, { command: "setDisplaySettings", result: saved({ control_panel_bounds: moved }) }], atMoveTen());
  await menu(page, "viewMenu");
  await row(page, "panel").hover();
  await row(page, "ctrlPanel").click();
  const panel = page.getByTestId("control-panel");
  await expect(page.getByTestId("tool-control-panel")).toHaveAttribute("aria-pressed", "true");
  await expect(panel).toHaveScreenshot("control-panel.png");

  await page.getByTestId("control-forward").click();
  await page.getByTestId("control-forward5").click();
  // Java opens it at 500 × 460 in the middle of the window.
  await expect(panel).toHaveCSS("left", "465px");
  await expect(panel).toHaveCSS("top", "241px");
  const title = (await page.getByTestId("control-panel-title").boundingBox())!;
  await page.mouse.move(title.x + 80, title.y + 10);
  await page.mouse.down();
  await page.mouse.move(title.x + 80 - 275, title.y + 10 - 161, { steps: 3 });
  await page.mouse.up();
  await expect(panel).toHaveCSS("left", "190px");
  await expect(panel).toHaveCSS("top", "80px");
  await expect.poll(() => calls(page)).toEqual([
    { command: "settings", args: [] },
    { command: "step", args: [1] },
    { command: "step", args: [5] },
    { command: "setDisplaySettings", args: [saved({ control_panel_bounds: moved })] },
  ]);
  await page.getByTestId("tool-control-panel").click();
  await expect(panel).toBeHidden();
});

const BOARD = () => ({ width: 19, height: 19, stones: midGame().board!.stones });
const AT = Date.UTC(2026, 10, 14, 8, 30);
const SLOTS: SaveSlot[] = [
  { id: "autosave", name: "名局研究.sgf", saved_at_unix_ms: AT, node_path: [], move_number: 20, board: BOARD(), is_autosave: true, corrupt: false },
  { id: "slot-1", name: "定式研究", saved_at_unix_ms: AT - 86_400_000, node_path: [], move_number: 20, board: BOARD(), is_autosave: false, corrupt: false },
  { id: "slot-2", name: "中盘战斗", saved_at_unix_ms: AT - 3_600_000, node_path: [], move_number: 20, board: BOARD(), is_autosave: false, corrupt: false },
  { id: "slot-3", name: "slot-3", saved_at_unix_ms: 0, node_path: [], move_number: 0, board: null, is_autosave: false, corrupt: true },
];

test("存档与读档 lies over the list and the tree: slots load, an overwrite asks first, a damaged slot only deletes", async ({ page }) => {
  await start(page, [
    { command: "saveSlots", result: SLOTS },
    { command: "loadSlot", result: 9 },
    { command: "overwriteSlot", result: SLOTS[2]! },
    { command: "saveSlots", result: SLOTS },
  ]);
  await menu(page, "fileMenu");
  await row(page, "saveAndLoad").click();
  const panel = page.getByTestId("save-panel");
  const slot = (id: string) => panel.locator(`[data-slot-id="${id}"]`);
  await expect(panel.getByTestId("save-slot")).toHaveCount(4);
  await expect(slot("slot-3").getByRole("button")).toHaveText(["删除"]);
  await expect(slot("autosave").getByRole("button")).toHaveText(["加载"]);
  await expect(page.getByTestId("main-cover")).toHaveScreenshot("save-panel.png");

  await slot("slot-1").getByTestId("save-slot-load").click();
  await slot("slot-2").getByTestId("save-slot-overwrite").click();
  await expect(page.getByTestId("save-confirm")).toContainText("存档已存在,是否覆盖?");
  await page.getByTestId("save-confirm-ok").click();
  await expect.poll(() => calls(page)).toEqual([
    { command: "settings", args: [] },
    { command: "saveSlots", args: [] },
    { command: "loadSlot", args: ["slot-1"] },
    { command: "overwriteSlot", args: ["slot-2"] },
    { command: "saveSlots", args: [] },
  ]);
  await expect(page.getByTestId("tool-save-load")).toHaveAttribute("aria-pressed", "true");
  await page.getByTestId("save-close").click();
  await expect(panel).toBeHidden();
});

test("自动播放 asks for the interval, plays on a fake clock with its button pressed, and stops on a second click", async ({ page }) => {
  await start(page, [{ command: "setDisplaySettings", result: saved({ autoplay_interval_ms: 500 }) }, { command: "step", result: 4 }], atMoveTen());
  const button = page.getByTestId("bottom-auto-play");
  await button.click();
  await expect(page.getByTestId("autoplay-dialog")).toHaveScreenshot("autoplay-dialog.png");

  await page.clock.install();
  await page.getByTestId("autoplay-interval").fill("0.5");
  await page.getByTestId("autoplay-ok").click();
  await expect(button).toHaveAttribute("aria-pressed", "true");
  await page.clock.runFor(500);
  await expect.poll(async () => (await calls(page)).map((call) => call.command)).toEqual(["settings", "setDisplaySettings", "step"]);
  await button.click();
  await expect(button).toHaveAttribute("aria-pressed", "false");
  await page.clock.runFor(3000);
  // Ctrl+A stops it too; the same interval is not saved again.
  await button.click();
  await page.getByTestId("autoplay-ok").click();
  await expect(button).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press("Control+A");
  await expect(button).toHaveAttribute("aria-pressed", "false");
  expect(await calls(page)).toEqual([
    { command: "settings", args: [] },
    { command: "setDisplaySettings", args: [saved({ autoplay_interval_ms: 500 })] },
    { command: "step", args: [1] },
  ]);
});
