import { beforeEach, expect, test, vi } from "vitest";
import { boardView, documentView } from "../../e2e/fixtures";
import type { AppEvent, GameInfoEdit, NewGameOptions, VariationTreeView } from "./backend";

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, Channel: class {} }));

beforeEach(() => {
  vi.resetModules();
  ipc.invoke.mockReset();
});

const options: NewGameOptions = {
  width: 13, height: 13, komi: 0.5, handicap: 4, rules: "japanese",
  black_name: "黑方", black_rank: "3d", white_name: null, white_rank: "1d",
};
const info: GameInfoEdit = {
  black_name: "甲", black_rank: null, white_name: "乙", white_rank: "2d",
  komi: 6.5, rules: "chinese", result: "B+R", date: "2026-11-11", event: "友谊赛",
};

test("edit API forwards every command argument and preserves revisions and errors", async () => {
  const backend = await import("./backend");
  const requests = [
    { run: () => backend.setComment("注释"), wire: "set_comment", args: { text: "注释" } },
    { run: () => backend.deleteNode(17), wire: "delete_node", args: { nodeId: 17 } },
    { run: () => backend.deleteBranch(18), wire: "delete_branch", args: { nodeId: 18 } },
    { run: () => backend.promoteToMain(19), wire: "promote_to_main", args: { nodeId: 19 } },
    { run: () => backend.newGame(options), wire: "new_game", args: { options } },
    { run: () => backend.setGameInfo(info), wire: "set_game_info", args: { info } },
  ];
  for (const [index, request] of requests.entries()) {
    ipc.invoke.mockResolvedValueOnce(index + 1);
    expect(await request.run()).toBe(index + 1);
    expect(ipc.invoke).toHaveBeenLastCalledWith(request.wire, request.args);
    const failure = { type: "App", data: `invalid ${request.wire}` };
    ipc.invoke.mockRejectedValueOnce(failure);
    await expect(request.run()).rejects.toBe(failure);
  }
  ipc.invoke.mockResolvedValueOnce(["/games/new.sgf", "/games/old.sgf"]);
  expect(await backend.recentFiles()).toEqual(["/games/new.sgf", "/games/old.sgf"]);
  expect(ipc.invoke).toHaveBeenLastCalledWith("recent_files");
  ipc.invoke.mockResolvedValueOnce(undefined);
  expect(await backend.clearRecentFiles()).toBeUndefined();
  expect(ipc.invoke).toHaveBeenLastCalledWith("clear_recent_files");
});

test("mock edits require scripts, record arguments and only replay scripted state", async () => {
  const mock = await import("../../e2e/mock-backend");
  const view = documentView(boardView(9, 9, []), 0);
  view.comment = "scripted comment";
  view.modified = true;
  const changed: AppEvent = { type: "DocumentChanged", data: { revision: 10, view } };
  mock.setScript({ commands: [
    { command: "setComment", result: 10, events: [changed] },
    { command: "deleteNode", result: 11 },
    { command: "deleteBranch", result: 12 },
    { command: "promoteToMain", result: 13 },
    { command: "newGame", result: 14 },
    { command: "setGameInfo", result: 15 },
    { command: "clearRecentFiles", result: undefined },
  ] });
  const listener = vi.fn();
  const id = await mock.subscribe(listener);
  try {
    expect(await mock.recentFiles()).toEqual([]);
    expect(mock.recordedCalls()).toEqual([]);
    expect(await mock.setComment("input is not interpreted")).toBe(10);
    expect(listener).toHaveBeenCalledTimes(1);
    expect(listener.mock.calls[0]![0].data.view.comment).toBe("scripted comment");
    expect(await mock.deleteNode(5)).toBe(11);
    expect(await mock.deleteBranch(6)).toBe(12);
    expect(await mock.promoteToMain(7)).toBe(13);
    expect(await mock.newGame(options)).toBe(14);
    expect(await mock.setGameInfo(info)).toBe(15);
    await mock.clearRecentFiles();
    expect(mock.recordedCalls()).toEqual([
      { command: "setComment", args: ["input is not interpreted"] },
      { command: "deleteNode", args: [5] },
      { command: "deleteBranch", args: [6] },
      { command: "promoteToMain", args: [7] },
      { command: "newGame", args: [options] },
      { command: "setGameInfo", args: [info] },
      { command: "clearRecentFiles", args: [] },
    ]);
    for (const run of [
      () => mock.setComment(""), () => mock.deleteNode(0), () => mock.deleteBranch(0),
      () => mock.promoteToMain(0), () => mock.newGame(options),
      () => mock.setGameInfo(info), () => mock.clearRecentFiles(),
    ]) await expect(run()).rejects.toThrow("Expected mock command (none)");
    expect(listener).toHaveBeenCalledTimes(1);
  } finally {
    await mock.unsubscribe(id);
  }
});

