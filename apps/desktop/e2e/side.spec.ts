import { expect, test } from "@playwright/test";
import type { ProblemMovesView, WireAppEvent } from "../src/api/backend";
import { analysisUpdated, captureVariation, documentChanged, midGame, recordedGame } from "./fixtures";
import type { MockCommand, MockScript } from "./mock-backend";

// The window of the reference screenshots, in LizzieYzy Next's own language.
test.use({ viewport: { width: 1429, height: 941 }, deviceScaleFactor: 1, locale: "zh-CN" });

const ENGINE = { program: "/opt/katago", model: "/opt/kata1-tf3-b11c768-s12002M-d6304M.bin.gz", config: "", extra_args: [] };

test("the side columns show what Java's show: the game's terms, the evaluation, the note on the move, the tree and the expected line", async ({ page }) => {
  const { view, tree, problems } = recordedGame(7);
  view.game_info.rules = "Chinese";
  // The engine's first choice is a line of seven moves; Rust replays it for the sub-board.
  const { analysis, moves, replay } = captureVariation(7);
  await page.goto("/");
  await page.evaluate(
    ([events, commands, problemMoves]) => {
      window.__mockBackend?.setProblemMoves(problemMoves);
      window.__mockBackend?.setScript({ initialEvents: events, commands });
    },
    [
      [documentChanged(view, 7), analysisUpdated(analysis)],
      [
        { command: "variationTree", result: tree },
        { command: "previewVariation", result: replay },
      ],
      problems,
    ] as [WireAppEvent[], MockCommand[], ProblemMovesView],
  );

  await expect(page.getByTestId("game-rules")).toHaveText("中国规则");
  await expect(page.getByTestId("to-play")).toHaveText("黑走");
  await expect(page.getByTestId("game-komi")).toHaveText("贴目 7.5");
  await expect(page.getByTestId("lead-line")).toHaveText("黑领先 1.6 目上一手 −1.2% −0.2目");
  // White played the last move: the note is read from White's side.
  await expect(page.getByTestId("analysis-summary")).toHaveText("白棋 胜率: 43.8% (−1.2%)\n领先: −1.6 (−0.2)\n(3.6k 计算量)\n贴目: 7.5", { useInnerText: true });
  await expect(page.getByTestId("review-status")).toHaveText("已评估 3/22");
  // The ranked move's dot takes its rank; both continuations hang from the current move.
  await expect(page.locator("[data-testid='tree-node'][data-current='true']")).toHaveAttribute("data-rank", "Inaccuracy");
  await expect(page.getByTestId("tree-node")).toHaveCount(23);
  // One replay serves the sub-board at once and the main board when the move is hovered.
  await expect(page.getByTestId("sub-board")).toHaveAttribute("data-moves", "7");
  await expect(page.getByTestId("board")).toHaveAttribute("data-ready", "true");
  await expect
    .poll(() => page.evaluate(() => window.__mockBackend?.recordedCalls() ?? []))
    .toEqual([
      { command: "variationTree", args: [] },
      { command: "previewVariation", args: [20, moves] },
    ]);
  // The columns only: the board between them has its own baselines.
  const box = async (region: string) => {
    const rect = await page.locator(`[data-region="${region}"]`).boundingBox();
    if (!rect) throw new Error(`no ${region} region`);
    return rect;
  };
  const [captured, status, moves, sub] = await Promise.all([box("captured"), box("status"), box("tree"), box("sub-board")]);
  await expect(page).toHaveScreenshot("left-column.png", {
    clip: { x: captured.x, y: captured.y, width: captured.width, height: status.y + status.height - captured.y },
  });
  await expect(page).toHaveScreenshot("right-column.png", {
    clip: { x: moves.x, y: moves.y, width: moves.width, height: sub.y + sub.height - moves.y },
  });
});

test("Rust names the engine, a catalog model in the interface language; the title ends with the file's name", async ({ page }) => {
  const view = midGame();
  view.file_name = "game.sgf";
  const script: MockScript = {
    initialEvents: [documentChanged(view, 7), { type: "EngineStatusChanged", data: { type: "Ready", data: { version: "1.18.2" } } }],
    commands: [
      { command: "settings", result: { version: 1, engine: ENGINE, pondering: true } },
      { command: "engineDisplayName", result: { type: "Catalog", data: "b11-flagship" } },
    ],
  };
  await page.addInitScript((initial) => {
    window.__mockInitialScript = initial;
  }, script);
  await page.goto("/");
  await expect(page.getByTestId("status-line")).toHaveText("Transformer 11B 旗舰版 · 分析中 · 空格 暂停");
  await expect(page).toHaveTitle(/^LizzieYzy Next 2 - Transformer 11B 旗舰版 .*B11.* - game\.sgf$/);
  await expect(page.getByRole("menubar")).toContainText("Transformer 11B 旗舰版");
});

test("a game that cannot be opened is reported in a message box", async ({ page }) => {
  await page.goto("/");
  await page.evaluate(
    (events) => window.__mockBackend?.setScript({ initialEvents: events }),
    [documentChanged(midGame(), 7), { type: "OpenFailed", data: { path: "/games/2026/broken.sgf", reason: "not an SGF file" } }] as WireAppEvent[],
  );
  const box = page.getByTestId("open-failed");
  await expect(box).toBeVisible();
  await expect(box).toContainText("无法打开棋谱");
  await expect(box).toContainText("broken.sgf");
  await expect(box).toContainText("not an SGF file");
  await box.getByTestId("open-failed-ok").click();
  await expect(box).toBeHidden();
});
