import { expect, test, type Page } from "@playwright/test";
import type { AnalysisView, DisplaySettings, EstimateStyle, Settings } from "../src/api/backend";
import { analysisUpdated, analysisView, defaultDisplaySettings, documentChanged, MID_GAME_STONES, midGame, midGameAnalysis, ownershipPattern } from "./fixtures";
import type { MockCommand, MockScript } from "./mock-backend";

test.use({ viewport: { width: 1280, height: 820 }, deviceScaleFactor: 1, locale: "zh-CN" });

const LIMITS = { limit_time: false, max_analyze_time_seconds: 600, limit_playout: false, limit_playouts: 100000 };

function settingsWith(display: Partial<DisplaySettings>): Settings {
  return {
    version: 1,
    engine: { program: "/opt/katago", model: "/opt/model.bin.gz", config: "", extra_args: [] },
    pondering: false,
    analysis_limits: LIMITS,
    display: { ...defaultDisplaySettings, ...display },
  } as unknown as Settings;
}

async function start(page: Page, settings: Settings, analysis: AnalysisView, commands: MockCommand[]) {
  const script: MockScript = {
    initialEvents: [documentChanged(midGame(), 3), analysisUpdated(analysis)],
    commands: [{ command: "settings", result: settings }, ...commands],
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

/** Opens a menu and the submenus on the way, then picks the entry. */
async function pick(page: Page, path: string[]) {
  await menu(page, path[0]!).click();
  for (const id of path.slice(1, -1)) await row(page, id).hover();
  await row(page, path.at(-1)!).click();
  await page.mouse.move(0, 0);
}

async function quarter(page: Page) {
  const box = await page.getByTestId("board").boundingBox();
  if (!box) throw new Error("the board has no box");
  return { x: box.x, y: box.y, width: box.width / 2, height: box.height / 2 };
}

/** The mid-game position's territory, without candidates over it. */
const territory = () => analysisView(3, 20, [], { ownership: ownershipPattern(19, 19) });

/** A first instinct gathered at a few points of the upper left, as the network gives one: -1 on stones, pass last. */
function policyPattern(): number[] {
  const peaks = [
    [6, 6, 1],
    [2, 8, 0.5],
    [7, 2, 0.35],
    [8, 9, 0.25],
  ] as const;
  const taken = new Set(MID_GAME_STONES.map(([x, y]) => y * 19 + x));
  const raw = Array.from({ length: 361 }, (_, index) => {
    if (taken.has(index)) return -1;
    const x = index % 19;
    const y = Math.floor(index / 19);
    return peaks.reduce((sum, [px, py, weight]) => sum + weight * Math.exp(-((x - px) ** 2 + (y - py) ** 2) / 1.6), 0.0004);
  });
  const total = raw.reduce((sum, value) => sum + Math.max(0, value), 0);
  return [...raw.map((value) => (value < 0 ? -1 : value / total)), 0.001];
}

const STYLES: [string, EstimateStyle][] = [
  ["kataEstimateByTransparentSmall", "TransparentSmall"],
  ["kataEstimateByTransparent", "Transparent"],
  ["kataEstimateByTransparentNotOnLive", "TransparentOffStones"],
  ["kataEstimateByBigSquare", "BigSquare"],
  ["kataEstimateBySize", "BySize"],
];

test("显示 → Kata形势显示 draws each of Java's five styles, and saves the choice", async ({ page }) => {
  const display = { ...defaultDisplaySettings, estimate_location: "Main" } as const;
  await start(
    page,
    settingsWith(display),
    territory(),
    STYLES.map(([, style]) => ({ command: "setDisplaySettings", result: { ...display, estimate_style: style } })),
  );
  const board = page.getByTestId("board");
  for (const [id, style] of STYLES) {
    await pick(page, ["viewMenu", "kataSettings", "kataEstimate", id]);
    await expect(board).toHaveAttribute("data-estimate", style);
    await expect(page).toHaveScreenshot(`estimate-${style}.png`, { clip: await quarter(page) });
  }
  expect((await calls(page)).map((call) => (call.args[0] as DisplaySettings).estimate_style)).toEqual(STYLES.map(([, style]) => style));
});

test("分析 → 纯网络 (H) shows the network's first instinct and its own estimate; H again goes back", async ({ page }) => {
  const ordinary = midGameAnalysis(3, 20, 1, true);
  const raw: AnalysisView = { ...ordinary, policy: policyPattern() };
  await start(page, settingsWith({ estimate_location: "Main" }), ordinary, [
    { command: "setPureNetwork", result: undefined, events: [analysisUpdated(raw)] },
    { command: "setPureNetwork", result: undefined, events: [analysisUpdated(ordinary)] },
  ]);
  const board = page.getByTestId("board");
  await pick(page, ["analyzeMenu", "showHeatmap"]);
  await expect(board).toHaveAttribute("data-policy", "true");
  // 在纯网络中显示 defaults to big squares under the stones; the candidates give way to the policy.
  await expect(board).toHaveAttribute("data-estimate", "BigSquare");
  await expect(page).toHaveScreenshot("pure-network.png", { clip: await quarter(page) });

  await page.keyboard.press("KeyH");
  await expect(board).not.toHaveAttribute("data-policy", "true");
  await expect(board).toHaveAttribute("data-estimate", "TransparentSmall");
  expect((await calls(page)).map((call) => [call.command, call.args])).toEqual([
    ["setPureNetwork", [true]],
    ["setPureNetwork", [false]],
  ]);
});

test("显示 → 小棋盘设置 → 第一感热点图 spreads the candidates' first instinct over the sub-board", async ({ page }) => {
  const analysis = midGameAnalysis(3, 20);
  const priors = [0.42, 0.21, 0.12, 0.08, 0.04, 0.02];
  analysis.candidates = analysis.candidates.map((candidate, index) => ({ ...candidate, prior: priors[index] ?? 0.01 }));
  await start(page, settingsWith({}), analysis, [
    { command: "setDisplaySettings", result: { ...defaultDisplaySettings, sub_board_heat: "Policy" } },
    { command: "setDisplaySettings", result: { ...defaultDisplaySettings, sub_board_heat: "None", sub_board_raw: true } },
  ]);
  const sub = page.getByTestId("sub-board");
  await pick(page, ["viewMenu", "subBoardSettings", "heatMapSettings", "showHeat"]);
  await expect(sub).toHaveAttribute("data-heat", "Policy");
  await expect(sub).toHaveScreenshot("sub-board-heat.png");

  await pick(page, ["viewMenu", "subBoardSettings", "subBoardShowRaw"]);
  await expect(sub).toHaveAttribute("data-heat", "");
  await expect(sub).toHaveAttribute("data-moves", "0");
});

test("显示 → 工具栏: groups can be hidden or added (PDA alone, Java #709), and the toolbar can join the menu bar", async ({ page }) => {
  const custom = { ...defaultDisplaySettings, top_toolbar_hidden: ["showPdaInToolbar", "showTimeLimitController", "showGameController"] };
  await start(page, settingsWith({}), midGameAnalysis(3, 20), [
    { command: "setDisplaySettings", result: { ...defaultDisplaySettings, top_toolbar_hidden: ["showPdaInToolbar"] } },
    { command: "setDisplaySettings", result: { ...defaultDisplaySettings, top_toolbar_hidden: ["showPdaInToolbar", "showTimeLimitController"] } },
    { command: "setDisplaySettings", result: custom },
    { command: "setAnalysisLimits", result: { ...LIMITS, limit_time: true, max_analyze_time_seconds: 30 } },
    { command: "setDisplaySettings", result: { ...custom, top_toolbar_layout: "Combined" } },
  ]);
  const toolbar = page.getByTestId("top-toolbar");
  await expect(toolbar.getByTestId("tool-pda")).toBeVisible();
  await pick(page, ["viewMenu", "toolBar", "topToolBar", "customTopToolBar", "showPdaInToolbar"]);
  await pick(page, ["viewMenu", "toolBar", "topToolBar", "customTopToolBar", "showTimeLimitController"]);
  await pick(page, ["viewMenu", "toolBar", "topToolBar", "customTopToolBar", "showGameController"]);
  await expect(toolbar.getByTestId("tool-pda")).toHaveCount(0);
  await expect(toolbar.getByTestId("tool-wrn")).toBeVisible();
  const time = toolbar.getByTestId("tool-time-limit").getByRole("textbox");
  await expect(time).toHaveValue("600");
  await expect(toolbar).toHaveScreenshot("custom-top-toolbar.png");

  await time.fill("30");
  await time.press("Enter");
  await expect(time).toHaveValue("30");

  await pick(page, ["viewMenu", "toolBar", "topToolBar", "topToolBarCombined"]);
  await expect(toolbar).toHaveAttribute("data-layout", "Combined");
  await expect(page.locator(".menubar-actions").getByTestId("top-toolbar")).toHaveCount(1);
  await expect(page).toHaveScreenshot("combined-top-toolbar.png", { clip: { x: 0, y: 0, width: 1280, height: 40 } });

  expect((await calls(page)).map((call) => call.command)).toEqual(["setDisplaySettings", "setDisplaySettings", "setDisplaySettings", "setAnalysisLimits", "setDisplaySettings"]);
  expect((await calls(page))[3]?.args).toEqual([{ ...LIMITS, limit_time: true, max_analyze_time_seconds: 30 }]);
});
