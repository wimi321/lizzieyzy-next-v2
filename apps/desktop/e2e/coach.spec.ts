import { expect, test, type Page } from "@playwright/test";
import type { Settings } from "../src/api/backend";
import type { CoachMoveRated, CoachView } from "../src/api/coach";
import type { GameView } from "../src/api/game";
import { documentChanged, midGame } from "./fixtures";
import type { MockCommand, MockScript } from "./mock-backend";

test.use({ viewport: { width: 1280, height: 820 }, deviceScaleFactor: 1, locale: "zh-CN" });

const B18 = { program: "/opt/katago", model: "/opt/weights/kata1-b18c384nbt.bin.gz", config: "", extra_args: [] };
const SETTINGS = { version: 1, engine: B18, pondering: false, engines: [{ id: "e1", name: "B18 均衡", profile: B18 }], main_engine_id: "e1" } as unknown as Settings;
const SETUP = { rank: { type: "Dan", data: 3 }, mode: "LiveFeedback", color: "Black", pace: "Natural", handicap: 0 } as const;
const COACHING: CoachView = { active: true, setup: SETUP, human_color: "Black", hints: 0, take_backs: 0 };
const PLAYING: GameView = { phase: "Playing", black: { type: "Human" }, white: { type: "Ai", data: { type: "KataGo", data: {} } }, to_play: "Black", ai_thinking: false, move_number: 12 };
const RATINGS: CoachMoveRated[] = [
  { node_id: 2, move_number: 1, color: "Black", verdict: "Good", winrate_before: 0.5, winrate_after: 0.52, winrate_change: 0.02, points_lost: 0, human_share: 0.41 },
  { node_id: 4, move_number: 3, color: "Black", verdict: "Slow", winrate_after: 0.45, winrate_change: -0.07, points_lost: 2.1, human_share: 0.12 },
  { node_id: 6, move_number: 5, color: "Black", verdict: "Brilliant", winrate_after: 0.58, winrate_change: 0.13, points_lost: 0, human_share: 0.06 },
  { node_id: 8, move_number: 7, color: "Black", verdict: "Problem", winrate_after: 0.46, winrate_change: -0.12, points_lost: 4.3, human_share: 0.08 },
  { node_id: 10, move_number: 9, color: "Black", verdict: "Good", winrate_after: 0.49, winrate_change: 0.03, points_lost: 0.2, human_share: 0.35 },
];
const rated = RATINGS.map((rating) => ({ type: "CoachMoveRated", data: rating }) as const);

async function start(page: Page, commands: MockCommand[], events: NonNullable<MockScript["initialEvents"]> = []) {
  const script: MockScript = { initialEvents: [documentChanged(midGame(), 3), ...events], commands: [{ command: "settings", result: SETTINGS }, ...commands] };
  await page.addInitScript((initial) => {
    window.__mockInitialScript = initial;
  }, script);
  await page.goto("/");
  await expect(page.getByTestId("board")).toHaveAttribute("data-ready", "true");
}

/** Opens AI 陪练 and starts it; Rust's events for the game arrive with the start, as they do in the app. */
function coaching(events: NonNullable<MockScript["initialEvents"]>): MockCommand[] {
  return [
    { command: "humanslModelStatus", result: { type: "Ready", data: { path: "/w/b18c384nbt-humanv0.bin.gz" } } },
    { command: "startCoach", result: COACHING, events: [{ type: "CoachUpdated", data: COACHING }, { type: "GameUpdated", data: PLAYING }, ...events] },
  ];
}

async function begin(page: Page) {
  await page.getByTestId("coach-button").click();
  await expect(page.getByTestId("coach-model")).toHaveAttribute("data-state", "Ready");
  await page.getByTestId("coach-start").click();
  await expect(page.getByTestId("coach-panel")).toBeVisible();
}

async function calls(page: Page) {
  return (await page.evaluate(() => window.__mockBackend?.recordedCalls() ?? [])).filter((call) => call.command !== "settings");
}

async function quarter(page: Page) {
  const box = await page.getByTestId("board").boundingBox();
  if (!box) throw new Error("the board has no box");
  return { x: box.x, y: box.y, width: box.width / 2, height: box.height / 2 };
}

test("the AI 陪练 button opens the setup; with the model ready it starts the game", async ({ page }) => {
  await start(page, [
    { command: "humanslModelStatus", result: { type: "Ready", data: { path: "/w/b18c384nbt-humanv0.bin.gz" } } },
    { command: "startCoach", result: COACHING, events: [{ type: "CoachUpdated", data: COACHING }, { type: "GameUpdated", data: PLAYING }] },
  ]);
  await page.getByTestId("coach-button").click();
  const dialog = page.getByTestId("coach-dialog");
  await expect(dialog.getByTestId("coach-model")).toHaveAttribute("data-state", "Ready");
  await dialog.getByTestId("coach-color-Black").click();
  await expect(dialog).toHaveScreenshot("coach-setup-ready.png");
  await dialog.getByTestId("coach-start").click();
  await expect(page.getByTestId("coach-panel")).toBeVisible();
  await expect(page.getByTestId("coach-button")).toHaveText("陪练中");
  const [, call] = await calls(page);
  expect(call).toEqual({ command: "startCoach", args: [expect.objectContaining({ rank: { type: "Dan", data: 3 }, mode: "LiveFeedback", color: "Black", pace: "Natural", handicap: 0 })] });
});

