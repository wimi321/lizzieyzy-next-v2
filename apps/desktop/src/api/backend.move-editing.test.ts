import { beforeEach, expect, test, vi } from "vitest";
import { boardView, documentView } from "../../e2e/fixtures";
import type { AppEvent, MoveEditingCommands, MoveView, StoneOriginView } from "./backend";

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, Channel: class {} }));

beforeEach(() => {
  vi.resetModules();
  ipc.invoke.mockReset();
});

const point = { x: 2, y: 3 };
const to = { x: 4, y: 5 };
const mv: MoveView = { type: "Play", data: { color: "White", point } };
const pass: MoveView = { type: "Pass", data: { color: "Black" } };
const moves = [mv, pass];

function mutations(backend: Omit<MoveEditingCommands, "stoneOrigin">) {
  return [
    { run: () => backend.playColored(mv), wire: "play_colored", name: "playColored" as const, args: { mv }, recorded: [mv] },
    { run: () => backend.playColored(pass), wire: "play_colored", name: "playColored" as const, args: { mv: pass }, recorded: [pass] },
    { run: () => backend.insertMove(mv), wire: "insert_move", name: "insertMove" as const, args: { mv }, recorded: [mv] },
    { run: () => backend.switchStone(point), wire: "switch_stone", name: "switchStone" as const, args: { point }, recorded: [point] },
    { run: () => backend.deleteStone(point), wire: "delete_stone", name: "deleteStone" as const, args: { point }, recorded: [point] },
    { run: () => backend.moveStone(point, to), wire: "move_stone", name: "moveStone" as const, args: { from: point, to }, recorded: [point, to] },
    { run: () => backend.addVariation(moves), wire: "add_variation", name: "addVariation" as const, args: { moves }, recorded: [moves] },
    { run: () => backend.startTrial(), wire: "start_trial", name: "startTrial" as const, recorded: [] },
    { run: () => backend.endTrial(), wire: "end_trial", name: "endTrial" as const, recorded: [] },
  ];
}

test("move editing API forwards all mutations, revisions and failures unchanged", async () => {
  const backend = await import("./backend");
  for (const [index, command] of mutations(backend).entries()) {
    ipc.invoke.mockResolvedValueOnce(index + 1);
    expect(await command.run()).toBe(index + 1);
    expect(ipc.invoke.mock.calls.at(-1)).toEqual(command.args ? [command.wire, command.args] : [command.wire]);
    const failure = { type: "App", data: `invalid ${command.wire}` };
    ipc.invoke.mockRejectedValueOnce(failure);
    await expect(command.run()).rejects.toBe(failure);
  }
});

test("stone origin API forwards source, review parent, empty results and errors", async () => {
  const backend = await import("./backend");
  const origin: StoneOriginView = { revision: 7, node_id: 2, move_number: 1, review_parent: 0 };
  for (const value of [origin, null]) {
    ipc.invoke.mockResolvedValueOnce(value);
    expect(await backend.stoneOrigin(point)).toEqual(value);
    expect(ipc.invoke.mock.calls.at(-1)).toEqual(["stone_origin", { point }]);
  }
  const failure = { type: "App", data: "outside board" };
  ipc.invoke.mockRejectedValueOnce(failure);
  await expect(backend.stoneOrigin(point)).rejects.toBe(failure);
});

test("mock move mutations require scripts, preserve explicit trial state and propagate errors", async () => {
  const mock = await import("../../e2e/mock-backend");
  const view = documentView(boardView(9, 9, []), 0);
  view.trial = true;
  const changed: AppEvent = { type: "DocumentChanged", data: { revision: 4, view } };
  const requests = mutations(mock);
  mock.setScript({ commands: requests.map((command, index) => ({
    command: command.name, result: index + 1, events: index === 0 ? [changed] : [],
  })) });
  const listener = vi.fn();
  const id = await mock.subscribe(listener);
  try {
    for (const [index, command] of requests.entries()) expect(await command.run()).toBe(index + 1);
    expect(mock.recordedCalls()).toEqual(requests.map((command) => ({ command: command.name, args: command.recorded })));
    expect(listener).toHaveBeenCalledTimes(1);
    expect(listener.mock.calls[0]![0].data.view.trial).toBe(true);
    for (const command of requests) await expect(command.run()).rejects.toThrow("Expected mock command (none)");
    const error = { type: "App" as const, data: "invalid operation" };
    for (const command of requests) {
      mock.setScript({ commands: [{ command: command.name, result: 3, error }] });
      await expect(command.run()).rejects.toEqual(error);
    }
    expect(listener).toHaveBeenCalledTimes(1);
  } finally {
    await mock.unsubscribe(id);
  }
});

test("mock origin defaults to null without consuming mutations and replays supplied query results", async () => {
  const mock = await import("../../e2e/mock-backend");
  const origin: StoneOriginView = { revision: 8, node_id: 3, move_number: 2, review_parent: 1 };
  mock.setScript({ commands: [
    { command: "startTrial", result: 8 },
    { command: "stoneOrigin", result: origin },
    { command: "stoneOrigin", result: null, error: { type: "App", data: "bad position" } },
  ] });
  expect(await mock.stoneOrigin(point)).toBeNull();
  expect(mock.recordedCalls()).toEqual([]);
  expect(await mock.startTrial()).toBe(8);
  expect(await mock.stoneOrigin(point)).toEqual(origin);
  await expect(mock.stoneOrigin(point)).rejects.toEqual({ type: "App", data: "bad position" });
  expect(await mock.stoneOrigin(point)).toBeNull();
});

test("legacy document scripts receive optional trial false without changing their data shape", async () => {
  const mock = await import("../../e2e/mock-backend");
  const view = documentView(boardView(9, 9, []), 0);
  delete view.trial;
  const initial: AppEvent = { type: "DocumentChanged", data: { revision: 1, view } };
  mock.setScript({ initialEvents: [initial] });
  const listener = vi.fn();
  const id = await mock.subscribe(listener);
  try {
    const event = listener.mock.calls[0]![0] as AppEvent;
    expect(event).toEqual(initial);
    if (event.type !== "DocumentChanged") throw new Error("expected document");
    expect(event.data.view.trial).toBe(false);
    expect(view.trial).toBeUndefined();
    mock.pushEvents([{ type: "DocumentChanged", data: { revision: 2, view: { ...view, trial: true } } }]);
    expect(listener.mock.calls.at(-1)![0].data.view.trial).toBe(true);
  } finally {
    await mock.unsubscribe(id);
  }
});

