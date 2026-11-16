import { expect, test, type Locator, type Page, type WebSocketRoute } from "@playwright/test";
import type { BoardView, ProblemMovesView, WebClientMessage, WebServerMessage } from "../src/api/backend";
import type { WebTrialState } from "../src/api/generated/WebTrialState";
import { analysedGame, boardView, MID_GAME_STONES, midGame, midGameAnalysis } from "./fixtures";

// The browser page P3-32's service hands out, against a stand-in for that service: the
// WebSocket is answered here, in the test, and never leaves the browser.

const BOARD = midGame().board!;
const loss = (winrate_loss: number, rank: "Inaccuracy" | "Mistake" | "Blunder") => ({ winrate_loss, score_loss: winrate_loss / 3, rank, is_best: false, candidate_rank: 5, visits: 800, before_visits: 800 });
const PROBLEMS: ProblemMovesView = {
  revision: 3,
  settings: { mode: "Auto", winrate_thresholds: [1, 3, 6, 12, 24], score_thresholds: [0.5, 1.5, 3, 6, 12] },
  query: { minimum_rank: "Inaccuracy", color: null, sort: "LossDesc" },
  entries: [
    { node_id: 37, move_number: 37, color: "Black", coordinate: "R14", loss: loss(14.2, "Blunder") },
    { node_id: 52, move_number: 52, color: "White", coordinate: "C6", loss: loss(7.8, "Mistake") },
    { node_id: 18, move_number: 18, color: "Black", coordinate: "K4", loss: loss(3.1, "Inaccuracy") },
  ],
  error_bar: [],
  current_index: 20,
};
const OPENING: WebServerMessage[] = [
  { type: "Hello", data: { client_id: "viewer-1", reconnect: "r-1", komi: 7.5, rules: "chinese", allow_trial: true } },
  { type: "FullState", data: { revision: 3, node_id: 20, board: BOARD, analysis: midGameAnalysis(3, 20, 1, true) } },
  { type: "WinrateHistory", data: analysedGame().graph },
  { type: "ProblemMoves", data: PROBLEMS },
  { type: "TrialState", data: { active: false } },
];

interface Host {
  sockets: WebSocketRoute[];
  urls: string[];
  received: WebClientMessage[];
}

async function serve(page: Page, answer: (message: WebClientMessage, send: (message: WebServerMessage) => void) => void = () => undefined): Promise<Host> {
  const host: Host = { sockets: [], urls: [], received: [] };
  await page.routeWebSocket(/\/ws(\?.*)?$/, (socket) => {
    const send = (message: WebServerMessage) => socket.send(JSON.stringify(message));
    host.sockets.push(socket);
    host.urls.push(socket.url());
    socket.onMessage((data) => {
      const message = JSON.parse(String(data)) as WebClientMessage;
      host.received.push(message);
      answer(message, send);
    });
    for (const message of OPENING) send(message);
  });
  await page.goto("/web.html");
  await expect(page.getByTestId("board")).toHaveAttribute("data-ready", "true");
  await expect(page.getByTestId("web-overlay")).toHaveCount(0);
  return host;
}

/** The CSS pixel of a point, from the board's own layout attributes. */
async function pixel(board: Locator, x: number, y: number) {
  const box = (await board.boundingBox())!;
  const read = async (name: string) => Number(await board.getAttribute(name));
  const cell = await read("data-cell");
  return { x: box.x + (await read("data-origin-x")) + cell * x, y: box.y + (await read("data-origin-y")) + cell * y };
}

const status = (page: Page) => page.getByTestId("web-status");

