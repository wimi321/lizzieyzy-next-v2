import { expect, test, type Page } from "@playwright/test";
import type { DisplaySettings, FloatingBoardsView, VariationPreviewView } from "../src/api/backend";
import { analysisUpdated, defaultDisplaySettings, documentChanged, midGame, midGameAnalysis } from "./fixtures";
import type { MockCommand, MockScript } from "./mock-backend";

test.use({ deviceScaleFactor: 1, locale: "zh-CN" });

type Display = Required<DisplaySettings>;
const saved = (change: Partial<Display> = {}): Display => ({ ...defaultDisplaySettings, ...change });
const ENGINE = { program: "/opt/katago", model: "/opt/model.bin.gz", config: "", extra_args: [] };
const ANALYSIS = midGameAnalysis(3, 20);
const BOARDS = (change: FloatingBoardsView = {}): Required<FloatingBoardsView> => ({ main: false, keep_main_board: false, sub: false, main_on_top: false, sub_on_top: false, main_locked: false, sub_locked: false, ...change });
const REPLAY: VariationPreviewView = { revision: 3, node_id: 20, board: midGame().board!, applied_moves: [], stones: [], stop_reason: null };

async function open(page: Page, path: string, display: Partial<Display>, commands: MockCommand[]) {
  const script: MockScript = {
    initialEvents: [documentChanged(midGame(), 3), analysisUpdated(ANALYSIS)],
    commands: [{ command: "settings", result: { version: 1, engine: ENGINE, pondering: false, display: saved(display) } }, ...commands],
  };
  await page.addInitScript((initial) => {
    window.__mockInitialScript = initial;
  }, script);
  await page.goto(path);
}

function calls(page: Page) {
  return page.evaluate(() => window.__mockBackend?.recordedCalls() ?? []);
}

const row = (page: Page, id: string) => page.locator(`.menu-row[data-menu-id="${id}"]`);

test.describe("a floating board's own window", () => {
  test.use({ viewport: { width: 600, height: 600 } });

  test("浮动主棋盘 fills its window; the pointer brings up 关闭 / 置顶 / 锁定; a drag moves the window and plays nothing", async ({ page }) => {
    await open(page, "/?window=float-main", { floating_boards: BOARDS({ main: true }) }, [
      { command: "setFloatingBoardOptions", result: BOARDS({ main: true, main_on_top: true }) },
      { command: "setFloatingBoardOptions", result: BOARDS({ main: true, main_on_top: true, main_locked: true }) },
      { command: "closeFloatingBoard", result: BOARDS() },
    ]);
    const board = page.getByTestId("board");
    await expect(board).toHaveAttribute("data-ready", "true");
    await expect(page.getByTestId("analysis-move")).toHaveCount(0);
    await page.mouse.move(300, 300);
    await expect(page.locator(".floating-buttons")).toHaveCSS("opacity", "1");
    await expect(page).toHaveScreenshot("floating-main.png");

    await page.mouse.down();
    await page.mouse.move(340, 320, { steps: 4 });
    await page.mouse.up();
    await page.getByTestId("floating-pin").click();
    await expect(page.getByTestId("floating-pin")).toHaveAttribute("aria-pressed", "true");
    await page.getByTestId("floating-lock").click();
    await expect(page.locator(".floating-edge")).toHaveCount(0);
    await page.getByTestId("floating-close").click();
    await expect.poll(async () => (await calls(page)).map((call) => [call.command, ...call.args])).toEqual([
      ["settings"],
      ["dragWindow"],
      ["setFloatingBoardOptions", "Main", true, false],
      ["setFloatingBoardOptions", "Main", true, true],
      ["closeFloatingBoard", "Main"],
    ]);
  });

  test("浮动小棋盘 shows the best line; the wheel takes it back a move", async ({ page }) => {
    await open(page, "/?window=float-sub", { floating_boards: BOARDS({ sub: true }) }, [
      { command: "previewVariation", result: REPLAY },
      { command: "previewVariation", result: REPLAY },
    ]);
    const content = page.locator(".floating-content");
    const best = [...ANALYSIS.candidates].sort((a, b) => a.order - b.order)[0]!;
    await expect(content).toHaveAttribute("data-line-length", String(best.pv.length));
    await content.hover();
    await page.mouse.wheel(0, -40);
    await expect(content).toHaveAttribute("data-line-length", String(best.pv.length - 1));
    const lines = (await calls(page)).filter((call) => call.command === "previewVariation").map((call) => call.args[1]);
    expect(lines).toEqual([best.pv, best.pv.slice(0, -1)]);
  });
});

test.describe("the main window", () => {
  test.use({ viewport: { width: 1000, height: 640 } });

  test("显示 → 面板 → 浮动主棋盘 leaves the board out of the main window; a board closed from its window comes back; Alt+7 floats both", async ({ page }) => {
    await open(page, "/", {}, [
      { command: "setFloatingLayout", result: BOARDS({ main: true }) },
      { command: "setFloatingLayout", result: BOARDS({ main: true, sub: true }) },
      { command: "fitMainWindow", result: undefined },
    ]);
    await expect(page.locator('[data-region="board"] [data-testid="board"]')).toHaveAttribute("data-ready", "true");
    await page.getByTestId("menubar").locator('[data-menu-id="viewMenu"]').click();
    await row(page, "panel").hover();
    await row(page, "independentMainBoard").click();
    await expect(page.locator('[data-region="board"]')).toHaveCount(0);
    await expect(page.getByTestId("main-panel")).toHaveScreenshot("board-floated.png");

    // The floating window's own ✕: Rust says so, and the board is back.
    await page.evaluate((view) => window.__mockBackend?.pushEvents([{ type: "FloatingBoardsChanged", data: view }]), BOARDS());
    await expect(page.locator('[data-region="board"]')).toHaveCount(1);
    // The menu strip keeps the focus, and its keys, after a choice; the shortcut needs it back.
    await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
    await page.keyboard.press("Alt+7");
    await expect(page.locator('[data-region="sub-board"]')).toHaveCount(0);
    const recorded = await calls(page);
    expect(recorded.map((call) => [call.command, ...(call.command === "fitMainWindow" ? [] : call.args)])).toEqual([
      ["settings"],
      ["setFloatingLayout", true, false, false],
      ["setFloatingLayout", true, false, true],
      ["fitMainWindow"],
    ]);
  });

  test("可视化面板设置 floats the sub-board at once and greys 显示小棋盘; 取消 puts it back", async ({ page }) => {
    await open(page, "/", {}, [
      { command: "setFloatingLayout", result: BOARDS({ sub: true }) },
      { command: "setDisplaySettings", result: saved() },
      { command: "setFloatingLayout", result: BOARDS() },
    ]);
    await page.getByTestId("menubar").locator('[data-menu-id="viewMenu"]').click();
    await row(page, "panel").hover();
    await row(page, "visualizedPanelSettings").click();
    await page.getByTestId("panel-float-sub").check();
    await expect(page.getByTestId("panel-show_sub_board")).toBeDisabled();
    await expect(page.locator('[data-region="sub-board"]')).toHaveCount(0);
    await expect(page.getByTestId("panel-settings")).toHaveScreenshot("panel-settings-floating.png");
    await page.keyboard.press("Escape");
    await expect(page.locator('[data-region="sub-board"]')).toHaveCount(1);
    await expect.poll(async () => (await calls(page)).map((call) => call.command)).toEqual(["settings", "setFloatingLayout", "setDisplaySettings", "setFloatingLayout"]);
    expect((await calls(page))[3]!.args).toEqual([false, false, false]);
  });
});
