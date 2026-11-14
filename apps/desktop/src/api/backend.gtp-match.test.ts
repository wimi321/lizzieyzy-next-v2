import { beforeEach, expect, test, vi } from "vitest";
import type { WireAppEvent } from "./backend";
interface FakeChannel { onmessage: (event: WireAppEvent) => void }
const ipc = vi.hoisted(() => ({ invoke: vi.fn(), channels: [] as FakeChannel[] }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, Channel: class implements FakeChannel {
  onmessage: (event: WireAppEvent) => void = () => {};
  constructor() { ipc.channels.push(this); }
} }));
beforeEach(() => { vi.resetModules(); ipc.invoke.mockReset(); ipc.channels.length = 0; });

test("GTP and match commands carry typed inputs and errors over IPC", async () => {
  const backend = await import("./backend");
  const entry = { id: "gtp-a", program: "/engine", args: ["--gtp"], init_commands: ["name"] };
  const setup = { a: { type: "Gtp" as const, data: { engine_id: "gtp-a", time: { type: "EngineDefault" as const } } } };
  const cases: [() => Promise<unknown>, string, object | undefined][] = [
    [() => backend.addGtpEngine(entry), "add_gtp_engine", { entry }],
    [() => backend.updateGtpEngine(entry), "update_gtp_engine", { entry }],
    [() => backend.removeGtpEngine("gtp-a"), "remove_gtp_engine", { id: "gtp-a" }],
    [() => backend.testGtpEngine("gtp-a"), "test_gtp_engine", { id: "gtp-a" }],
    [() => backend.gtpConsole("gtp-a"), "gtp_console", { engineId: "gtp-a" }],
    [() => backend.gtpConsoleSend("gtp-a", "name"), "gtp_console_send", { engineId: "gtp-a", text: "name" }],
    [() => backend.startMatch(setup), "start_match", { setup }],
    [backend.pauseMatch, "pause_match", undefined], [backend.resumeMatch, "resume_match", undefined],
    [backend.stopMatch, "stop_match", undefined], [backend.matchView, "match_view", undefined],
    [() => backend.setMatchGames(3), "set_match_games", { n: 3 }],
    [backend.matchPlayNow, "match_play_now", undefined],
    [() => backend.matchResign("White"), "match_resign", { color: "White" }],
    [() => backend.setMatchSideParams("B", 0.2, -0.5), "set_match_side_params", { side: "B", wrn: 0.2, pda: -0.5 }],
  ];
  for (const [call, name, args] of cases) {
    await call(); expect(ipc.invoke).toHaveBeenLastCalledWith(...(args ? [name, args] : [name]));
  }
  const error = { type: "Gtp", data: { type: "StateChangingCommand", data: { command: "play" } } };
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(backend.gtpConsoleSend("gtp-a", "play B D4")).rejects.toBe(error);
});

test("match and console events share one channel without entering legacy listeners", async () => {
  const backend = await import("./backend"); ipc.invoke.mockResolvedValue(71);
  const old = vi.fn(); const match = vi.fn(); const console = vi.fn();
  const ids = await Promise.all([backend.subscribe(old), backend.subscribeMatchUpdated(match), backend.subscribeGtpConsole(console)]);
  expect(ipc.channels).toHaveLength(1);
  ipc.channels[0]!.onmessage({ type: "MatchUpdated", data: { completed: 2 } });
  ipc.channels[0]!.onmessage({ type: "GtpConsoleLine", data: { text: "name" } });
  expect(old).not.toHaveBeenCalled(); expect(match).toHaveBeenCalledWith({ completed: 2 });
  expect(console).toHaveBeenCalledWith({ text: "name" });
  for (const id of ids) await backend.unsubscribe(id);
  expect(ipc.invoke).toHaveBeenLastCalledWith("unsubscribe", { id: 71 });
});

test("mock requires scripts, fills DTO defaults and preserves explicit fields", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  mock.setScript({ commands: [
    { command: "settings", result: { version: 1, engine: null, pondering: true } },
    { command: "startMatch", result: { completed: 4, a: { black: 2 } }, events: [
      { type: "MatchUpdated", data: { phase: "Paused" } }, { type: "GtpConsoleLine", data: { text: "hello" } },
    ] },
    { command: "gtpConsole", result: [{ text: "received" }] },
    { command: "testGtpEngine", result: { name: "Example" } },
  ] });
  const old=vi.fn(); const match=vi.fn(); const console=vi.fn();
  const ids=await Promise.all([mock.subscribe(old), mock.subscribeMatchUpdated(match), mock.subscribeGtpConsole(console)]);
  expect((await mock.settings()).gtp_engines).toEqual([]);
  const view = await mock.startMatch({});
  expect(view.completed).toBe(4); expect(view.a?.black).toBe(2); expect(view.a?.white).toBe(0);
  expect(view.unscored).toEqual({}); expect(match.mock.calls[0]?.[0].games).toBe(1);
  expect(console.mock.calls[0]?.[0].direction).toBe("Received"); expect(old).not.toHaveBeenCalled();
  expect((await mock.gtpConsole("gtp-a"))[0]?.sequence).toBe(0);
  expect((await mock.testGtpEngine("gtp-a")).commands).toEqual([]);
  await expect(mock.gtpConsoleSend("gtp-a", "name")).rejects.toThrow("Expected mock command");
  await expect(mock.startMatch({})).rejects.toThrow("Expected mock command");
  for (const id of ids) await mock.unsubscribe(id);
});