test("without the model, 下载并开始 fetches it in the dialog with progress and pause", async ({ page }) => {
  const progress = { phase: "Downloading", asset_id: "human-sl", downloaded_bytes: 41_943_040, total_bytes: 99_066_230 } as const;
  await start(page, [
    { command: "humanslModelStatus", result: { type: "Missing" } },
    { command: "startAssetDownload", result: { ...progress, downloaded_bytes: 0 }, events: [{ type: "AssetDownloadProgress", data: progress }] },
  ]);
  await page.getByTestId("coach-button").click();
  const dialog = page.getByTestId("coach-dialog");
  await expect(dialog.getByTestId("coach-start")).toHaveText("下载并开始");
  await expect(dialog).toHaveScreenshot("coach-setup-download.png");
  await dialog.getByTestId("coach-start").click();
  await expect(dialog.getByTestId("coach-progress")).toContainText("42%");
  await expect(dialog.getByTestId("coach-pause")).toBeVisible();
  await expect(dialog).toHaveScreenshot("coach-setup-downloading.png");
  expect((await calls(page)).map((call) => [call.command, call.args])).toEqual([
    ["humanslModelStatus", []],
    ["startAssetDownload", ["human-sl"]],
  ]);
});

test("边下边提醒: the panel rates each move, and H gives a hint in two steps", async ({ page }) => {
  await start(page, [
    ...coaching([...rated, { type: "CoachUpdated", data: { ...COACHING, last_rating: RATINGS.at(-1)! } }]),
    { command: "coachHint", result: { type: "Region", data: { row: 0, column: 0 } } },
    { command: "coachHint", result: { type: "Exact", data: { mv: { type: "Play", data: { color: "Black", point: { x: 2, y: 3 } } }, human_share: 0.32 } } },
  ]);
  await begin(page);
  const panel = page.getByTestId("coach-panel");
  await expect(panel.getByTestId("coach-history")).toContainText("妙手 1");
  await expect(panel).toHaveScreenshot("coach-panel.png");
  await page.keyboard.press("KeyH");
  await expect(panel.getByTestId("coach-hint")).toHaveAttribute("data-step", "Region");
  await expect(panel.getByTestId("coach-hint")).toHaveScreenshot("coach-hint-region.png");
  await panel.getByTestId("coach-more-hint").click();
  await expect(panel.getByTestId("coach-hint")).toContainText("C16");
  await expect(page).toHaveScreenshot("coach-hint-exact.png", { clip: await quarter(page) });
});

test("a 恶手 holds the AI's reply until the player chooses", async ({ page }) => {
  const blunder: CoachMoveRated = { node_id: 12, move_number: 11, color: "Black", verdict: "Blunder", winrate_after: 0.27, winrate_change: -0.23, points_lost: 8.4, human_share: 0.03 };
  await start(page, [
    ...coaching([...rated, { type: "CoachMoveRated", data: blunder }, { type: "CoachUpdated", data: { ...COACHING, awaiting_decision: true, last_rating: blunder } }]),
    { command: "coachDecision", result: { type: "Exact", data: { mv: { type: "Play", data: { color: "Black", point: { x: 15, y: 3 } } }, human_share: 0.28 } } },
  ]);
  await begin(page);
  const card = page.getByTestId("coach-blunder");
  await expect(card).toContainText("第 11 手 · 恶手");
  await card.getByTestId("coach-show-best").click();
  await expect(card.getByTestId("coach-best")).toContainText("Q16");
  await expect(card).toHaveScreenshot("coach-blunder.png");
  expect((await calls(page)).map((call) => [call.command, call.args]).at(-1)).toEqual(["coachDecision", ["ShowBest"]]);
});

test("the summary counts the game and leads into the review", async ({ page }) => {
  const over: CoachView = {
    ...COACHING,
    active: false,
    hints: 2,
    take_backs: 1,
    summary: {
      result: "B+3.5",
      end_reason: "Scored",
      total_moves: 182,
      blunders: 2,
      hints: 2,
      take_backs: 1,
      worst_moves: [
        { node_id: 60, move_number: 59, verdict: "Blunder", points_lost: 11.2 },
        { node_id: 98, move_number: 97, verdict: "Blunder", points_lost: 7.6 },
        { node_id: 34, move_number: 33, verdict: "Problem", points_lost: 4.1 },
      ],
    },
  };
  await start(page, [...coaching([{ type: "GameUpdated", data: { ...PLAYING, phase: "Finished", result: "B+3.5" } }, { type: "CoachUpdated", data: over }]), { command: "coachReview", result: undefined }]);
  await begin(page);
  const summary = page.getByTestId("coach-summary");
  await expect(summary).toContainText("你赢了");
  await expect(summary.getByTestId("coach-worst")).toHaveCount(3);
  await expect(page.getByTestId("coach-panel")).toHaveScreenshot("coach-summary.png");
  await summary.getByTestId("coach-review").click();
  await expect(page.getByTestId("coach-panel")).toHaveCount(0);
  expect((await calls(page)).map((call) => call.command)).toEqual(["humanslModelStatus", "startCoach", "coachReview"]);
});
