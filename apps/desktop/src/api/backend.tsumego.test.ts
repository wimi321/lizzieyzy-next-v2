import { beforeEach, expect, test, vi } from "vitest";
import type { TsumegoOptions, TsumegoStatus, WireAppEvent } from "./backend";
import { emptyNineteen } from "../../e2e/fixtures";

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, Channel: class {} }));

const options: TsumegoOptions = {
  to_play: "White", attacker: "Black", ko_threat: "Defender", wall_distance: 2,
};
const status: TsumegoStatus = {
  revision: 7, active: true, source_node: 3, options, attacker: "Black", ko_warnings: ["White"],
};

beforeEach(() => {
  vi.resetModules();
  ipc.invoke.mockReset();
});

test("tsumego IPC forwards options, revisions, status and errors unchanged", async () => {
  const backend = await import("./backend");
  ipc.invoke.mockResolvedValueOnce(7);
  expect(await backend.generateTsumego(options)).toBe(7);
  expect(ipc.invoke).toHaveBeenLastCalledWith("generate_tsumego", { options });
  ipc.invoke.mockResolvedValueOnce(status);
  expect(await backend.tsumegoStatus()).toBe(status);
  expect(ipc.invoke).toHaveBeenLastCalledWith("tsumego_status");
  ipc.invoke.mockResolvedValueOnce(8);
  expect(await backend.exitTsumego()).toBe(8);
  expect(ipc.invoke).toHaveBeenLastCalledWith("exit_tsumego");
  ipc.invoke.mockRejectedValueOnce({ type: "App", data: "invalid wall distance" });
  await expect(backend.generateTsumego({ ...options, wall_distance: 0 })).rejects.toEqual({
    type: "App", data: "invalid wall distance",
  });
});

test("mock tsumego replays document events and warning data without generating stones", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const event: WireAppEvent = { type: "DocumentChanged", data: { revision: 7, view: emptyNineteen() } };
  mock.setScript({ commands: [
    { command: "generateTsumego", result: 7, events: [event] },
    { command: "tsumegoStatus", result: status },
    { command: "exitTsumego", result: 8 },
    { command: "generateTsumego", result: 8, error: "no room for walls" },
  ] });
  const updates = vi.fn();
  const id = await mock.subscribe(updates);
  expect(await mock.generateTsumego(options)).toBe(7);
  expect(updates).toHaveBeenLastCalledWith(event);
  const received = await mock.tsumegoStatus();
  expect(received).toEqual(status);
  received.ko_warnings.length = 0;
  expect(status.ko_warnings).toEqual(["White"]);
  expect(await mock.exitTsumego()).toBe(8);
  await expect(mock.generateTsumego(options)).rejects.toThrow("no room for walls");
  expect(mock.recordedCalls()).toEqual([
    { command: "generateTsumego", args: [options] },
    { command: "tsumegoStatus", args: [] },
    { command: "exitTsumego", args: [] },
    { command: "generateTsumego", args: [options] },
  ]);
  await mock.unsubscribe(id);
});

test("mock settings preserve all explicitly supplied tsumego options", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  mock.setScript({ commands: [{ command: "settings", result: {
    version: 1, engine: null, pondering: false, tsumego: options,
  } }] });
  expect((await mock.settings()).tsumego).toEqual(options);
});
