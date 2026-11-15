import { expect, test, type Page } from "@playwright/test";
import type { CandidateView, DisplaySettings, VariationPreviewView } from "../src/api/backend";
import { analysisUpdated, boardView, defaultDisplaySettings, documentChanged, MID_GAME_STONES, midGame, midGameAnalysis } from "./fixtures";
import type { MockCommand, MockScript } from "./mock-backend";

// Small enough that a whole main panel stays under the file budget, and near Java's window shares.
test.use({ viewport: { width: 1000, height: 640 }, deviceScaleFactor: 1, locale: "zh-CN" });

type Display = Required<DisplaySettings>;
type Stone = (typeof MID_GAME_STONES)[number];
const saved = (change: Partial<Display> = {}): Display => ({ ...defaultDisplaySettings, ...change });
const save = (change: Partial<Display>): MockCommand => ({ command: "setDisplaySettings", result: saved(change) });
const ENGINE = { program: "/opt/katago", model: "/opt/model.bin.gz", config: "", extra_args: [] };
const ANALYSIS = midGameAnalysis(3, 20);

/** Rust's replay of a candidate's line, stones numbered from the first move on. */
function replay(candidate: CandidateView): VariationPreviewView {
  const colors = ["Black", "White"] as const;
  const played: Stone[] = candidate.pv.flatMap((move, index) => (move.type === "Play" ? [[move.data.point.x, move.data.point.y, colors[index % 2]!] as Stone] : []));
  const last = played[played.length - 1]!;
  return {
    revision: 3,
    node_id: 20,
    board: boardView(19, 19, [...MID_GAME_STONES, ...played], { toPlay: colors[played.length % 2]!, moveNumber: 20 + played.length, last }),
    applied_moves: candidate.pv,
    stones: played.map(([x, y, color], index) => ({ point: { x, y }, color, move_number: index + 1 })),
    stop_reason: null,
  };
}
const LINES = [...ANALYSIS.candidates].sort((a, b) => a.order - b.order).slice(0, 4).map((candidate): MockCommand => ({ command: "previewVariation", result: replay(candidate) }));

async function start(page: Page, display: Partial<Display>, commands: MockCommand[]) {
  const script: MockScript = {
    initialEvents: [documentChanged(midGame(), 3), analysisUpdated(ANALYSIS)],
    commands: [{ command: "settings", result: { version: 1, engine: ENGINE, pondering: false, display: saved(display) } }, ...commands],
  };
  await page.addInitScript((initial) => {
    window.__mockInitialScript = initial;
  }, script);
  await page.goto("/");
  await expect(page.locator('[data-region="board"] [data-testid="board"]')).toHaveAttribute("data-ready", "true");
}

function calls(page: Page) {
  return page.evaluate(() => window.__mockBackend?.recordedCalls() ?? []);
}

const row = (page: Page, id: string) => page.locator(`.menu-row[data-menu-id="${id}"]`);
const regions = (page: Page) => page.locator(".main-region").evaluateAll((found) => found.map((region) => region.getAttribute("data-region")));

/** The window content's height above the bottom toolbar, as Java's sizes count it. */
const javaHeight = (page: Page) => page.evaluate(() => window.innerHeight - (document.querySelector<HTMLElement>('[data-testid="bottom-toolbar"]')?.offsetHeight ?? 0));

test("思考模式 from 显示 → 布局模式: a plain board on the left, the window resized as Java does", async ({ page }) => {
  await start(page, {}, [save({ layout_mode: "Thinking", show_comment: false }), { command: "fitMainWindow", result: undefined }]);
  const height = await javaHeight(page);
  await page.getByTestId("menubar").locator('[data-menu-id="viewMenu"]').click();
  await row(page, "layoutModeMenu").hover();
  await row(page, "extraMode3").click();
  const thinking = page.getByTestId("thinking-board");
  await expect(thinking).toBeVisible();
  await expect(thinking.getByTestId("board")).toHaveAttribute("data-ready", "true");
  await expect(page.locator('[data-region="sub-board"]')).toHaveCount(0);
  await expect(page.getByTestId("main-panel")).toHaveScreenshot("thinking.png");
  const recorded = await calls(page);
  expect(recorded.map((call) => call.command)).toEqual(["settings", "setDisplaySettings", "fitMainWindow"]);
  expect(recorded[1]!.args[0]).toMatchObject({ layout_mode: "Thinking", show_comment: false, show_list_pane: true });
  expect(recorded[2]!.args).toEqual([Math.round(height * 1.66), 640]);
});

test("四方图 (Alt+5) shows the first four candidates' lines; Alt+5 again goes back to the default layout", async ({ page }) => {
  const fit: MockCommand = { command: "fitMainWindow", result: undefined };
  await start(page, {}, [save({ layout_mode: "MultiView", show_comment: false }), fit, ...LINES, save({}), fit]);
  // The analysis is on show (and the sub-board's own line asked for) before the four boards appear.
  await expect(page.getByTestId("analysis-move")).toHaveAttribute("data-node-id", "20");
  await page.keyboard.press("Alt+5");
  const view = page.getByTestId("multi-view");
  await expect(view).toHaveAttribute("data-lines", "4");
  await expect.poll(async () => (await calls(page)).filter((call) => call.command === "previewVariation").length).toBe(4);
  await expect(page.getByTestId("main-panel")).toHaveScreenshot("multi-view.png");
  const moves = (await calls(page)).filter((call) => call.command === "previewVariation").map((call) => call.args[1]);
  expect(moves).toEqual([...ANALYSIS.candidates].sort((a, b) => a.order - b.order).slice(0, 4).map((candidate) => candidate.pv));

  await page.keyboard.press("Alt+5");
  await expect(view).toHaveCount(0);
  await expect.poll(async () => (await calls(page)).at(-2)?.args[0]).toMatchObject({ layout_mode: "Normal", show_comment: true });
});