test.describe("on a computer", () => {
  test.use({ viewport: { width: 1280, height: 820 }, deviceScaleFactor: 1, locale: "zh-CN" });

  test("the host's game and analysis, the graph and the problem moves; hovering a candidate shows its line", async ({ page }) => {
    await serve(page);
    for (const part of ["第 20 手", "黑走", "黑胜率 56.2%", "黑目差 +1.6", "计算量 3.6k", "贴目 7.5", "规则 chinese"]) await expect(status(page)).toContainText(part);
    await expect(page.getByTestId("winrate-graph")).toHaveAttribute("data-count", "121");
    await expect(page.getByTestId("web-problems")).toContainText("R14");
    await expect(page.getByTestId("web-trial-enter")).toBeEnabled();

    const board = page.getByTestId("board");
    const at = await pixel(board, 10, 6);
    await page.mouse.move(at.x, at.y);
    await expect(board).toHaveAttribute("data-preview", "10,6");
    await expect(page).toHaveScreenshot("web-page-desktop.png");

    // Over the graph, a move's own values; away from it, the current move's again.
    const graph = page.getByTestId("winrate-graph");
    const plot = page.locator(".web-graph-plot");
    const box = (await graph.boundingBox())!;
    const left = Number(await graph.getAttribute("data-plot-left"));
    const right = Number(await graph.getAttribute("data-plot-right"));
    await page.mouse.move(box.x + left + (60 / 120) * (box.width - left - right), box.y + box.height / 2);
    await expect(plot).toHaveAttribute("data-inspected", "60");
    const sixty = analysedGame().graph.points[60]!.summary!;
    await expect(graph.locator(".graph-value:not(.score)")).toHaveText((sixty.winrate * 100).toFixed(1));
    await page.mouse.move(box.x + box.width / 2, box.y - 60);
    await expect(plot).not.toHaveAttribute("data-inspected");

    // 白方视角 turns the numbers over and is kept in this browser.
    await page.getByTestId("web-perspective-white").click();
    await expect(status(page)).toContainText("白胜率 43.8%");
    await expect(status(page)).toContainText("白目差 −1.6");
    await page.reload();
    await expect(status(page)).toContainText("白胜率 43.8%");
    await expect(page.getByTestId("web-perspective-white")).toHaveAttribute("aria-checked", "true");
  });

  test("trying moves: asking, playing, following a branch, stepping back, leaving; refusals; someone else's trial", async ({ page }) => {
    const mine = (state: Partial<WebTrialState>): WebServerMessage => ({
      type: "TrialState",
      data: { active: true, owner: "viewer-1", anchor_move: 20, move_number: 20, can_back: false, can_forward: false, branches: [{ x: 6, y: 9, label: "1", index: 0 }], ...state },
    });
    const played: BoardView = boardView(19, 19, [...MID_GAME_STONES, [4, 4, "Black"]], { toPlay: "White", moveNumber: 21, last: [4, 4, "Black"] });
    const host = await serve(page, (message, send) => {
      if (message.type === "EnterTrial") send(mine({}));
      if (message.type === "TrialMove") {
        send({ type: "FullState", data: { revision: 4, node_id: 90, board: played, analysis: null } });
        send(mine({ move_number: 21, can_back: true, branches: [] }));
      }
      if (message.type === "TrialBack") {
        send({ type: "FullState", data: { revision: 5, node_id: 20, board: BOARD, analysis: null } });
        send(mine({}));
      }
      if (message.type === "ExitTrial") send({ type: "TrialState", data: { active: false } });
    });
    const trial = page.getByTestId("web-trial");
    await page.getByTestId("web-trial-enter").click();
    await expect(trial).toHaveAttribute("data-state", "mine");
    await expect(trial).toContainText("点棋盘落子");
    await expect(page.getByTestId("web-trial-back")).toBeDisabled();

    const board = page.getByTestId("board");
    const branch = await pixel(board, 6, 9);
    await page.mouse.click(branch.x, branch.y);
    const empty = await pixel(board, 4, 4);
    await page.mouse.click(empty.x, empty.y);
    await expect(status(page)).toContainText("第 21 手");
    await expect(page.getByTestId("web-trial-back")).toBeEnabled();
    await page.getByTestId("web-trial-back").click();
    await expect(status(page)).toContainText("第 20 手");
    await page.getByTestId("web-trial-exit").click();
    await expect(trial).toHaveAttribute("data-state", "none");
    expect(host.received).toEqual([
      { type: "EnterTrial" },
      { type: "TrialChild", data: { index: 0 } },
      { type: "TrialMove", data: { x: 4, y: 4 } },
      { type: "TrialBack" },
      { type: "ExitTrial" },
    ]);

    const socket = host.sockets[0]!;
    socket.send(JSON.stringify({ type: "TrialDenied", data: { reason: "InUse" } } satisfies WebServerMessage));
    await expect(page.getByTestId("web-notice")).toHaveText("别人正在试下，请稍后再试。");
    socket.send(JSON.stringify({ type: "TrialState", data: { active: true, owner: "viewer-2" } } satisfies WebServerMessage));
    await expect(trial).toHaveAttribute("data-state", "others");
    await expect(trial).toHaveText("有人正在试下");
    await expect(page.getByTestId("web-trial-enter")).toHaveCount(0);
  });

  test("a dropped connection veils the page, then reconnects with the server's credential", async ({ page }) => {
    const host = await serve(page);
    expect(host.urls[0]).toMatch(/\/ws$/);
    await host.sockets[0]!.close();
    await expect(page.getByTestId("web-overlay")).toHaveText("连接中断，正在重连…");
    // The last position stays under the veil.
    await expect(status(page)).toContainText("第 20 手");
    await expect(page.getByTestId("web-overlay")).toHaveCount(0);
    expect(host.urls[1]).toMatch(/\/ws\?reconnect=r-1$/);
  });
});

