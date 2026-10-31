import { beforeEach, expect, test, vi } from "vitest";
import type { BoardSyncOptions, BoardSyncStatus, WireAppEvent } from "./backend";

interface FakeChannel { onmessage: (event: WireAppEvent) => void }
const ipc = vi.hoisted(() => ({ invoke: vi.fn(), channels: [] as FakeChannel[] }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: ipc.invoke,
  Channel: class implements FakeChannel {
    onmessage: (event: WireAppEvent) => void = () => {};
    constructor() { ipc.channels.push(this); }
  },
}));

beforeEach(() => {
  vi.resetModules();
  ipc.invoke.mockReset();
  ipc.channels.length = 0;
});

const defaults: BoardSyncOptions = { always_sync: true, always_goto_last: false, language: "Cn" };
const syncing: BoardSyncStatus = {
  phase: "Syncing", always_sync: true, always_goto_last: false, remote_move_number: null, error: null,
};
const stopped: BoardSyncStatus = { ...syncing, phase: "Stopped" };

test("board sync IPC forwards options, controls and typed errors without selecting a program", async () => {
  const backend = await import("./backend");
  const custom: BoardSyncOptions = { always_sync: false, always_goto_last: true, language: "En" };
  const cases: [() => Promise<unknown>, string, Record<string, unknown> | undefined, unknown][] = [
    [() => backend.startBoardSync(), "start_board_sync", { options: defaults }, syncing],
    [() => backend.startBoardSync(custom), "start_board_sync", { options: custom }, syncing],
    [() => backend.stopBoardSync(), "stop_board_sync", undefined, stopped],
    [() => backend.boardSyncStatus(), "board_sync_status", undefined, stopped],
  ];
  for (const [call, name, args, result] of cases) {
    ipc.invoke.mockResolvedValueOnce(result);
    expect(await call()).toEqual(result);
    expect(ipc.invoke).toHaveBeenLastCalledWith(...(args ? [name, args] : [name]));
  }
  for (const data of [
    { type: "UnsupportedPlatform" },
    { type: "MissingExecutable", data: { path: "resources/readboard/readboard.exe" } },
  ]) {
    const error = { type: "BoardSync", data };
    ipc.invoke.mockRejectedValueOnce(error);
    await expect(backend.startBoardSync()).rejects.toBe(error);
  }
});

test("sync, review and document listeners share one channel and separate event types", async () => {
  const backend = await import("./backend");
  ipc.invoke.mockImplementation(async (command) => {
    if (command === "subscribe") {
      ipc.channels[0]!.onmessage({ type: "BoardSyncStatusChanged", data: syncing });
      return 33;
    }
  });
  const ui = vi.fn();
  const review = vi.fn();
  const sync = vi.fn();
  const ids = await Promise.all([
    backend.subscribe(ui), backend.subscribeWholeGameProgress(review), backend.subscribeBoardSyncStatus(sync),
  ]);
  expect(ipc.channels).toHaveLength(1);
  const engine = { type: "EngineStatusChanged", data: { type: "Stopped" } } as const;
  ipc.channels[0]!.onmessage(engine);
  expect(sync.mock.calls).toEqual([[syncing]]);
  expect(ui.mock.calls).toEqual([[engine]]);
  expect(review).not.toHaveBeenCalled();
  await backend.unsubscribe(ids[0]!);
  await backend.unsubscribe(ids[1]!);
  expect(ipc.invoke).toHaveBeenCalledTimes(1);
  ipc.channels[0]!.onmessage({ type: "BoardSyncStatusChanged", data: stopped });
  expect(sync).toHaveBeenLastCalledWith(stopped);
  await backend.unsubscribe(ids[2]!);
  expect(ipc.invoke).toHaveBeenLastCalledWith("unsubscribe", { id: 33 });
  ipc.channels[0]!.onmessage({ type: "BoardSyncStatusChanged", data: syncing });
  expect(sync).toHaveBeenCalledTimes(2);
});

test("mock replays sync status, command results and typed failures without board logic", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const missing = { type: "BoardSync", data: { type: "MissingExecutable", data: { path: "readboard.exe" } } } as const;
  mock.setScript({ commands: [
    { command: "startBoardSync", result: syncing, events: [{ type: "BoardSyncStatusChanged", data: syncing }] },
    { command: "stopBoardSync", result: stopped, events: [{ type: "BoardSyncStatusChanged", data: stopped }] },
    { command: "startBoardSync", result: syncing, error: missing },
  ] });
  const ui = vi.fn();
  const sync = vi.fn();
  const uiId = await mock.subscribe(ui);
  const syncId = await mock.subscribeBoardSyncStatus(sync);
  expect(await mock.boardSyncStatus()).toEqual(stopped);
  expect(await mock.startBoardSync()).toEqual(syncing);
  expect(await mock.boardSyncStatus()).toEqual(syncing);
  expect(await mock.stopBoardSync()).toEqual(stopped);
  await expect(mock.startBoardSync()).rejects.toEqual(missing);
  expect(ui).not.toHaveBeenCalled();
  expect(sync.mock.calls).toEqual([[syncing], [stopped]]);
  expect(mock.recordedCalls()).toEqual([
    { command: "startBoardSync", args: [defaults] },
    { command: "stopBoardSync", args: [] },
    { command: "startBoardSync", args: [defaults] },
  ]);
  await mock.unsubscribe(uiId);
  await mock.unsubscribe(syncId);
  mock.pushEvents([{ type: "BoardSyncStatusChanged", data: syncing }]);
  expect(sync).toHaveBeenCalledTimes(2);
});
