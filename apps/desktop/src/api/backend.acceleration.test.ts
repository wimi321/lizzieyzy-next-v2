import { beforeEach, expect, test, vi } from "vitest";
import type { AccelerationProgress, Settings, WireAppEvent } from "./backend";

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

test("acceleration IPC forwards the chosen backend and keeps typed failures", async () => {
  const api = await import("./backend");
  const cases: [() => Promise<unknown>, string, object | undefined][] = [
    [() => api.enableAcceleration("cuda"), "enable_acceleration", { backend: "cuda" }],
    [() => api.enableAcceleration("tensorrt"), "enable_acceleration", { backend: "tensorrt" }],
    [() => api.enableAcceleration("opencl"), "enable_acceleration", { backend: "opencl" }],
    [api.disableAcceleration, "disable_acceleration", undefined],
    [api.dismissAccelerationHint, "dismiss_acceleration_hint", undefined],
    [api.accelerationOutput, "acceleration_output", undefined],
  ];
  for (const [call, name, args] of cases) {
    const reply = { marker: name };
    ipc.invoke.mockResolvedValueOnce(reply);
    expect(await call()).toBe(reply);
    expect(ipc.invoke).toHaveBeenLastCalledWith(...(args ? [name, args] : [name]));
  }
  const error = { type: "Acceleration", data: { type: "Failed", data: { reason: "driver", stderr_tail: ["CUDA error"] } } };
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(api.enableAcceleration("cuda")).rejects.toBe(error);
});

test("acceleration stages use the existing ordered channel without reaching old UI listeners", async () => {
  const api = await import("./backend");
  ipc.invoke.mockResolvedValue(93);
  const ui = vi.fn();
  const acceleration = vi.fn();
  const download = vi.fn();
  const ids = await Promise.all([
    api.subscribe(ui), api.subscribeAccelerationProgress(acceleration), api.subscribeAssetDownloadProgress(download),
  ]);
  const progress: AccelerationProgress = { backend: "opencl", phase: "Tuning", output_tail: ["Tuning xGemmDirect"] };
  ipc.channels[0]!.onmessage({ type: "AccelerationProgress", data: progress });
  expect(ipc.channels).toHaveLength(1);
  expect(acceleration.mock.calls).toEqual([[progress]]);
  expect(ui).not.toHaveBeenCalled();
  expect(download).not.toHaveBeenCalled();
  for (const id of ids) await api.unsubscribe(id);
  expect(ipc.invoke).toHaveBeenLastCalledWith("unsubscribe", { id: 93 });
});

test("mock supplies old settings and plan defaults, and scripts every mutation and stage", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const before = { version: 1, engine: null, pondering: true } as Settings;
  const ready: AccelerationProgress = { backend: "opencl", phase: "Ready", output_tail: ["Using OpenCL Device"] };
  mock.setScript({ commands: [
    { command: "settings", result: before },
    { command: "accelerationPlan", result: { gpus: [{ name: "AMD", nvidia: false }], options: [{ backend: "opencl", recommended: true }] } },
    { command: "enableAcceleration", result: before, events: [{ type: "AccelerationProgress", data: ready }] },
    { command: "disableAcceleration", result: { profile: null, checks: [] } },
    { command: "dismissAccelerationHint", result: undefined },
    { command: "settings", result: { ...before, acceleration_hint_dismissed: true } },
  ] });
  expect((await mock.settings()).acceleration_hint_dismissed).toBe(false);
  const plan = await mock.accelerationPlan();
  expect(plan.current).toBe("external");
  expect(plan.gpus?.[0]).toMatchObject({ name: "AMD", driver_version: null, nvidia: false });
  expect(plan.options?.[0]).toMatchObject({ backend: "opencl", recommended: true, installed: false, download_bytes: 0 });
  expect(await mock.accelerationOutput()).toEqual([]);
  const progress = vi.fn();
  const ui = vi.fn();
  const progressId = await mock.subscribeAccelerationProgress(progress);
  const uiId = await mock.subscribe(ui);
  expect(await mock.enableAcceleration("opencl")).toEqual(before);
  expect(progress).toHaveBeenLastCalledWith(ready);
  expect(ui).not.toHaveBeenCalled();
  await mock.disableAcceleration();
  await mock.dismissAccelerationHint();
  expect((await mock.settings()).acceleration_hint_dismissed).toBe(true);
  mock.pushEvents([{ type: "AccelerationProgress", data: { backend: "cuda", phase: "Starting" } }]);
  expect(progress).toHaveBeenLastCalledWith({ backend: "cuda", phase: "Starting" });
  expect(progress.mock.lastCall?.[0].output_tail).toEqual([]);
  await expect(mock.enableAcceleration("cuda")).rejects.toThrow("Expected mock command");
  await expect(mock.disableAcceleration()).rejects.toThrow("Expected mock command");
  await expect(mock.dismissAccelerationHint()).rejects.toThrow("Expected mock command");
  await mock.unsubscribe(progressId);
  await mock.unsubscribe(uiId);
});