test.describe("on a phone held upright", () => {
  test.use({ viewport: { width: 390, height: 844 }, deviceScaleFactor: 1, hasTouch: true, isMobile: true, locale: "zh-CN" });

  test("the board across the screen, the rest below; a long press shows a candidate's line", async ({ page }) => {
    const host = await serve(page, (message, send) => {
      if (message.type === "EnterTrial") send({ type: "TrialState", data: { active: true, owner: "viewer-1", can_back: false, can_forward: false, branches: [] } });
    });
    const board = page.getByTestId("board");
    const boardBox = (await board.boundingBox())!;
    const sideBox = (await page.getByTestId("web-trial").boundingBox())!;
    expect(boardBox.width).toBeGreaterThan(340);
    expect(sideBox.y).toBeGreaterThanOrEqual(boardBox.y + boardBox.height);

    await page.getByTestId("web-coordinates").selectOption("fox");
    const at = await pixel(board, 10, 6);
    const area = page.locator(".web-board");
    await area.dispatchEvent("pointerdown", { pointerType: "touch", pointerId: 7, isPrimary: true, clientX: at.x, clientY: at.y, bubbles: true });
    await expect(board).toHaveAttribute("data-preview", "10,6");
    await expect(page).toHaveScreenshot("web-page-portrait.png", { fullPage: true });
    await area.dispatchEvent("pointerup", { pointerType: "touch", pointerId: 7, isPrimary: true, clientX: at.x, clientY: at.y, bubbles: true });
    await expect(board).toHaveAttribute("data-preview", "");

    // Trying moves: a long press the browser takes for a scroll gets no click; the next tap still plays.
    await page.getByTestId("web-trial-enter").tap();
    await expect(page.getByTestId("web-trial")).toHaveAttribute("data-state", "mine");
    await area.dispatchEvent("pointerdown", { pointerType: "touch", pointerId: 8, isPrimary: true, clientX: at.x, clientY: at.y, bubbles: true });
    await expect(board).toHaveAttribute("data-preview", "10,6");
    await area.dispatchEvent("pointercancel", { pointerType: "touch", pointerId: 8, isPrimary: true, clientX: at.x, clientY: at.y, bubbles: true });
    await expect(board).toHaveAttribute("data-preview", "");
    const empty = await pixel(board, 4, 4);
    await page.touchscreen.tap(empty.x, empty.y);
    await expect.poll(() => host.received).toEqual([{ type: "EnterTrial" }, { type: "TrialMove", data: { x: 4, y: 4 } }]);
  });
});
