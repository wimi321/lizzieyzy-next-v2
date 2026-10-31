import { beforeEach, expect, expectTypeOf, test, vi } from "vitest";
import type { AppEvent, Backend, CommandError, MoveRankSettings, Settings } from "./backend";

const ipc = vi.hoisted(() => ({ invoke: vi.fn(), channels: [] as FakeChannel[] }));

interface FakeChannel {
  onmessage: (event: AppEvent) => void;
}

vi.mock("@tauri-apps/api/core", () => ({
  invoke: ipc.invoke,
  Channel: class implements FakeChannel {
    onmessage: (event: AppEvent) => void = () => {};
    constructor() { ipc.channels.push(this); }
  },
}));

beforeEach(() => {
  vi.resetModules();
  ipc.invoke.mockReset();
  ipc.channels.length = 0;
});

const events: AppEvent[] = [
  {
    type: "DocumentChanged",
    data: {
      revision: 1,
      view: {
        board: null,
        tree: { root: 0, current: 0, main_line: [0], variation_count: 0, structure_revision: 1 },
        game_info: {
          black_name: null, black_rank: null, white_name: null, white_rank: null,
          komi: 7.5, rules: null, handicap: null, result: null, date: null, event: null,
        },
        graph: {
          points: [{ node_id: 0, move_number: 0, summary: null }],
          current_index: 0,
        },
        comment: null,
        warnings: [],
        position_error: "test position",
      },
    },
  },
  {
    type: "AnalysisUpdated",
    data: {
      node_id: 0, revision: 1, winrate: 0.5, score_lead: 0, visits: 10,
      summary: { winrate: 0.6, score_lead: 2, visits: 20 },
      visits_per_second: 1200,
      candidates: [], ownership: null, is_during_search: false, source: "Live",
    },
  },
  { type: "EngineStatusChanged", data: { type: "Stopped" } },
];

test("forwards every command with typed arguments and preserves results/errors", async () => {
  const backend = await import("./backend");
  const mv = { type: "Pass", data: { color: "Black" } } as const;
  const profile = { program: "engine", model: "model", config: "config", extra_args: [] };
  const settings = { version: 1, engine: profile, pondering: true, fox_history: [] };
  const setup = { ...profile, config: null };
  const discovery = { programs: ["engine"], models: ["model"], configs: [] };
  const query = { type: "Nickname", value: "00123456" } as const;
  const page = { games: [], next_cursor: null };
  const search = { account: { uid: "123", nickname: "00123456" }, page };
  const cases: [() => Promise<unknown>, string, Record<string, unknown> | undefined, unknown][] = [
    [() => backend.openBytes([40, 59, 41]), "open_bytes", { bytes: [40, 59, 41] }, 1],
    [() => backend.openPath("棋谱.sgf"), "open_path", { path: "棋谱.sgf" }, 2],
    [() => backend.save("saved.sgf"), "save", { path: "saved.sgf" }, undefined],
    [() => backend.play(mv), "play", { mv }, 3],
    [() => backend.step(-1), "step", { count: -1 }, 4],
    [() => backend.goTo(7), "go_to", { nodeId: 7 }, 5],
    [() => backend.undo(), "undo", undefined, 6],
    [() => backend.redo(), "redo", undefined, 7],
    [() => backend.setPondering(false), "set_pondering", { enabled: false }, undefined],
    [() => backend.configureEngine(profile), "configure_engine", { profile }, undefined],
    [() => backend.configureEngine(null), "configure_engine", { profile: null }, undefined],
    [() => backend.detectEngines(), "detect_engines", undefined, discovery],
    [() => backend.autoConfigureEngine(), "auto_configure_engine", undefined, profile],
    [() => backend.tryConfigureEngine(setup), "try_configure_engine", { profile: setup }, undefined],
    [() => backend.foxSearch(query), "fox_search", { query }, search],
    [() => backend.foxNextPage("123", "cursor"), "fox_next_page", { uid: "123", cursor: "cursor" }, page],
    [() => backend.foxOpenGame("game-id"), "fox_open_game", { gameId: "game-id" }, 8],
    [() => backend.foxClearHistory(), "fox_clear_history", undefined, undefined],
    [() => backend.settings(), "settings", undefined, settings],
  ];
  for (const [call, command, args, result] of cases) {
    ipc.invoke.mockResolvedValueOnce(result);
    expect(await call()).toEqual(result);
    expect(ipc.invoke).toHaveBeenLastCalledWith(...(args ? [command, args] : [command]));
  }
  ipc.invoke.mockRejectedValueOnce("invalid SGF");
  await expect(backend.openBytes([])).rejects.toBe("invalid SGF");
  const missing = { type: "Fox", data: { type: "UserNotFound" } };
  ipc.invoke.mockRejectedValueOnce(missing);
  await expect(backend.foxSearch(query)).rejects.toEqual(missing);
});

