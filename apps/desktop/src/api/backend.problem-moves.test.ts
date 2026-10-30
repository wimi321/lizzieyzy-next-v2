import { beforeEach, expect, test, vi } from "vitest";
import type { MoveRankSettings, ProblemMovesQuery, ProblemMovesView } from "./backend";

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, Channel: class {} }));

beforeEach(() => {
  vi.resetModules();
  ipc.invoke.mockReset();
});

const settings: MoveRankSettings = {
  mode: "Combined", winrate_thresholds: [1, 3, 6, 12, 24], score_thresholds: [0.5, 1.5, 3, 6, 12],
};
const query: ProblemMovesQuery = { minimum_rank: "Mistake", color: "White", sort: "MoveAsc" };
const view: ProblemMovesView = {
  revision: 7, current_index: 1, settings, query,
  entries: [{
    node_id: 4, move_number: 2, color: "White", coordinate: "Q16",
    loss: { winrate_loss: 15, score_loss: 7, rank: "Mistake", is_best: false, candidate_rank: null, visits: 500, before_visits: 1000 },
  }],
  error_bar: [
    { node_id: 0, move_number: 0, mv: null, loss: null },
    { node_id: 3, move_number: 1, mv: { type: "Pass", data: { color: "Black" } }, loss: null },
    {
      node_id: 4, move_number: 2,
      mv: { type: "Play", data: { color: "White", point: { x: 15, y: 3 } } },
      loss: { winrate_loss: 15, score_loss: 7, rank: "Mistake", is_best: false, candidate_rank: null, visits: 500, before_visits: 1000 },
    },
  ],
};

test("problem move IPC forwards queries, snapshots, classification settings and errors", async () => {
  const backend = await import("./backend");
  ipc.invoke.mockResolvedValueOnce(view);
  expect(await backend.problemMoves(query)).toEqual(view);
  expect(ipc.invoke).toHaveBeenLastCalledWith("problem_moves", { query });
  ipc.invoke.mockResolvedValueOnce(view);
  expect(await backend.problemMoves()).toEqual(view);
  expect(ipc.invoke).toHaveBeenLastCalledWith("problem_moves", {
    query: { minimum_rank: "Inaccuracy", color: null, sort: "LossDesc" },
  });
  ipc.invoke.mockResolvedValueOnce(settings);
  expect(await backend.setMoveRankSettings(settings)).toEqual(settings);
  expect(ipc.invoke).toHaveBeenLastCalledWith("set_move_rank_settings", { settings });
  ipc.invoke.mockRejectedValueOnce("invalid thresholds");
  await expect(backend.setMoveRankSettings(settings)).rejects.toBe("invalid thresholds");
});

test("mock problem move snapshots remain scripted and preserve null loss and candidate rank", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const progress = { completed: 3, total: 3, visits: 500, status: { type: "Completed" } } as const;
  mock.setScript({ commands: [
    { command: "problemMoves", result: view, events: [{ type: "WholeGameProgress", data: progress }] },
    { command: "setMoveRankSettings", result: settings },
    { command: "problemMoves", result: { ...view, entries: [] } },
  ] });
  const updates = vi.fn();
  const id = await mock.subscribeWholeGameProgress(updates);
  const received = await mock.problemMoves(query);
  expect(received).toEqual(view);
  expect(updates).toHaveBeenLastCalledWith(progress);
  expect(await mock.setMoveRankSettings(settings)).toEqual(settings);
  received.error_bar[0]!.node_id = 99;
  expect(await mock.problemMoves()).toEqual({ ...view, entries: [] });
  expect(mock.recordedCalls()).toEqual([
    { command: "problemMoves", args: [query] },
    { command: "setMoveRankSettings", args: [settings] },
    { command: "problemMoves", args: [{ minimum_rank: "Inaccuracy", color: null, sort: "LossDesc" }] },
  ]);
  await mock.unsubscribe(id);
});
