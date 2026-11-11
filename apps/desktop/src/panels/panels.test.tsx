import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { BoardView, GameInfoView, GraphPointView, MoveRank, VariationTreeView } from "@/api/backend";
import en from "../locales/en.json";
import zh from "../locales/zh-CN.json";
import { analysisSummary } from "./info";
import { CapturedBox, StatisticsBox } from "./InfoPanel";
import { MoveTree } from "./MoveTree";
import { NotesPanel } from "./NotesPanel";

afterEach(cleanup);

const tree: VariationTreeView = {
  revision: 3,
  root: 0,
  nodes: [
    { id: 0, parent: null, children: [1], move_number: 0, mv: null, move_error: null },
    { id: 1, parent: 0, children: [2, 4], move_number: 1, mv: { type: "Play", data: { color: "Black", point: { x: 15, y: 3 } } }, move_error: null },
    { id: 2, parent: 1, children: [3], move_number: 2, mv: { type: "Play", data: { color: "White", point: { x: 3, y: 15 } } }, move_error: null },
    { id: 3, parent: 2, children: [], move_number: 3, mv: { type: "Pass", data: { color: "Black" } }, move_error: null },
    { id: 4, parent: 1, children: [], move_number: 2, mv: { type: "Play", data: { color: "White", point: { x: 2, y: 2 } } }, move_error: null },
  ],
};

const board: BoardView = {
  width: 19, height: 19, stones: [], last_move: null, to_play: "White", move_number: 0, captured_black: 2, captured_white: 0,
};

const game: GameInfoView = {
  black_name: "甲", black_rank: "9段", white_name: "乙", white_rank: null, komi: 7.5, rules: "Chinese",
  handicap: null, result: "B+R", date: "2026-10-28", event: "野狐 十段赛",
};

const points: GraphPointView[] = [
  { node_id: 0, move_number: 0, summary: { winrate: 0.5, score_lead: 0.5, visits: 10 } },
  { node_id: 1, move_number: 1, summary: { winrate: 0.62, score_lead: 2.4, visits: 2300 } },
];

test("the move tree draws every node, marks the current one, colours ranked moves and jumps on click", () => {
  const onJump = vi.fn();
  const ranks = new Map<number, MoveRank>([[2, "Mistake"]]);
  const { container } = render(<MoveTree tree={tree} current={2} line={[0, 1, 2, 3]} boardSize={19} ranks={ranks} label={zh.tree.label} onJump={onJump} />);
  const nodes = screen.getAllByTestId("tree-node");
  expect(nodes.map((node) => node.dataset.id)).toEqual(["0", "1", "2", "3", "4"]);
  expect(nodes.find((node) => node.dataset.current === "true")?.dataset.id).toBe("2");
  expect(nodes[1]?.querySelector("title")?.textContent).toBe("1 · Q16");
  // The root is a square; a ranked move's dot takes its rank's colour.
  expect(nodes[0]?.querySelector(".tree-root")).not.toBeNull();
  expect(nodes[2]?.dataset.rank).toBe("Mistake");
  expect((nodes[2]?.querySelector(".tree-dot") as SVGElement).style.fill).toBe("var(--rank-mistake)");
  expect(nodes[1]?.dataset.rank).toBeUndefined();
  // Node 4 is off the displayed line, so its dot is dimmed.
  expect(nodes[4]?.querySelector(".tree-dot")?.classList.contains("off-line")).toBe(true);
  expect(container.querySelectorAll(".tree-edge.lit")).toHaveLength(3);
  // The current move carries its number.
  expect(nodes[2]?.querySelector(".tree-number")?.textContent).toBe("2");

  fireEvent.click(nodes[4]!);
  expect(onJump).toHaveBeenCalledWith(4);
  fireEvent.click(nodes[2]!);
  expect(onJump).toHaveBeenCalledTimes(1);
});

test("the statistics show both winrates around the move number, the bar, the lead and the last move's change", () => {
  const played: BoardView = { ...board, move_number: 1, last_move: { type: "Play", data: { color: "Black", point: { x: 3, y: 3 } } } };
  render(<StatisticsBox board={played} points={points} currentIndex={1} labels={zh.info} />);
  expect(screen.getByText("黑 62.0%")).toBeTruthy();
  expect(screen.getByText("白 38.0%")).toBeTruthy();
  expect(screen.getByTestId("move-number").textContent).toBe("1");
  expect(screen.getByTestId("winrate-bar").dataset.black).toBe("62.0");
  expect(screen.getByTestId("lead-line").textContent).toBe("黑领先 2.4 目上一手 +12.0% +1.9目");
});

test("before any analysis the statistics say so", () => {
  render(<StatisticsBox board={board} points={[]} currentIndex={0} labels={zh.info} />);
  expect(screen.getByText("黑 —")).toBeTruthy();
  expect(screen.getByTestId("lead-line").textContent).toBe(zh.info.noAnalysis);
});

