import { beforeEach, expect, expectTypeOf, test, vi } from "vitest";
import type { AppEvent, Backend } from "./backend";

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
        tree: { root: 0, current: 0, main_line: [0], variation_count: 0 },
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
      candidates: [], ownership: null, is_during_search: false, source: "Live",
    },
  },
  { type: "EngineStatusChanged", data: { type: "Stopped" } },
];

test("forwards every command with typed arguments and preserves results/errors", async () => {
  const backend = await import("./backend");
  const mv = { type: "Pass", data: { color: "Black" } } as const;
  const profile = { program: "engine", model: "model", config: "config", extra_args: [] };
  const settings = { version: 1, engine: profile, pondering: true };
  const setup = { ...profile, config: null };
  const discovery = { programs: ["engine"], models: ["model"], configs: [] };
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
    [() => backend.tryConfigureEngine(setup), "try_configure_engine", { profile: setup }, undefined],
    [() => backend.settings(), "settings", undefined, settings],
  ];
  for (const [call, command, args, result] of cases) {
    ipc.invoke.mockResolvedValueOnce(result);
    expect(await call()).toEqual(result);
    expect(ipc.invoke).toHaveBeenLastCalledWith(...(args ? [command, args] : [command]));
  }
  ipc.invoke.mockRejectedValueOnce("invalid SGF");
  await expect(backend.openBytes([])).rejects.toBe("invalid SGF");
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
