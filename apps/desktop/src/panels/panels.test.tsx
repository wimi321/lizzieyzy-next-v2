import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { AnalysisView, CandidateView, GraphPointView } from "@/api/backend";
import zh from "../locales/zh-CN.json";
import { CandidateTable } from "./CandidateTable";
import { InfoPanel } from "./InfoPanel";
import { MoveList } from "./MoveList";

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

test("the candidate table lists engine order, mover values and visit shares", () => {
  const onPreview = vi.fn();
  const onPlay = vi.fn();
  // Out of order on purpose; 600 + 300 + 100 visits in total.
  const view = analysis([candidate(1, 2, 0.4, 300, -1), candidate(0, 1, 0.3, 600, -2.5), candidate(2, 5, 0.45, 100, 0.5)]);
  render(<CandidateTable analysis={view} toPlay="White" maxCandidates={2} labels={zh.table} onPreview={onPreview} onPlay={onPlay} />);

  expect(rows()).toHaveLength(2);
  // White to play: Black's 30% and −2.5 read as White's 70.0 and +2.5.
  expect(within(rows()[0]!).getAllByRole("cell").map((cell) => cell.textContent)).toEqual(["1", "70.0", "600", "60.0", "+2.5"]);
  expect(rows()[0]!.dataset.best).toBe("true");
  expect(within(rows()[1]!).getAllByRole("cell")[3]?.textContent).toBe("30.0");

  fireEvent.mouseEnter(rows()[1]!);
  expect(onPreview).toHaveBeenLastCalledWith({ x: 2, y: 3 });
  fireEvent.click(rows()[1]!);
  expect(onPlay).toHaveBeenCalledWith({ x: 2, y: 3 });
  fireEvent.mouseLeave(screen.getByRole("table"));
  expect(onPreview).toHaveBeenLastCalledWith(null);
});

test("the candidate table is empty without analysis", () => {
  render(<CandidateTable analysis={null} toPlay={null} maxCandidates={10} labels={zh.table} onPreview={vi.fn()} onPlay={vi.fn()} />);
  expect(screen.queryAllByTestId("candidate-row")).toHaveLength(0);
});

function line(count: number, analysed: number[] = []): GraphPointView[] {
  return Array.from({ length: count }, (_, index) => ({
    node_id: 100 + index,
    move_number: index,
    summary: analysed.includes(index) ? { winrate: 0.5, score_lead: 0, visits: 10 } : null,
  }));
}

test("the move list marks the current move and jumps to others", () => {
  const onJump = vi.fn();
  render(<MoveList points={line(5, [1])} currentIndex={2} variationCount={3} labels={zh.tree} onJump={onJump} />);
  const buttons = screen.getAllByRole("button");
  expect(buttons).toHaveLength(5);
  expect(buttons[2]?.getAttribute("aria-current")).toBe("step");
  expect(buttons[2]?.textContent).toContain("3 个变化");
  expect(buttons[1]?.dataset.analysed).toBe("true");

  fireEvent.click(buttons[4]!);
  expect(onJump).toHaveBeenCalledWith(104);
  fireEvent.click(buttons[2]!);
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
  render(<InfoPanel board={{ ...board, stones: [] }} points={points} currentIndex={1} labels={zh.info} />);

  expect(screen.getByText("提子 2")).toBeTruthy();
  expect(screen.getByTestId("winrate-bar").dataset.black).toBe("62.0");
  expect(screen.getByTestId("lead-line").textContent).toBe("黑领先 2.4 目最后一手 (#1)：+12.0% +1.9 目");
});
