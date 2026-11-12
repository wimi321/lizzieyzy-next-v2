import { expect, test, type Page } from "@playwright/test";
import type { AppEvent, DisplaySettings, DocumentView, MoveRank, ProblemMovesView } from "../src/api/backend";
import { analysedGame, analysisUpdated, defaultDisplaySettings, documentChanged, MID_GAME_STONES, midGame, recordedGame } from "./fixtures";
import type { MockCommand } from "./mock-backend";

test.use({ viewport: { width: 1280, height: 820 }, deviceScaleFactor: 1, locale: "zh-CN" });

type Display = Required<DisplaySettings>;
const saved = (change: Partial<Display>, from: Display = defaultDisplaySettings): Display => ({ ...from, ...change });
const save = (display: Display): MockCommand => ({ command: "setDisplaySettings", result: display });

async function start(page: Page, events: AppEvent[], commands: MockCommand[], problems: ProblemMovesView | null = null) {
  await page.goto("/");
  await page.evaluate(
    ([initialEvents, scripted, problemMoves]) => {
      window.__mockBackend?.setProblemMoves(problemMoves);
      window.__mockBackend?.setScript({ initialEvents, commands: scripted });
    },
    [events, commands, problems] as const,
  );
  const board = page.getByTestId("board");
  await expect(board).toHaveAttribute("data-ready", "true");
  await expect(board).not.toHaveAttribute("data-cell", "0.000");
  return board;
}

async function calls(page: Page) {
  return page.evaluate(() => window.__mockBackend?.recordedCalls() ?? []);
}

const row = (page: Page, id: string) => page.locator(`.menu-row[data-menu-id="${id}"]`);

/** 显示 → … → entry, through the submenus on the way. */
async function choose(page: Page, ...path: string[]) {
  await page.getByTestId("menubar").locator('[data-menu-id="viewMenu"]').click();
  for (const id of path.slice(0, -1)) await row(page, id).hover();
  await row(page, path[path.length - 1]!).click();
}

/** The board's top-left quarter, or its top half: enough to see the marks, small enough for the file budget. */
async function boardPart(page: Page, part: "quarter" | "top") {
  const box = await page.getByTestId("board").boundingBox();
  if (!box) throw new Error("the board has no box");
  return { x: box.x, y: box.y, width: part === "quarter" ? box.width / 2 : box.width, height: box.height / 2 };
}

/** The recorded middle game with each stone's move and node, as Rust sends them; node ids equal move numbers. */
function numbered(view: DocumentView): DocumentView {
  const board = view.board!;
  board.stone_move_numbers = board.stones.map(() => null);
  board.stone_node_ids = board.stones.map(() => null);
  MID_GAME_STONES.forEach(([x, y], index) => {
    board.stone_move_numbers![y * board.width + x] = index + 1;
    board.stone_node_ids![y * board.width + x] = index + 1;
  });
  return view;
}

test("显示 → 坐标 hides the coordinates, the board grows, and the choice is saved", async ({ page }) => {
  // Without player names: names under the board keep its margin, as in Java.
  const board = await start(page, [documentChanged(numbered(midGame()), 7)], [save(saved({ show_coordinates: false }))]);
  const before = Number(await board.getAttribute("data-cell"));
  await choose(page, "coordsMenu");
  await expect.poll(async () => Number(await board.getAttribute("data-cell"))).toBeGreaterThan(before);
  expect(await calls(page)).toEqual([{ command: "setDisplaySettings", args: [saved({ show_coordinates: false })] }]);
  await expect(page).toHaveScreenshot("no-coordinates.png", { clip: await boardPart(page, "quarter") });

  await page.getByTestId("menubar").locator('[data-menu-id="viewMenu"]').click();
  await expect(row(page, "coordsMenu")).toHaveAttribute("aria-checked", "false");
});

test("手数 → 全部 numbers every stone, the last in red; M steps on to the last move only", async ({ page }) => {
  const { view } = recordedGame(7);
  const all = saved({ move_numbers: "All" });
  const board = await start(page, [documentChanged(numbered(view), 7)], [save(all), save(saved({ move_numbers: "LastOne" }))]);
  await expect(board).toHaveAttribute("data-numbers", "0");
  await choose(page, "moveMenu", "allMoveNum");
  await expect(board).toHaveAttribute("data-numbers", "20");
  await expect(page.getByTestId("toggle-move-numbers")).toHaveAttribute("aria-pressed", "true");
  await expect(page).toHaveScreenshot("move-numbers.png", { clip: await boardPart(page, "quarter") });

  await page.locator('[data-region="statistics"]').click();
  await page.keyboard.press("m");
  await expect(board).toHaveAttribute("data-numbers", "1");
  expect((await calls(page)).map((call) => call.args[0])).toEqual([all, saved({ move_numbers: "LastOne" })]);
});