test("preserves automatic setup reasons and canonical model names", async () => {
  const backend = await import("./backend");
  const profile = {
    program: "katago", model: "default.bin.gz", config: "analysis.cfg", extra_args: [],
    canonical_model_name: "kata1-tf3-b11c768-s12002M-d6304M.bin.gz",
  };
  ipc.invoke.mockResolvedValueOnce(profile);
  expect(await backend.autoConfigureEngine()).toEqual(profile);
  const errors = [
    { type: "NoPrograms" },
    { type: "NoModels" },
    { type: "AllFailed", data: {
      attempts: [{ program: "katago", model: "default.bin.gz", reason: "version too old" }],
      reason: "no usable pair",
    } },
  ] as const;
  for (const data of errors) {
    const error = { type: "AutoSetup", data };
    ipc.invoke.mockRejectedValueOnce(error);
    await expect(backend.autoConfigureEngine()).rejects.toBe(error);
  }
});

test.each(["Timeout", "Network"] as const)("preserves Fox %s errors for search and open", async (type) => {
  const backend = await import("./backend");
  const error = { type: "Fox", data: { type } } as const;
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(backend.foxSearch({ type: "Nickname", value: "sample" })).rejects.toBe(error);
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(backend.foxOpenGame("game-id")).rejects.toBe(error);
});

test("one channel preserves all event kinds, including snapshots before subscribe resolves", async () => {
  const backend = await import("./backend");
  const first = vi.fn();
  const second = vi.fn();
  ipc.invoke.mockImplementation(async (name) => {
    if (name === "subscribe") {
      for (const event of events) ipc.channels[0]?.onmessage(event);
      return 12;
    }
  });
  const ids = await Promise.all([backend.subscribe(first), backend.subscribe(second)]);
  expect(ipc.channels).toHaveLength(1);
  expect(first.mock.calls.map(([event]) => event)).toEqual(events);
  expect(second.mock.calls.map(([event]) => event)).toEqual(events);
  await backend.unsubscribe(ids[0]!);
  expect(ipc.invoke).toHaveBeenCalledTimes(1);
  await backend.unsubscribe(ids[1]!);
  expect(ipc.invoke).toHaveBeenLastCalledWith("unsubscribe", { id: 12 });
  await backend.unsubscribe(ids[1]!);
  for (const event of events) ipc.channels[0]?.onmessage(event);
  expect(ipc.invoke).toHaveBeenCalledTimes(2);
  expect(first).toHaveBeenCalledTimes(3);
  expect(second).toHaveBeenCalledTimes(3);
});

test("late unsubscribe and old channel cannot affect a new subscription", async () => {
  const backend = await import("./backend");
  ipc.invoke.mockResolvedValueOnce(10);
  const oldListener = vi.fn();
  const oldId = await backend.subscribe(oldListener);
  const oldChannel = ipc.channels[0]!;
  const oldCallback = oldChannel.onmessage;
  let finishUnsubscribe: (() => void) | undefined;
  ipc.invoke.mockImplementationOnce(() => new Promise<void>((resolve) => {
    finishUnsubscribe = resolve;
  }));
  const removing = backend.unsubscribe(oldId);
  await Promise.resolve();
  ipc.invoke.mockResolvedValueOnce(11);
  const newListener = vi.fn();
  const newId = await backend.subscribe(newListener);
  finishUnsubscribe?.();
  await removing;
  oldCallback(events[0]!);
  ipc.channels[1]?.onmessage(events[1]!);
  expect(oldListener).not.toHaveBeenCalled();
  expect(newListener.mock.calls).toEqual([[events[1]]]);
  await backend.unsubscribe(newId);
  expect(ipc.invoke).toHaveBeenLastCalledWith("unsubscribe", { id: 11 });
});

test("subscription failure rejects every waiter and permits a fresh retry", async () => {
  const backend = await import("./backend");
  ipc.invoke.mockRejectedValueOnce("bridge unavailable");
  const first = vi.fn();
  const second = vi.fn();
  const failures = await Promise.allSettled([
    backend.subscribe(first), backend.subscribe(second),
  ]);
  expect(failures).toEqual([
    { status: "rejected", reason: "bridge unavailable" },
    { status: "rejected", reason: "bridge unavailable" },
  ]);
  ipc.invoke.mockResolvedValueOnce(2);
  const fresh = vi.fn();
  const id = await backend.subscribe(fresh);
  ipc.channels[0]?.onmessage(events[0]!);
  ipc.channels[1]?.onmessage(events[1]!);
  expect(first).not.toHaveBeenCalled();
  expect(second).not.toHaveBeenCalled();
  expect(fresh.mock.calls).toEqual([[events[1]]]);
  await backend.unsubscribe(id);
});