test("the captured-stones box gives the rules, whose turn it is, the captures and the komi", () => {
  const { rerender } = render(<CapturedBox board={board} game={game} labels={zh.info} />);
  expect(screen.getByTestId("game-rules").textContent).toBe("中国规则");
  // What has no room in the box is on hover.
  expect(screen.getByTestId("game-rules").getAttribute("title")).toBe("B+R · 2026-10-28 · 野狐 十段赛");
  expect(screen.getByTestId("to-play").textContent).toBe("白走");
  expect(screen.getByTestId("game-komi").textContent).toBe("贴目 7.5");
  expect(screen.getByText("提子 2")).toBeTruthy();
  expect(screen.getByText("提子 0")).toBeTruthy();
  // The side to move has the larger stone; the stones name the players.
  expect(screen.getByAltText("白方").dataset.toPlay).toBe("true");
  expect(screen.getByAltText("黑方").dataset.toPlay).toBe("false");
  expect(screen.getByAltText("黑方").getAttribute("title")).toBe("甲 9段");
  expect(screen.getByAltText("白方").getAttribute("title")).toBe("乙");

  rerender(
    <CapturedBox
      board={{ ...board, to_play: "Black" }}
      game={{ ...game, komi: 0.5, handicap: 3, rules: "tromp_taylor", result: null, date: null, event: null }}
      labels={en.info}
    />,
  );
  expect(screen.getByTestId("game-rules").textContent).toBe("Tromp-Taylor rules");
  expect(screen.getByTestId("game-rules").getAttribute("title")).toBe("3 handicap");
  expect(screen.getByTestId("to-play").textContent).toBe("Black to play");
  expect(screen.getByTestId("game-komi").textContent).toBe("Komi 0.5");

  // A handicap game that records no rules says the handicap instead.
  rerender(<CapturedBox board={board} game={{ ...game, rules: null, handicap: 4 }} labels={zh.info} />);
  expect(screen.getByTestId("game-rules").textContent).toBe("让 4 子");
});

test("the match rates join the box once the strength estimate has them", () => {
  const { rerender } = render(<CapturedBox board={board} game={game} blackMatch={0.736} whiteMatch={null} labels={zh.info} />);
  expect(screen.getByTestId("player-match-black").textContent).toBe("吻合度 73.6");
  expect(screen.getByTestId("player-match-black").getAttribute("title")).toBe(zh.info.matchHint);
  expect(screen.queryByTestId("player-match-white")).toBeNull();

  rerender(<CapturedBox board={board} game={game} labels={zh.info} />);
  expect(screen.queryByTestId("player-match-black")).toBeNull();
});

test("the analysis note reads from the side that played the move", () => {
  const whiteMove = { type: "Play", data: { color: "White", point: { x: 3, y: 3 } } } as const;
  // White just played: its winrate is 38.0%, down 12.0 points; it trails by 2.4, 1.9 worse than before.
  expect(analysisSummary(points, 1, whiteMove, "Transformer B11 · 2026-09-25", 7.5, zh.notes)).toEqual([
    "白棋 胜率: 38.0% (−12.0%)",
    "领先: −2.4 (−1.9)",
    "(Transformer B11 · 2026-09-25 / 2.3k 计算量)",
    "贴目: 7.5",
  ]);
  // Nobody played the root: Black's side, no change, and no engine name yet.
  expect(analysisSummary(points, 0, null, null, 6.5, en.notes)).toEqual(["Black winrate: 50.0%", "Lead: 0.5", "(10 visits)", "Komi: 6.5"]);
  expect(analysisSummary([{ node_id: 0, move_number: 0, summary: null }], 0, null, null, 7.5, zh.notes)).toEqual([]);
});

test("the notes panel opens on the comment tab: the analysis note, then the game's comment", () => {
  const onTab = vi.fn();
  const { rerender } = render(
    <NotesPanel tab="comment" onTab={onTab} problemCount={2} status={{ state: "done", text: zh.notes.statusDone }} summary={["白棋 胜率: 38.0%", "贴目: 7.5"]} comment="好棋" labels={zh.notes}>
      <p data-testid="problem-stub" />
    </NotesPanel>,
  );
  expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual(["评论", "问题手2"]);
  expect(screen.getByTestId("tab-comment").getAttribute("aria-selected")).toBe("true");
  expect(screen.getByTestId("review-status").textContent).toBe("评估完成");
  expect(screen.getByTestId("review-status").dataset.state).toBe("done");
  expect(screen.getByTestId("analysis-summary").textContent).toBe("白棋 胜率: 38.0%\n贴目: 7.5");
  expect(screen.getByTestId("comment").textContent).toContain("好棋");
  expect(screen.queryByTestId("problem-stub")).toBeNull();
  fireEvent.click(screen.getByTestId("tab-problems"));
  expect(onTab).toHaveBeenCalledWith("problems");

  rerender(
    <NotesPanel tab="problems" onTab={onTab} problemCount={0} status={null} summary={[]} comment={null} labels={zh.notes}>
      <p data-testid="problem-stub" />
    </NotesPanel>,
  );
  expect(screen.getByTestId("problem-stub")).toBeTruthy();
  expect(screen.queryByTestId("review-status")).toBeNull();

  rerender(
    <NotesPanel tab="comment" onTab={onTab} problemCount={0} status={null} summary={[]} comment={null} labels={zh.notes}>
      <p data-testid="problem-stub" />
    </NotesPanel>,
  );
  expect(screen.getByTestId("comment").textContent).toBe(zh.notes.noComment);
});