test("落子评价标记 → 全部 marks every ranked move with a dot in its rank's colour", async ({ page }) => {
  const { view, problems } = recordedGame(7);
  const ranks = new Map<number, MoveRank>([[1, "Good"], [9, "Mistake"], [12, "Blunder"], [13, "Inaccuracy"], [14, "Best"]]);
  for (const point of problems.error_bar) {
    const rank = ranks.get(point.node_id);
    if (rank) point.loss = { winrate_loss: 5, score_loss: 1, rank, is_best: rank === "Best", candidate_rank: 2, visits: 800, before_visits: 800 };
  }
  const board = await start(page, [documentChanged(numbered(view), 7)], [save(saved({ move_rank_marks: "All" }))], problems);
  await expect(board).toHaveAttribute("data-rank-dots", "0");
  await choose(page, "moveRankMenu", "rankAllMove");
  await expect(board).toHaveAttribute("data-rank-dots", "5");
  await expect(board).toHaveAttribute("data-last-rank", "Inaccuracy");
  await expect(page).toHaveScreenshot("rank-marks.png", { clip: await boardPart(page, "quarter") });
});

test("胜率图设置: only the score line, filled, with the blunder bars", async ({ page }) => {
  const game = analysedGame();
  const loss = (rank: MoveRank) => ({ winrate_loss: rank === "Blunder" ? 24 : 9, score_loss: 4, rank, is_best: false, candidate_rank: 4, visits: 1000, before_visits: 1000 });
  const problems: ProblemMovesView = {
    revision: 7,
    settings: { mode: "Auto", winrate_thresholds: [1, 3, 6, 12, 24], score_thresholds: [0.5, 1.5, 3, 6, 12] },
    query: { minimum_rank: "Inaccuracy", color: null, sort: "LossDesc" },
    entries: [],
    error_bar: game.graph.points.map((point) => ({
      node_id: point.node_id,
      move_number: point.move_number,
      mv: null,
      loss: point.move_number === 37 ? loss("Blunder") : point.move_number === 52 ? loss("Mistake") : null,
    })),
    current_index: 20,
  };
  const scoreOnly = saved({ winrate_graph_mode: "ScoreLead" });
  await start(page, [documentChanged(game, 7)], [save(scoreOnly), save(saved({ show_blunder_bar: true }, scoreOnly))], problems);
  const graph = page.getByTestId("winrate-graph");
  await expect(graph.getByTestId("graph-winrate")).not.toHaveCount(0);
  await expect(graph.getByTestId("graph-mistake")).toHaveCount(0);

  await choose(page, "winrate", "showWinRateOrScoreLeadLine", "showScoreLeadLineOnly");
  await expect(graph.getByTestId("graph-winrate")).toHaveCount(0);
  await expect(graph.getByTestId("graph-fill")).not.toHaveCount(0);
  await choose(page, "winrate", "showBlunderBar");
  await expect(graph.getByTestId("graph-mistake")).toHaveCount(2);
  await expect(graph).toHaveScreenshot("graph-score-only.png");
});

test("面板: hiding the graph and the comment gives their room to the sub-board and the tree", async ({ page }) => {
  await page.setViewportSize({ width: 960, height: 600 });
  const { view } = recordedGame(7);
  const noGraph = saved({ show_winrate_graph: false });
  await start(page, [documentChanged(view, 7)], [save(noGraph), save(saved({ show_comment: false }, noGraph))]);
  const tree = page.locator('[data-region="tree"]');
  const treeBefore = (await tree.boundingBox())!.height;
  await choose(page, "panel", "winrateGraph");
  await expect(page.locator('[data-region="graph"]')).toHaveCount(0);
  await choose(page, "panel", "commitPane");
  await expect(page.locator('[data-region="comment"]')).toHaveCount(0);
  // The sub-board moves under the graph's place; the tree takes the whole right column.
  await expect.poll(async () => (await tree.boundingBox())!.height).toBeGreaterThan(treeBefore);
  const sub = (await page.locator('[data-region="sub-board"]').boundingBox())!;
  expect(sub.x).toBeLessThan((await page.getByTestId("board").boundingBox())!.x);
  await expect(page.getByTestId("main-panel")).toHaveScreenshot("hidden-panels.png");
});

test("下一手提示 → 简单 rings the recorded moves without what they lead to", async ({ page }) => {
  const { view, tree, problems, analysis } = recordedGame(7);
  const board = await start(
    page,
    [documentChanged(view, 7), analysisUpdated(analysis)],
    [{ command: "variationTree", result: tree }, save(saved({ next_moves: "Simple" }))],
    problems,
  );
  await expect(board).toHaveAttribute("data-next-moves", "2");
  await choose(page, "nextMoveHint", "nextMoveHintSimple");
  await expect.poll(async () => (await calls(page)).length).toBe(2);
  await page.getByTestId("menubar").locator('[data-menu-id="viewMenu"]').click();
  await row(page, "nextMoveHint").hover();
  await expect(row(page, "nextMoveHintSimple")).toHaveAttribute("aria-checked", "true");
  await expect(row(page, "minPlayoutsForNextMove")).toContainText("30)");
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  await expect(page).toHaveScreenshot("next-move-simple.png", { clip: await boardPart(page, "top") });
});