test("a throwing listener does not interrupt ordered delivery to other listeners", async () => {
  const backend = await import("./backend");
  const log = vi.spyOn(console, "error").mockImplementation(() => {});
  ipc.invoke.mockResolvedValue(3);
  const bad = await backend.subscribe(() => { throw new Error("consumer failed"); });
  const healthy = vi.fn();
  const good = await backend.subscribe(healthy);
  for (const event of events) ipc.channels[0]?.onmessage(event);
  expect(healthy.mock.calls.map(([event]) => event)).toEqual(events);
  expect(log).toHaveBeenCalledTimes(3);
  await backend.unsubscribe(bad);
  await backend.unsubscribe(good);
  log.mockRestore();
});

// Only this test loader imports the E2E implementation. Production backend.ts
// never imports it; Vite's E2E alias owns the test build's replacement.
const loadMock = () => vi.importActual<typeof import("../../e2e/mock-backend")>(
  "../../e2e/mock-backend",
);

test("mock supplies fixed live speed and preserves scripted speed, zero and null", async () => {
  const mock = await loadMock();
  const { analysisView, analysisUpdated } = await import("../../e2e/fixtures");
  const legacy = analysisView(1, 0, []);
  delete legacy.visits_per_second;
  const scripted = [850, 0, null].map((visits_per_second) =>
    analysisUpdated(analysisView(1, 0, [], { visits_per_second })));
  mock.setScript({ initialEvents: [
    analysisUpdated(legacy),
    analysisUpdated({ ...legacy, source: "Cache" }),
    ...scripted,
  ] });
  const received: AppEvent[] = [];
  const id = await mock.subscribe((event) => received.push(event));
  expect(received.map((event) => event.type === "AnalysisUpdated" ? event.data.visits_per_second : undefined))
    .toEqual([1200, null, 850, 0, null]);
  await mock.unsubscribe(id);
});

test("mock replies and all three event kinds follow the script without domain logic", async () => {
  const mock = await loadMock();
  expectTypeOf(mock).toMatchTypeOf<Backend>();
  const initial = events[2]!;
  mock.setScript({
    initialEvents: [initial],
    commands: [
      { command: "openBytes", result: 1, events: events.slice(0, 2) },
      { command: "setPondering", result: undefined, events: [initial] },
    ],
  });
  const listener = vi.fn();
  const id = await mock.subscribe(listener);
  expect(await mock.openBytes([40, 59, 41])).toBe(1);
  await mock.setPondering(false);
  expect(listener.mock.calls.map(([event]) => event)).toEqual([initial, ...events]);
  expect(mock.recordedCalls()).toEqual([
    { command: "openBytes", args: [[40, 59, 41]] },
    { command: "setPondering", args: [false] },
  ]);
  await mock.unsubscribe(id);
  await mock.unsubscribe(id);
  mock.pushEvents(events);
  expect(listener).toHaveBeenCalledTimes(4);
});

test("mock queues reentrant pushes after the remaining scripted events", async () => {
  const mock = await loadMock();
  mock.setScript({});
  const received: AppEvent[] = [];
  const id = await mock.subscribe((event) => {
    received.push(event);
    if (event.type === "DocumentChanged") mock.pushEvents([events[2]!]);
  });
  mock.pushEvents(events.slice(0, 2));
  expect(received).toEqual(events);
  await mock.unsubscribe(id);
});

test("mock scripts discovery, setup success and readable setup failure", async () => {
  const mock = await loadMock();
  const discovery = { programs: ["katago"], models: ["kata1.bin.gz"], configs: [] };
  const profile = { program: "katago", model: "kata1.bin.gz", config: null, extra_args: [] };
  mock.setScript({ commands: [
    { command: "detectEngines", result: discovery },
    { command: "tryConfigureEngine", result: undefined },
    { command: "tryConfigureEngine", result: undefined, error: "model is missing" },
  ] });
  expect(await mock.detectEngines()).toEqual(discovery);
  await expect(mock.tryConfigureEngine(profile)).resolves.toBeUndefined();
  await expect(mock.tryConfigureEngine(profile)).rejects.toThrow("model is missing");
  expect(mock.recordedCalls()).toEqual([
    { command: "detectEngines", args: [] },
    { command: "tryConfigureEngine", args: [profile] },
    { command: "tryConfigureEngine", args: [profile] },
  ]);
});

