import { expect, test, type Page } from "@playwright/test";
import type { DocumentView, PointView, RestrictionKind, ScoringView } from "../src/api/backend";
import { documentChanged, MID_GAME_STONES, midGame } from "./fixtures";
import type { MockCommand, MockScript } from "./mock-backend";

// Java's window in the reference screenshots.
test.use({ viewport: { width: 1429, height: 941 }, deviceScaleFactor: 1, locale: "zh-CN" });

const ENGINE = { program: "/opt/katago", model: "/opt/model.bin.gz", config: "", extra_args: [] };

function restricted(kind: RestrictionKind, points: PointView[]): DocumentView {
  return { ...midGame(), analysis_restriction: { kind, points, depth: 5 } };
}

async function start(page: Page, view: DocumentView, commands: MockCommand[]) {
  const script: MockScript = {
    initialEvents: [documentChanged(view, 3)],
    commands: [{ command: "settings", result: { version: 1, engine: ENGINE, pondering: false } }, ...commands],
  };
  await page.addInitScript((initial) => {
    window.__mockInitialScript = initial;
  }, script);
  await page.goto("/");
  const board = page.getByTestId("board");
  await expect(board).toHaveAttribute("data-ready", "true");
  await expect(board).not.toHaveAttribute("data-cell", "0.000");
  return board;
}

/** Where point (x, y) is on the page, fractions allowed. */
async function pointer(page: Page) {
  const board = page.getByTestId("board");
  const box = (await board.boundingBox())!;
  const read = async (name: string) => Number(await board.getAttribute(name));
  const [originX, originY, cell] = [await read("data-origin-x"), await read("data-origin-y"), await read("data-cell")];
  return {
    at: (x: number, y: number): [number, number] => [box.x + originX + x * cell, box.y + originY + y * cell],
    /** The board's top rows: enough to see the marks, small enough for the file budget. */
    rows: (rows: number) => ({ x: box.x, y: box.y, width: box.width, height: originY + (rows - 0.5) * cell }),
  };
}

async function drag(page: Page, from: [number, number], to: [number, number]) {
  await page.mouse.move(...from);
  await page.mouse.down();
  await page.mouse.move(...to, { steps: 4 });
}

function calls(page: Page) {
  return page.evaluate(() => window.__mockBackend?.recordedCalls() ?? []);
}

const SETTINGS = { command: "settings", args: [] };
const row = (page: Page, id: string) => page.locator(`.menu-row[data-menu-id="${id}"]`);

test("选点限制: Alt+drag frames points over the ones already restricted; the tray chip keeps it in sight and Alt+click clears", async ({ page }) => {
  const framed: PointView[] = [3, 4].flatMap((y) => [3, 4, 5].map((x) => ({ x, y })));
  const board = await start(page, restricted("Avoid", [{ x: 12, y: 4 }, { x: 13, y: 4 }, { x: 12, y: 5 }]), [
    { command: "addRestrictionPoints", result: { kind: "Allow", points: framed, depth: 5 }, events: [documentChanged(restricted("Allow", framed), 4)] },
    { command: "clearAnalysisRestriction", result: undefined, events: [documentChanged(midGame(), 5)] },
  ]);
  const chip = page.getByTestId("restriction-chip");
  await expect(board).toHaveAttribute("data-restricted", "Avoid:3");
  await expect(chip).toContainText("不分析 3 点");
  const { at, rows } = await pointer(page);

  await page.keyboard.down("Alt");
  await drag(page, at(2.6, 2.6), at(5.4, 4.4));
  await expect(board).toHaveAttribute("data-dragging", "Allow");
  await expect(page).toHaveScreenshot("region-drag.png", { clip: rows(7) });
  await page.mouse.up();
  await expect(board).toHaveAttribute("data-restricted", "Allow:6");
  await expect(chip).toContainText("只分析 6 点");

  await page.mouse.click(...at(10, 12));
  await page.keyboard.up("Alt");
  await expect(chip).toHaveCount(0);
  await expect(board).not.toHaveAttribute("data-restricted");
  expect(await calls(page)).toEqual([SETTINGS, { command: "addRestrictionPoints", args: ["Allow", framed] }, { command: "clearAnalysisRestriction", args: [] }]);
});

