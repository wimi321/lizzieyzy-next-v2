import { beforeEach, expect, test, vi } from "vitest";
import type { CommandError, TencentPage } from "./backend";

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, Channel: class {} }));

beforeEach(() => {
  vi.resetModules();
  ipc.invoke.mockReset();
});

test("online kifu commands preserve inputs and return Rust results", async () => {
  const api = await import("./backend");
  const page: TencentPage = { games: [], next_cursor: null };
  for (const [call, command, args, result] of [
    [() => api.tencentSearch("fixture-query"), "tencent_search", { query: "fixture-query" }, page],
    [() => api.tencentNextPage("fixture-query", "fixture-cursor"), "tencent_next_page", { query: "fixture-query", cursor: "fixture-cursor" }, page],
    [() => api.tencentOpenGame("fixture-game"), "tencent_open_game", { chessid: "fixture-game" }, 7],
    [api.tencentClearHistory, "tencent_clear_history", undefined, undefined],
    [() => api.openUrl(' "https://example.invalid/fixture.sgf" '), "open_url", { url: ' "https://example.invalid/fixture.sgf" ' }, 8],
  ] as const) {
    ipc.invoke.mockResolvedValueOnce(result);
    expect(await call()).toBe(result);
    expect(ipc.invoke).toHaveBeenLastCalledWith(...(args === undefined ? [command] : [command, args]));
  }
  const error: CommandError = { type: "Tencent", data: { type: "Api", data: { result: 7, ret: null, message: "fixture rejection" } } };
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(api.tencentSearch("fixture-query")).rejects.toBe(error);
});

test("all online mock commands consume scripts and reject unscripted work", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const page: TencentPage = { games: [{ id: "fixture-game", black: { rank: { type: "Professional", data: 2 } } }], next_cursor: "fixture-cursor" };
  mock.setScript({ commands: [
    { command: "tencentSearch", result: page },
    { command: "tencentNextPage", result: {} },
    { command: "tencentOpenGame", result: 7 },
    { command: "tencentClearHistory", result: undefined },
    { command: "openUrl", result: 8 },
  ] });
  const found = await mock.tencentSearch("fixture-query");
  expect(found.next_cursor).toBe("fixture-cursor");
  expect(found.games?.[0]?.id).toBe("fixture-game");
  expect(found.games?.[0]?.start_time).toBeNull();
  expect(found.games?.[0]?.move_count).toBe(0);
  expect(found.games?.[0]?.black?.rank).toEqual({ type: "Professional", data: 2 });
  expect(found.games?.[0]?.white?.nickname).toBe("");
  expect(found.games?.[0]?.white?.rank).toEqual({ type: "Kyu", data: 18 });
  expect(found.games?.[0]?.result?.kind).toBe("Other");
  const last = await mock.tencentNextPage("fixture-query", "fixture-cursor");
  expect(last.games).toEqual([]);
  expect(last.next_cursor).toBeNull();
  expect(await mock.tencentOpenGame("fixture-game")).toBe(7);
  await mock.tencentClearHistory();
  expect(await mock.openUrl("https://example.invalid/fixture.sgf")).toBe(8);
  mock.setScript({ commands: [{ command: "settings", result: { version: 1, engine: null, pondering: false } }] });
  await expect(mock.tencentSearch("fixture-query")).rejects.toThrow("Expected mock command settings");
  expect((await mock.settings()).pondering).toBe(false);
  for (const call of [
    () => mock.tencentSearch("fixture-query"),
    () => mock.tencentNextPage("fixture-query", "fixture-cursor"),
    () => mock.tencentOpenGame("fixture-game"),
    mock.tencentClearHistory,
    () => mock.openUrl("https://example.invalid/fixture.sgf"),
  ]) await expect(call()).rejects.toThrow("Expected mock command");
});

test("online mock preserves typed failures and supplies only missing history", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const unsupported: CommandError = { type: "OpenUrl", data: { type: "UnsupportedYike" } };
  const network: CommandError = { type: "Tencent", data: { type: "Request", data: { type: "Network" } } };
  mock.setScript({ commands: [
    { command: "settings", result: { version: 1, engine: null, pondering: true } },
    { command: "settings", result: { version: 1, engine: null, pondering: true, tencent_history: ["fixture-query"] } },
    { command: "openUrl", result: 0, error: unsupported },
    { command: "tencentSearch", result: {}, error: network },
  ] });
  expect((await mock.settings()).tencent_history).toEqual([]);
  expect((await mock.settings()).tencent_history).toEqual(["fixture-query"]);
  await expect(mock.openUrl("https://yikeweiqi.com/fixture")).rejects.toEqual(unsupported);
  await expect(mock.tencentSearch("fixture-query")).rejects.toEqual(network);
});

test("online mock defaults preserve explicitly scripted game fields", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const page: TencentPage = { games: [{
    id: "fixture-game", start_time: 0, move_count: 0,
    black: { uid: "fixture-black", nickname: "fixture black", rank: { type: "Kyu", data: 4 } },
    white: { uid: "fixture-white", nickname: "fixture white", rank: { type: "Dan", data: 7 } },
    result: { winner: "White", kind: "Timeout", score: null, unit: null },
  }], next_cursor: null };
  mock.setScript({ commands: [{ command: "tencentSearch", result: page }] });
  expect(await mock.tencentSearch("fixture-query")).toEqual(page);
});
