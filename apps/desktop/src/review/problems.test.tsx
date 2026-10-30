import { act, cleanup, fireEvent, render, renderHook, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { ProblemMovesQuery, ProblemMovesView, ProblemMoveView } from "@/api/backend";
import zh from "../locales/zh-CN.json";
import { ProblemList } from "./ProblemList";
import { useProblemMoves } from "./useProblemMoves";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

function entry(nodeId: number, move: number, color: "Black" | "White", rank: "Inaccuracy" | "Mistake" | "Blunder", winrate: number, score: number): ProblemMoveView {
  return {
    node_id: nodeId,
    move_number: move,
    color,
    coordinate: `Q${move}`,
    loss: { winrate_loss: winrate, score_loss: score, rank, is_best: false, candidate_rank: 3, visits: 1000, before_visits: 1000 },
  };
}

function view(entries: ProblemMoveView[]): ProblemMovesView {
  return {
    revision: 4,
    settings: { mode: "Auto", winrate_thresholds: [1, 3, 6, 12, 24], score_thresholds: [0.5, 1.5, 3, 6, 12] },
    query: { minimum_rank: "Inaccuracy", color: null, sort: "LossDesc" },
    entries,
    error_bar: [],
    current_index: 0,
  };
}

function list(props: Partial<Parameters<typeof ProblemList>[0]> = {}) {
  const handlers = { onColor: vi.fn(), onSort: vi.fn(), onJump: vi.fn() };
  render(
    <ProblemList view={null} current={-1} color={null} sort="LossDesc" analysed labels={zh.problems} {...handlers} {...props} />,
  );
  return handlers;
}

test("each problem shows its move, player, coordinate and losses; a click jumps", () => {
  const { onJump } = list({
    view: view([entry(37, 37, "Black", "Blunder", 24.6, 6.25), entry(52, 52, "White", "Inaccuracy", 3.2, 0.8)]),
    current: 52,
  });
  const rows = screen.getAllByTestId("problem-row");
  expect(rows.map((row) => row.textContent)).toEqual(["第 37 手Q37−24.6%−6.3 目", "第 52 手Q52−3.2%−0.8 目"]);
  expect(rows[0]?.dataset.rank).toBe("Blunder");
  expect(rows[0]?.title).toBe("恶手");
  expect(rows[1]?.getAttribute("aria-current")).toBe("step");
  fireEvent.click(rows[0]!);
  expect(onJump).toHaveBeenCalledWith(37);
});

test("the player filter and the sort order are passed up", () => {
  const { onColor, onSort } = list({ view: view([]) });
  expect(screen.getByTestId("problems-all").getAttribute("aria-checked")).toBe("true");
  fireEvent.click(screen.getByTestId("problems-white"));
  expect(onColor).toHaveBeenCalledWith("White");
  fireEvent.change(screen.getByTestId("problems-sort"), { target: { value: "MoveAsc" } });
  expect(onSort).toHaveBeenCalledWith("MoveAsc");
});

test("the empty list explains whether analysis is missing or nothing reached the threshold", () => {
  list({ view: view([]), analysed: false });
  expect(screen.getByText(zh.problems.noAnalysis)).toBeTruthy();
  cleanup();
  list({ view: view([]), analysed: true });
  expect(screen.getByText(zh.problems.empty)).toBeTruthy();
});

const QUERY: ProblemMovesQuery = { minimum_rank: "Inaccuracy", color: null, sort: "LossDesc" };

test("the list refreshes at once for documents and progress, at most every 800ms for live analysis", async () => {
  vi.useFakeTimers();
  const backend = { problemMoves: vi.fn(async () => view([])) };
  const { rerender } = renderHook((signals) => useProblemMoves(backend, QUERY, signals), {
    initialProps: { revision: 1, progress: 0, live: 0 },
  });
  await act(async () => {});
  expect(backend.problemMoves).toHaveBeenCalledTimes(1);

  rerender({ revision: 2, progress: 0, live: 0 });
  rerender({ revision: 2, progress: 5, live: 0 });
  await act(async () => {});
  expect(backend.problemMoves).toHaveBeenCalledTimes(3);

  // Twenty live updates inside one window cause a single read.
  for (let visits = 1; visits <= 20; visits++) rerender({ revision: 2, progress: 5, live: visits * 50 });
  await act(async () => vi.advanceTimersByTime(800));
  expect(backend.problemMoves).toHaveBeenCalledTimes(4);
});

test("only the newest request's reply is shown", async () => {
  const replies: Array<(value: ProblemMovesView) => void> = [];
  const backend = { problemMoves: vi.fn(() => new Promise<ProblemMovesView>((resolve) => replies.push(resolve))) };
  const { result, rerender } = renderHook((signals) => useProblemMoves(backend, QUERY, signals), {
    initialProps: { revision: 1, progress: 0, live: 0 },
  });
  rerender({ revision: 2, progress: 0, live: 0 });
  await act(async () => replies[1]?.(view([entry(9, 9, "Black", "Mistake", 8, 2)])));
  await act(async () => replies[0]?.(view([])));
  expect(result.current?.entries.map((item) => item.node_id)).toEqual([9]);
});
