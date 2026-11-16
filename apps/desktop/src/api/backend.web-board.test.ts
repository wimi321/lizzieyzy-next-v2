import { beforeEach, expect, test, vi } from "vitest";
import type { Settings, WebBoardStatus, WireAppEvent } from "./backend";
import { analysisUpdated, analysisView, boardView, candidate } from "../../e2e/fixtures";

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

test("web board IPC preserves parameters, replies and typed failures", async () => {
  const api = await import("./backend");
  const cases: [() => Promise<unknown>, string, object | undefined][] = [
    [api.startWebBoard, "start_web_board", undefined],
    [api.stopWebBoard, "stop_web_board", undefined],
    [api.webBoardStatus, "web_board_status", undefined],
    [() => api.setWebBoardOptions({ allow_trial: false }), "set_web_board_options", { options: { allow_trial: false } }],
    [api.forceEndWebTrial, "force_end_web_trial", undefined],
  ];
  for (const [call, name, args] of cases) {
    const result = { marker: name };
    ipc.invoke.mockResolvedValueOnce(result);
    expect(await call()).toBe(result);
    expect(ipc.invoke).toHaveBeenLastCalledWith(...(args ? [name, args] : [name]));
  }
  const failure = { type: "WebBoard", data: { type: "Bind", data: { reason: "occupied" } } };
  ipc.invoke.mockRejectedValueOnce(failure);
  await expect(api.startWebBoard()).rejects.toBe(failure);
});

test("web board status uses the shared channel and stays out of legacy subscriptions", async () => {
  const api = await import("./backend");
  ipc.invoke.mockResolvedValue(95);
  const ui = vi.fn();
  const web = vi.fn();
  const ids = await Promise.all([api.subscribe(ui), api.subscribeWebBoardChanged(web)]);
  const status: WebBoardStatus = { running: true, port: 9998, viewers: 2, trial: { owner: "client-a", since: 123 } };
  ipc.channels[0]!.onmessage({ type: "WebBoardChanged", data: status });
  expect(ipc.channels).toHaveLength(1);
  expect(web.mock.calls).toEqual([[status]]);
  expect(ui).not.toHaveBeenCalled();
  for (const id of ids) await api.unsubscribe(id);
  expect(ipc.invoke).toHaveBeenLastCalledWith("unsubscribe", { id: 95 });
});

test("mock defaults old settings and status while every web command remains scripted", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const old = { version: 1, engine: null, pondering: true } as Settings;
  mock.setScript({ commands: [
    { command: "settings", result: old },
    { command: "startWebBoard", result: { running: true, port: 9998 } },
    { command: "webBoardStatus", result: { running: true, viewers: 2 } },
    { command: "setWebBoardOptions", result: undefined },
    { command: "settings", result: { ...old, web_board: { allow_trial: false } } },
    { command: "forceEndWebTrial", result: undefined },
    { command: "stopWebBoard", result: undefined },
  ] });
  expect((await mock.settings()).web_board?.allow_trial).toBe(true);
  const started = await mock.startWebBoard();
  expect(started.running).toBe(true);
  expect(started.addresses).toEqual([]);
  expect(started.trial).toBeNull();
  expect((await mock.webBoardStatus()).viewers).toBe(2);
  await mock.setWebBoardOptions({ allow_trial: false });
  expect((await mock.settings()).web_board?.allow_trial).toBe(false);
  await mock.forceEndWebTrial();
  await mock.stopWebBoard();
  const ui = vi.fn();
  const web = vi.fn();
  const ids = await Promise.all([mock.subscribe(ui), mock.subscribeWebBoardChanged(web)]);
  mock.pushEvents([{ type: "WebBoardChanged", data: { viewers: 3 } }]);
  expect(web).toHaveBeenCalledTimes(1);
  expect(web.mock.lastCall?.[0].viewers).toBe(3);
  expect(web.mock.lastCall?.[0].running).toBe(false);
  expect(ui).not.toHaveBeenCalled();
  for (const call of [mock.startWebBoard, mock.stopWebBoard, mock.webBoardStatus, mock.forceEndWebTrial]) {
    await expect(call()).rejects.toThrow("Expected mock command");
  }
  await expect(mock.setWebBoardOptions({ allow_trial: true })).rejects.toThrow("Expected mock command");
  for (const id of ids) await mock.unsubscribe(id);
});

test("analysis candidate uncertainty and LCB are preserved with null legacy defaults", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const legacy = analysisView(0, 0, [candidate({ x: 3, y: 3 }, 0, 0.62, 2, 10, [], boardView(19, 19, []))]);
  const current = structuredClone(legacy);
  current.candidates[0]!.score_stdev = 8.2;
  current.candidates[0]!.lcb = 0.59;
  mock.setScript({ commands: [{ command: "wholeGameResult", result: current }] });
  const received: WireAppEvent[] = [];
  const id = await mock.subscribe((event) => received.push(event));
  mock.pushEvents([analysisUpdated(legacy)]);
  const event = received[0];
  expect(event?.type).toBe("AnalysisUpdated");
  if (event?.type === "AnalysisUpdated") {
    expect(event.data.candidates[0]!.score_stdev).toBeNull();
    expect(event.data.candidates[0]!.lcb).toBeNull();
  }
  const result = await mock.wholeGameResult(0);
  expect(result?.candidates[0]!.score_stdev).toBe(8.2);
  expect(result?.candidates[0]!.lcb).toBe(0.59);
  await mock.unsubscribe(id);
});