test("精简模式 (Alt+3) leaves the board alone; opening a panel goes back to the default layout", async ({ page }) => {
  await start(page, {}, [save({ layout_mode: "Min" }), { command: "fitMainWindow", result: undefined }, save({})]);
  await page.keyboard.press("Alt+3");
  await expect.poll(() => regions(page)).toEqual(["board"]);
  await expect(page.getByTestId("main-panel")).toHaveScreenshot("min.png");
  await page.getByTestId("menubar").locator('[data-menu-id="viewMenu"]').click();
  await row(page, "panel").hover();
  await row(page, "commitPane").click();
  await expect.poll(async () => (await calls(page)).at(-1)?.args[0]).toMatchObject({ layout_mode: "Normal" });
});

test("放大小棋盘 (Ctrl+F) and 放大胜率图 (Ctrl+W) exclude each other; [ and ] move the board; 重置界面位置 asks Rust", async ({ page }) => {
  await start(page, {}, [save({ large_sub_board: true }), save({ large_winrate_graph: true }), save({ large_winrate_graph: false }), save({}), { command: "resetWindowPositions", result: undefined }]);
  const panel = page.getByTestId("main-panel");
  const board = page.locator('[data-region="board"]');
  await page.keyboard.press("Control+F");
  // The board goes to the right edge.
  await expect.poll(async () => {
    const box = await board.boundingBox();
    return box ? box.x + box.width : 0;
  }).toBeGreaterThan(960);
  await expect(panel).toHaveScreenshot("large-sub-board.png");
  await page.keyboard.press("Control+W");
  await expect(page.locator('[data-region="list"]')).toHaveCount(0);
  await expect(panel).toHaveScreenshot("large-graph.png");
  await page.keyboard.press("Control+W");
  await page.keyboard.press("BracketLeft");
  await page.getByTestId("menubar").locator('[data-menu-id="viewMenu"]').click();
  await row(page, "deletePersistFile").click();
  await expect.poll(async () => (await calls(page)).map((call) => call.command)).toEqual([
    "settings", "setDisplaySettings", "setDisplaySettings", "setDisplaySettings", "setDisplaySettings", "resetWindowPositions",
  ]);
  const sent = (await calls(page)).slice(1, 5).map((call) => call.args[0] as Display);
  expect(sent.map((display) => [display.large_sub_board, display.large_winrate_graph, display.show_list_pane, display.board_position])).toEqual([
    [true, false, true, 4],
    [false, true, false, 4],
    [false, false, false, 4],
    [false, false, false, 3],
  ]);
});

test("可视化面板设置 previews at once and 取消 puts it back; 自定义布局 1 saves the window as it is and Alt+8 brings it back", async ({ page }) => {
  const placed = { bounds: { x: 30, y: 40, width: 1000, height: 700 }, maximized: false };
  await start(page, {}, [
    save({ show_comment: false }),
    save({}),
    save({ show_status_bar: false }),
    { command: "settings", result: { version: 1, engine: ENGINE, pondering: false, display: saved({ show_status_bar: false, main_window: placed }) } },
    save({ show_status_bar: false }),
    save({}),
    { command: "applyWindowLayout", result: saved({ main_window: placed }) },
  ]);
  await page.getByTestId("menubar").locator('[data-menu-id="viewMenu"]').click();
  await row(page, "panel").hover();
  await row(page, "visualizedPanelSettings").click();
  const dialog = page.getByTestId("panel-settings");
  await page.getByTestId("panel-show_comment").uncheck();
  await expect(page.locator('[data-region="comment"]')).toHaveCount(0);
  await expect(dialog).toHaveScreenshot("panel-settings.png");
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
  await expect(page.locator('[data-region="comment"]')).toHaveCount(1);

  await page.getByTestId("menubar").locator('[data-menu-id="viewMenu"]').click();
  await page.getByTestId("menu-aside-customeMode1").click();
  await expect(dialog).toContainText("设置自定义布局 1");
  await page.getByTestId("panel-show_status_bar").uncheck();
  await page.getByTestId("panel-settings-apply").click();
  await expect(dialog).toBeHidden();
  await expect.poll(async () => (await calls(page)).length).toBe(6);
  const slot = ((await calls(page))[5]!.args[0] as Display).custom_layouts[0];
  expect(slot).toMatchObject({ main_window: placed, panels: { show_status_bar: false, show_comment: true } });

  // Alt+8: the slot's panels again, and its window place through Rust.
  await page.keyboard.press("Alt+8");
  await expect.poll(async () => (await calls(page)).map((call) => call.command).slice(6)).toEqual(["setDisplaySettings", "applyWindowLayout"]);
  expect((await calls(page))[7]!.args[0]).toMatchObject({ main_window: placed });
});