test("the toolbar arms one rectangle; the depth menu and the right-click entries reach Rust", async ({ page }) => {
  const two = [{ x: 15, y: 15 }, { x: 16, y: 15 }];
  const corner = [{ x: 0, y: 0 }];
  const board = await start(page, midGame(), [
    { command: "addRestrictionPoints", result: { kind: "Avoid", points: two, depth: 5 }, events: [documentChanged(restricted("Avoid", two), 4)] },
    { command: "setRestrictionDepth", result: undefined },
    { command: "setAnalysisRestriction", result: { kind: "Allow", points: corner, depth: 5 }, events: [documentChanged(restricted("Allow", corner), 5)] },
  ]);
  const { at } = await pointer(page);
  await page.getByTestId("tool-select-avoid").click();
  await expect(board).toHaveAttribute("data-selecting", "Avoid");
  await drag(page, at(14.6, 14.6), at(16.4, 15.4));
  await page.mouse.up();
  await expect(board).not.toHaveAttribute("data-selecting");
  await expect(board).toHaveAttribute("data-restricted", "Avoid:2");

  await page.getByTestId("tool-avoid-depth").click();
  const menu = page.getByTestId("avoid-depth-menu");
  await expect(menu).toHaveScreenshot("depth-menu.png");
  await page.getByTestId("depth-first").click();
  await expect(menu).toBeHidden();

  await page.mouse.click(...at(0, 0), { button: "right" });
  await page.getByTestId("board-menu").locator('[data-menu-id="allow-point"]').click();
  await expect(page.getByTestId("restriction-chip")).toContainText("只分析 1 点");
  expect(await calls(page)).toEqual([
    SETTINGS,
    { command: "addRestrictionPoints", args: ["Avoid", two] },
    { command: "setRestrictionDepth", args: ["Avoid", { type: "FirstMove" }] },
    { command: "setAnalysisRestriction", args: ["Allow", corner] },
  ]);
});

test("引擎参数 (Alt+D) saves PDA, WRN and the temperature; the toolbar fields follow and change them at once", async ({ page }) => {
  const saved = { playout_doubling_advantage: 1.5, wide_root_noise: 0.2, root_policy_temperature: null };
  const unset = { ...saved, wide_root_noise: null };
  await start(page, midGame(), [
    { command: "setSearchParams", result: saved },
    { command: "setSearchParams", result: unset },
  ]);
  await page.keyboard.press("Alt+D");
  const dialog = page.getByTestId("params-dialog");
  await expect(dialog).toHaveScreenshot("params-dialog.png");
  await page.getByTestId("params-playout_doubling_advantage").fill("1.5");
  await page.getByTestId("params-wide_root_noise").fill("0.2");
  await page.getByTestId("params-ok").click();
  await expect(dialog).toBeHidden();
  await expect(page.getByTestId("tool-pda")).toHaveValue("1.5");
  await expect(page.getByTestId("tool-pda-on")).toBeChecked();
  await expect(page.getByTestId("tool-wrn")).toHaveValue("0.2");

  await page.getByTestId("tool-wrn-on").uncheck();
  await expect(page.getByTestId("tool-wrn")).toHaveValue("0.04");
  await page.getByTestId("tool-params").click();
  await expect(page.getByTestId("params-wide_root_noise")).toHaveValue("");
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
  expect(await calls(page)).toEqual([SETTINGS, { command: "setSearchParams", args: [saved] }, { command: "setSearchParams", args: [unset] }]);
});

