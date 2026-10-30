import { beforeEach, expect, test, vi } from "vitest";
import type { MoveView, VariationPreviewView, VariationTreeView } from "./backend";

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, Channel: class {} }));

beforeEach(() => {
  vi.resetModules();
  ipc.invoke.mockReset();
});

const moves: MoveView[] = [
  { type: "Play", data: { color: "Black", point: { x: 0, y: 1 } } },
  { type: "Pass", data: { color: "White" } },
];
const tree: VariationTreeView = {
  revision: 4,
  root: 0,
  nodes: [
    { id: 0, parent: null, children: [1], move_number: 0, mv: null, move_error: null },
    { id: 1, parent: 0, children: [], move_number: 1, mv: moves[0]!, move_error: null },
  ],
};
const preview: VariationPreviewView = {
  revision: 7,
  node_id: 0,
  board: {
    width: 2, height: 2, stones: [null, null, "Black", null],
    last_move: moves[1]!, to_play: "Black", move_number: 2,
    captured_black: 1, captured_white: 0,
  },
  applied_moves: moves,
  stones: [{ point: { x: 0, y: 1 }, color: "Black", move_number: 1 }],
  stop_reason: "occupied point",
};

test("tree and PV commands forward typed requests and preserve core results and errors", async () => {
  const backend = await import("./backend");
  ipc.invoke.mockResolvedValueOnce(tree);
  expect(await backend.variationTree()).toEqual(tree);
  expect(ipc.invoke).toHaveBeenLastCalledWith("variation_tree");

  ipc.invoke.mockResolvedValueOnce(preview);
  expect(await backend.previewVariation(0, moves)).toEqual(preview);
  expect(ipc.invoke).toHaveBeenLastCalledWith("preview_variation", { nodeId: 0, moves });

  ipc.invoke.mockRejectedValueOnce("missing node");
  await expect(backend.previewVariation(999, [])).rejects.toBe("missing node");
  ipc.invoke.mockRejectedValueOnce("closed");
  await expect(backend.variationTree()).rejects.toBe("closed");
});

test("mock returns scripted trees and captured PV boards without emitting edits", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>(
    "../../e2e/mock-backend",
  );
  mock.setScript({ commands: [
    { command: "variationTree", result: tree },
    { command: "previewVariation", result: preview },
  ] });
  const listener = vi.fn();
  const subscription = await mock.subscribe(listener);
  expect(await mock.variationTree()).toEqual(tree);
  expect(await mock.previewVariation(0, moves)).toEqual(preview);
  expect(listener).not.toHaveBeenCalled();
  expect(mock.recordedCalls()).toEqual([
    { command: "variationTree", args: [] },
    { command: "previewVariation", args: [0, moves] },
  ]);
  await mock.unsubscribe(subscription);
});