test("recent-files query defaults without consuming commands and preserves scripted failures", async () => {
  const mock = await import("../../e2e/mock-backend");
  const error = { type: "App" as const, data: "settings are read-only" };
  mock.setScript({ commands: [
    { command: "recentFiles", result: ["/games/棋谱.sgf"] },
    { command: "clearRecentFiles", result: undefined, error },
    { command: "setComment", result: 1, error: "missing node" },
  ] });
  expect(await mock.recentFiles()).toEqual(["/games/棋谱.sgf"]);
  expect(await mock.recentFiles()).toEqual([]);
  await expect(mock.clearRecentFiles()).rejects.toEqual(error);
  await expect(mock.setComment("text")).rejects.toThrow("missing node");
  expect(mock.recordedCalls().map((call) => call.command)).toEqual([
    "recentFiles", "clearRecentFiles", "setComment",
  ]);
});

test("old document, settings and tree scripts receive safe new-field defaults", async () => {
  const mock = await import("../../e2e/mock-backend");
  const view = documentView(boardView(9, 9, []), 0);
  delete view.file_path;
  delete view.modified;
  delete view.tree.comment_revision;
  const tree: VariationTreeView = {
    revision: 4, root: 0,
    nodes: [{ id: 0, parent: null, children: [], move_number: 0, mv: null, move_error: null }],
  };
  const marked: VariationTreeView = {
    ...tree, comment_revision: 7, nodes: [{ ...tree.nodes[0]!, has_comment: true }],
  };
  mock.setScript({
    initialEvents: [{ type: "DocumentChanged", data: { revision: 4, view } }],
    commands: [
      { command: "settings", result: { version: 1, engine: null, pondering: false } },
      { command: "variationTree", result: tree },
      { command: "variationTree", result: marked },
    ],
  });
  const listener = vi.fn();
  const id = await mock.subscribe(listener);
  try {
    const received = listener.mock.calls[0]![0].data.view;
    expect(received.file_path).toBeNull();
    expect(received.modified).toBe(false);
    expect(received.tree.comment_revision).toBe(0);
    expect((await mock.settings()).recent_files).toEqual([]);
    const defaulted = await mock.variationTree();
    expect(defaulted.comment_revision).toBe(0);
    expect(defaulted.nodes[0]!.has_comment).toBe(false);
    expect(defaulted).toEqual(tree);
    expect(await mock.variationTree()).toEqual(marked);
    mock.pushEvents([{ type: "DocumentChanged", data: {
      revision: 8,
      view: { ...view, file_path: "/games/saved.sgf", modified: true,
        tree: { ...view.tree, comment_revision: 8 } },
    } }]);
    const explicit = listener.mock.calls.at(-1)![0].data.view;
    expect(explicit.file_path).toBe("/games/saved.sgf");
    expect(explicit.modified).toBe(true);
    expect(explicit.tree.comment_revision).toBe(8);
    expect(tree.comment_revision).toBeUndefined();
    expect(tree.nodes[0]!.has_comment).toBeUndefined();
  } finally {
    await mock.unsubscribe(id);
  }
});