test("mock replays automatic setup profiles, analysis events and typed failures", async () => {
  const mock = await loadMock();
  const profile = {
    program: "katago", model: "default.bin.gz", config: "analysis.cfg", extra_args: [],
    canonical_model_name: "kata1-tf3-b11c768-s12002M-d6304M.bin.gz",
  };
  const error: CommandError = { type: "AutoSetup", data: { type: "AllFailed", data: {
    attempts: [{ program: "katago", model: "default.bin.gz", reason: "trial failed" }],
    reason: "all pairs failed",
  } } };
  mock.setScript({ commands: [
    { command: "autoConfigureEngine", result: profile, events: [events[1]!] },
    { command: "autoConfigureEngine", result: profile, error },
  ] });
  const listener = vi.fn();
  const id = await mock.subscribe(listener);
  expect(await mock.autoConfigureEngine()).toEqual(profile);
  expect(listener).toHaveBeenCalledExactlyOnceWith(events[1]);
  await expect(mock.autoConfigureEngine()).rejects.toEqual(error);
  expect(mock.recordedCalls()).toEqual([
    { command: "autoConfigureEngine", args: [] },
    { command: "autoConfigureEngine", args: [] },
  ]);
  await mock.unsubscribe(id);
});

test("mock rejects unexpected commands and scripted errors", async () => {
  const mock = await loadMock();
  mock.setScript({ commands: [
    { command: "openPath", result: 1, error: "invalid SGF" },
    { command: "save", result: undefined },
  ] });
  await expect(mock.step(1)).rejects.toThrow("Expected mock command openPath, received step");
  await expect(mock.openPath("bad.sgf")).rejects.toThrow("invalid SGF");
  await expect(mock.save("saved.sgf")).resolves.toBeUndefined();
  await expect(mock.undo()).rejects.toThrow("Expected mock command (none), received undo");
});

test("mock scripts Fox search, pagination, open events, history and typed errors", async () => {
  const mock = await loadMock();
  const { foxSearchResult } = await import("../../e2e/fixtures");
  const result = foxSearchResult();
  const query = { type: "Nickname", value: result.account.nickname } as const;
  const missing = { type: "Fox", data: { type: "UserNotFound" } } as const;
  mock.setScript({ commands: [
    { command: "foxSearch", result },
    { command: "foxNextPage", result: { games: [], next_cursor: null } },
    { command: "foxOpenGame", result: 1, events: [events[0]!] },
    { command: "settings", result: { version: 1, engine: null, pondering: true, fox_history: [result.account] } },
    { command: "foxClearHistory", result: undefined },
    { command: "settings", result: { version: 1, engine: null, pondering: true } },
    { command: "foxSearch", result, error: missing },
  ] });
  const listener = vi.fn();
  const id = await mock.subscribe(listener);
  expect(await mock.foxSearch(query)).toEqual(result);
  expect(await mock.foxNextPage(result.account.uid, result.page.next_cursor!))
    .toEqual({ games: [], next_cursor: null });
  expect(await mock.foxOpenGame(result.page.games[0]!.id)).toBe(1);
  expect(listener).toHaveBeenCalledExactlyOnceWith(events[0]);
  expect((await mock.settings()).fox_history).toEqual([result.account]);
  await mock.foxClearHistory();
  expect((await mock.settings()).fox_history).toEqual([]);
  await expect(mock.foxSearch(query)).rejects.toEqual(missing);
  expect(mock.recordedCalls().slice(0, 3)).toEqual([
    { command: "foxSearch", args: [query] },
    { command: "foxNextPage", args: [result.account.uid, result.page.next_cursor] },
    { command: "foxOpenGame", args: [result.page.games[0]!.id] },
  ]);
  await mock.unsubscribe(id);
});

test.each([
  { history: false, rank: false },
  { history: true, rank: false },
  { history: false, rank: true },
  { history: true, rank: true },
])("mock independently defaults omitted settings fields (history: $history, rank: $rank)", async ({ history, rank }) => {
  const mock = await loadMock();
  const base = { version: 1, engine: null, pondering: false };
  const customHistory = [
    { uid: "200", nickname: "recent" },
    { uid: "100", nickname: "earlier" },
  ];
  const customRank: MoveRankSettings = {
    mode: "Winrate",
    winrate_thresholds: [2, 4, 8, 16, 32],
    score_thresholds: [1, 2, 4, 8, 16],
  };
  mock.setScript({ commands: [{
    command: "settings",
    result: {
      ...base,
      ...(history ? { fox_history: customHistory } : {}),
      ...(rank ? { move_rank: customRank } : {}),
    },
  }] });
  const result = await mock.settings();
  expectTypeOf(result).toEqualTypeOf<Settings>();
  expect(result).toEqual({
    ...base,
    fox_history: history ? customHistory : [],
    move_rank: rank ? customRank : {
      mode: "Auto",
      winrate_thresholds: [1, 3, 6, 12, 24],
      score_thresholds: [0.5, 1.5, 3, 6, 12],
    },
  });
});
