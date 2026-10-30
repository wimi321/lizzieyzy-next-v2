import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { AnalysisView, BoardView, CandidateView, GraphPointView, VariationTreeView } from "@/api/backend";
import zh from "../locales/zh-CN.json";
import { CandidateTable } from "./CandidateTable";
import { InfoPanel } from "./InfoPanel";
import { MoveTree } from "./MoveTree";

afterEach(cleanup);

function candidate(order: number, x: number, winrate: number, visits: number, score: number): CandidateView {
  return {
    coordinate: `P${order}`,
    point: { x, y: 3 },
    visits,
    winrate,
    score_lead: score,
    prior: 0.1,
    order,
    pv: [],
  };
}

function analysis(candidates: CandidateView[]): AnalysisView {
  return {
    node_id: 4,
    revision: 1,
    winrate: 0.6,
    score_lead: 2,
    visits: 1000,
    summary: { winrate: 0.6, score_lead: 2, visits: 1000 },
    candidates,
    ownership: null,
    is_during_search: true,
    source: "Live",
  };
}

const rows = () => screen.getAllByTestId("candidate-row");
const LIMITS = [5, 10, 20, 361] as const;
const labels = { ...zh.table, limit: zh.board.candidates, limitAll: zh.board.candidatesAll };

test("the candidate table lists engine order, mover values and visit shares", () => {
  const onPreview = vi.fn();
  const onPlay = vi.fn();
  // Out of order on purpose; 600 + 300 + 100 visits in total.
  const view = analysis([candidate(1, 2, 0.4, 300, -1), candidate(0, 1, 0.3, 600, -2.5), candidate(2, 5, 0.45, 100, 0.5)]);
  const onMaxCandidates = vi.fn();
  render(
    <CandidateTable
      analysis={view}
      toPlay="White"
      maxCandidates={2}
      candidateLimits={LIMITS}
      labels={labels}
      onMaxCandidates={onMaxCandidates}
      onPreview={onPreview}
      onPlay={onPlay}
    />,
  );

  expect(rows()).toHaveLength(2);
  // White to play: Black's 30% and −2.5 read as White's 70.0 and +2.5.
  const cells = (row: HTMLElement) => [...row.children].filter((cell) => !cell.classList.contains("candidate-share")).map((cell) => cell.textContent);
  expect(cells(rows()[0]!)).toEqual(["1", "P0", "70.0", "600", "+2.5"]);
  expect(rows()[0]!.dataset.best).toBe("true");
  // The share of the search is the bar behind the row and the tooltip.
  expect(rows()[1]!.title).toBe("P1 · 占比 30.0%");
  expect((rows()[1]!.querySelector(".candidate-share") as HTMLElement).style.width).toBe("30%");

  fireEvent.mouseEnter(rows()[1]!);
  expect(onPreview).toHaveBeenLastCalledWith({ x: 2, y: 3 });
  fireEvent.click(rows()[1]!);
  expect(onPlay).toHaveBeenCalledWith({ x: 2, y: 3 });
  fireEvent.mouseLeave(within(screen.getByTestId("candidate-table")).getByRole("list"));
  expect(onPreview).toHaveBeenLastCalledWith(null);

  // The candidate count sits in the list's header.
  fireEvent.change(screen.getByTestId("max-candidates"), { target: { value: "361" } });
  expect(onMaxCandidates).toHaveBeenCalledWith(361);
  expect(within(screen.getByTestId("max-candidates")).getByText(zh.board.candidatesAll)).toBeTruthy();
});

test("the candidate table explains that it waits for analysis", () => {
  render(
    <CandidateTable
      analysis={null}
      toPlay={null}
      maxCandidates={10}
      candidateLimits={LIMITS}
      labels={labels}
      onMaxCandidates={vi.fn()}
      onPreview={vi.fn()}
      onPlay={vi.fn()}
    />,
  );
  expect(screen.queryAllByTestId("candidate-row")).toHaveLength(0);
  expect(screen.getByText(zh.table.empty)).toBeTruthy();
});

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

test("the move tree draws every node, marks the current one and jumps on click", () => {
  const onJump = vi.fn();
  const { container } = render(
    <MoveTree tree={tree} current={2} line={[0, 1, 2, 3]} boardSize={19} variationCount={1} labels={zh.tree} onJump={onJump} />,
  );
  const nodes = screen.getAllByTestId("tree-node");
  expect(nodes.map((node) => node.dataset.id)).toEqual(["0", "1", "2", "3", "4"]);
  expect(nodes.find((node) => node.dataset.current === "true")?.dataset.id).toBe("2");
  expect(nodes[1]?.querySelector("title")?.textContent).toBe("1 · Q16");
  // Node 4 is off the displayed line, so its stone is dimmed.
  expect(nodes[4]?.querySelector(".tree-stone")?.classList.contains("off-line")).toBe(true);
  expect(container.querySelectorAll(".tree-edge.lit")).toHaveLength(3);

  fireEvent.click(nodes[4]!);
  expect(onJump).toHaveBeenCalledWith(4);
  fireEvent.click(nodes[2]!);
  expect(onJump).toHaveBeenCalledTimes(1);
});

test("the info panel shows captures, the winrate bar and the last move's change", () => {
  const points: GraphPointView[] = [
    { node_id: 0, move_number: 0, summary: { winrate: 0.5, score_lead: 0.5, visits: 10 } },
    { node_id: 1, move_number: 1, summary: { winrate: 0.62, score_lead: 2.4, visits: 10 } },
  ];
  const board = {
    width: 19,
    height: 19,
    stones: [],
    last_move: { type: "Play", data: { color: "Black", point: { x: 3, y: 3 } } },
    to_play: "White",
    move_number: 1,
    captured_black: 2,
    captured_white: 0,
  } as const;
  render(
    <InfoPanel
      board={{ ...board, stones: [] }}
      black={{ name: "柯洁", rank: "9段" }}
      white={{ name: null, rank: null }}
      points={points}
      currentIndex={1}
      labels={zh.info}
    />,
  );

  expect(screen.getByText("提子 2")).toBeTruthy();
  expect(screen.getByText("柯洁")).toBeTruthy();
  expect(screen.getByText("9段")).toBeTruthy();
  // A missing name falls back to the colour.
  expect(screen.getByText("白方")).toBeTruthy();
  expect(screen.getByTestId("winrate-bar").dataset.black).toBe("62.0");
  expect(screen.getByTestId("lead-line").textContent).toBe("黑领先 2.4 目最后一手 (#1)：+12.0% +1.9 目");
});

test("the player rows show the match rate once the strength estimate has one", () => {
  const board: BoardView = {
    width: 19, height: 19, stones: [], last_move: null, to_play: "Black", move_number: 0, captured_black: 0, captured_white: 0,
  };
  const { rerender } = render(
    <InfoPanel board={board} black={{ name: "甲", rank: null, match: 0.736 }} white={{ name: "乙", rank: null, match: null }} points={[]} currentIndex={0} labels={zh.info} />,
  );
  expect(screen.getByTestId("player-match-black").textContent).toBe("吻合 74%");
  expect(screen.getByTestId("player-match-black").getAttribute("title")).toBe(zh.info.matchHint);
  expect(screen.queryByTestId("player-match-white")).toBeNull();

  rerender(<InfoPanel board={board} black={{ name: "甲", rank: null }} white={{ name: "乙", rank: null }} points={[]} currentIndex={0} labels={zh.info} />);
  expect(screen.queryByTestId("player-match-black")).toBeNull();
});