/** Black on the left, White on the right, the middle column neutral; White's stones on the left are dead. */
function scoringView(rule: "Area" | "Territory", change: Partial<ScoringView> = {}): ScoringView {
  const ownership = Array.from({ length: 361 }, (_, index) => (index % 19 < 9 ? "Black" : index % 19 > 9 ? "White" : null)) as ScoringView["ownership"];
  const dead = MID_GAME_STONES.filter(([x, , color]) => color === "White" && x < 9).map(([x, y]) => ({ x, y }));
  const area = rule === "Area";
  return {
    node_id: 20,
    rule,
    dead_stones: dead,
    neutral_points: [],
    ownership,
    black: { living_stones: 9, territory: 72, dead_stones: 0, captures: 0, komi: 0, handicap_compensation: 0, total: area ? 81 : 75 },
    white: { living_stones: 8, territory: 75, dead_stones: dead.length, captures: 0, komi: 7.5, handicap_compensation: 0, total: area ? 90.5 : 82.5 },
    score_lead: area ? -9.5 : -7.5,
    result: area ? "W+9.5" : "W+7.5",
    ...change,
  };
}

test("终局点目 (Ctrl+Q) marks the board, a click turns a group, the rule switches, and 确定 writes the result", async ({ page }) => {
  const area = scoringView("Area");
  const toggled = scoringView("Area", { dead_stones: [...area.dead_stones, { x: 15, y: 3 }] });
  const territory = scoringView("Territory");
  const board = await start(page, midGame(), [
    { command: "startScoring", result: area, events: [documentChanged({ ...midGame(), scoring: area }, 4)] },
    { command: "toggleScoringPoint", result: toggled, events: [documentChanged({ ...midGame(), scoring: toggled }, 5)] },
    { command: "setScoringRule", result: territory, events: [documentChanged({ ...midGame(), scoring: territory }, 6)] },
    { command: "commitScoring", result: 7, events: [documentChanged(midGame(), 7)] },
  ]);
  const { at, rows } = await pointer(page);
  await page.keyboard.press("Control+Q");
  await expect(board).toHaveAttribute("data-scoring", "true");
  const panel = page.getByTestId("scoring-panel");
  await expect(panel.getByTestId("scoring-white")).toContainText("贴目 7.5");
  await expect(page).toHaveScreenshot("scoring-board.png", { clip: rows(8) });
  await expect(page.getByTestId("main-cover")).toHaveScreenshot("scoring-panel.png");

  await page.mouse.click(...at(15, 3));
  await panel.getByTestId("scoring-rule-territory").click();
  await expect(panel.getByTestId("scoring-black")).toContainText("提子");
  await expect(panel.getByTestId("scoring-result")).toHaveText("白胜 7.5 目");
  await panel.getByTestId("scoring-confirm").click();
  await expect(panel).toBeHidden();
  await expect(board).not.toHaveAttribute("data-scoring");
  expect(await calls(page)).toEqual([
    SETTINGS,
    { command: "startScoring", args: [] },
    { command: "toggleScoringPoint", args: [{ x: 15, y: 3 }] },
    { command: "setScoringRule", args: ["Territory"] },
    { command: "commitScoring", args: [] },
  ]);
});

test("自动补完征子 says what it did; 规则 and 更新官方权重 open their windows", async ({ page }) => {
  await start(page, midGame(), [{ command: "continueLadder", result: { outcome: "Captured", moves: 7 } }]);
  await page.getByTestId("menubar").locator('[data-menu-id="gameMenu"]').click();
  await row(page, "continueLadder").click();
  await expect(page.getByTestId("tools-toast")).toHaveText("征子成立，已补完 7 手，一次撤销即可还原。");

  await page.getByTestId("tool-rules").click();
  await expect(page.getByTestId("rules-dialog")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("rules-dialog")).toBeHidden();
  await page.getByTestId("tool-weights").click();
  await expect(page.getByTestId("tools-tab-weights")).toHaveAttribute("aria-selected", "true");
  expect((await calls(page)).map((call) => call.command)).toEqual(["settings", "continueLadder"]);
});
