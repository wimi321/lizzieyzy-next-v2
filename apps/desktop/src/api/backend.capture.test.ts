import { beforeEach, expect, test, vi } from "vitest";
import type { CaptureResult, CapturePoint, CaptureRecognition, ScreenCaptureStarted, Settings, WireAppEvent } from "./backend";
import { defaultCaptureSettings } from "../../e2e/fixtures";

interface FakeChannel { onmessage: (event: WireAppEvent) => void }
const ipc = vi.hoisted(() => ({ invoke: vi.fn(), channels: [] as FakeChannel[] }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: ipc.invoke,
  Channel: class implements FakeChannel {
    onmessage: (event: WireAppEvent) => void = () => {};
    constructor() { ipc.channels.push(this); }
  },
}));

const points: [CapturePoint, CapturePoint, CapturePoint] = [
  { x: 20, y: 20 }, { x: 60, y: 60 }, { x: 100, y: 60 },
];
const result: CaptureResult = {
  corner: "BottomRight", board_size: 9,
  cells: { rows: 2, columns: 3, cells: ["Empty", "Black", "White", "Black", "Empty", "White"] },
};
const recognition: CaptureRecognition = {
  grid: { origin: points[0], gap_x: 40, gap_y: 40, rows: 2, columns: 3, corner: "TopLeft" },
  cells: result.cells,
};
const started: ScreenCaptureStarted = {
  session: { id: 8, width: 2000, height: 1400, scale_factor: 2 },
  logical_width: 1000, logical_height: 700,
};

beforeEach(() => {
  vi.resetModules();
  ipc.invoke.mockReset();
  ipc.channels.length = 0;
});

test("capture IPC keeps pixel points, corrections and binary PNG unchanged", async () => {
  const api = await import("./backend");
  const png = Uint8Array.of(137, 80, 78, 71, 13, 10, 26, 10).buffer;
  ipc.invoke.mockResolvedValueOnce(png);
  expect(await api.captureImage(8)).toBe(png);
  expect(ipc.invoke).toHaveBeenLastCalledWith("capture_image", { id: 8 });
  ipc.invoke.mockResolvedValueOnce(recognition);
  expect(await api.recognizeCapture(8, points)).toBe(recognition);
  expect(ipc.invoke).toHaveBeenLastCalledWith("recognize_capture", { id: 8, points });
  ipc.invoke.mockResolvedValueOnce(17);
  expect(await api.finishCapture(8, result)).toBe(17);
  expect(ipc.invoke).toHaveBeenLastCalledWith("finish_capture", { id: 8, result });
  const cases: [() => Promise<unknown>, string, object?][] = [
    [api.captureSettings, "capture_settings"],
    [() => api.setCaptureSettings(defaultCaptureSettings), "set_capture_settings", { settings: defaultCaptureSettings }],
    [api.capturePermission, "capture_permission"],
    [api.requestCapturePermission, "request_capture_permission"],
    [api.openCapturePermissionSettings, "open_capture_permission_settings"],
    [api.startScreenCapture, "start_screen_capture"],
    [() => api.cancelCapture(8), "cancel_capture", { id: 8 }],
  ];
  for (const [call, name, args] of cases) {
    ipc.invoke.mockResolvedValueOnce(started);
    expect(await call()).toBe(started);
    expect(ipc.invoke.mock.lastCall).toEqual(args === undefined ? [name] : [name, args]);
  }
  const failure = { type: "ScreenCapture", data: { type: "PermissionMissing" } };
  ipc.invoke.mockRejectedValueOnce(failure);
  await expect(api.startScreenCapture()).rejects.toBe(failure);
});

test("capture completion shares one channel and never reaches ordinary board listeners", async () => {
  const api = await import("./backend");
  ipc.invoke.mockResolvedValue(42);
  const ordinary = vi.fn();
  const ended = vi.fn();
  const ids = await Promise.all([api.subscribe(ordinary), api.subscribeCaptureEnded(ended)]);
  const outcomes = [
    { type: "Finished" }, { type: "Cancelled" }, { type: "Failed", data: { reason: "window failed" } },
  ] as const;
  for (const outcome of outcomes) ipc.channels[0]!.onmessage({ type: "CaptureEnded", data: { outcome } });
  expect(ipc.channels).toHaveLength(1);
  expect(ended.mock.calls).toEqual(outcomes.map((outcome) => [{ outcome }]));
  expect(ordinary).not.toHaveBeenCalled();
  await api.unsubscribe(ids[1]!);
  ipc.channels[0]!.onmessage({ type: "CaptureEnded", data: { outcome: { type: "Finished" } } });
  expect(ended).toHaveBeenCalledTimes(3);
  await api.unsubscribe(ids[0]!);
  expect(ipc.invoke).toHaveBeenLastCalledWith("unsubscribe", { id: 42 });
});

test("mock capture reads are defaults without scripts, mutations and image reads require scripts", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const legacy = { version: 1, engine: null, pondering: true } as Settings;
  const png = Uint8Array.of(137, 80, 78, 71).buffer;
  const thresholds = { ...defaultCaptureSettings, black_offset: 0, gray_offset: 255 };
  mock.setScript({ commands: [
    { command: "settings", result: legacy },
    { command: "settings", result: { ...legacy, capture: { black_percent: 100 } } },
    { command: "captureSettings", result: thresholds },
    { command: "capturePermission", result: "Missing" },
    { command: "setCaptureSettings", result: thresholds },
    { command: "requestCapturePermission", result: "Granted" },
    { command: "openCapturePermissionSettings", result: undefined },
    { command: "startScreenCapture", result: started },
    { command: "captureImage", result: png },
    { command: "recognizeCapture", result: recognition },
    { command: "finishCapture", result: 18 },
    { command: "cancelCapture", result: undefined },
  ] });
  expect(await mock.captureSettings()).toEqual(defaultCaptureSettings);
  expect(await mock.capturePermission()).toBe("NotNeeded");
  const oldSettings = await mock.settings();
  expect(oldSettings.capture).toEqual(defaultCaptureSettings);
  expect(JSON.parse(JSON.stringify(oldSettings))).not.toHaveProperty("capture");
  const partial = (await mock.settings()).capture!;
  expect(partial.black_percent).toBe(100);
  expect(partial.white_offset).toBe(120);
  expect(await mock.captureSettings()).toEqual(thresholds);
  expect(await mock.capturePermission()).toBe("Missing");
  expect(await mock.setCaptureSettings(thresholds)).toEqual(thresholds);
  expect(await mock.requestCapturePermission()).toBe("Granted");
  await mock.openCapturePermissionSettings();
  expect(await mock.startScreenCapture()).toEqual(started);
  expect(new Uint8Array(await mock.captureImage(8))).toEqual(new Uint8Array(png));
  expect(await mock.recognizeCapture(8, points)).toEqual(recognition);
  expect(await mock.finishCapture(8, result)).toBe(18);
  await mock.cancelCapture(8);
  const unscripted = [
    () => mock.setCaptureSettings(thresholds), mock.requestCapturePermission,
    mock.openCapturePermissionSettings, mock.startScreenCapture,
    () => mock.captureImage(8), () => mock.recognizeCapture(8, points),
    () => mock.finishCapture(8, result), () => mock.cancelCapture(8),
  ];
  for (const call of unscripted) await expect(call()).rejects.toThrow("Expected mock command");
  expect(await mock.captureSettings()).toEqual(defaultCaptureSettings);
  expect(await mock.capturePermission()).toBe("NotNeeded");
});

test("mock CaptureEnded pushEvents preserve outcomes and unsubscribe", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  mock.setScript({});
  const ordinary = vi.fn();
  const ended = vi.fn();
  const uiId = await mock.subscribe(ordinary);
  const id = await mock.subscribeCaptureEnded(ended);
  const events: WireAppEvent[] = [
    { type: "CaptureEnded", data: { outcome: { type: "Finished" } } },
    { type: "CaptureEnded", data: { outcome: { type: "Cancelled" } } },
    { type: "CaptureEnded", data: { outcome: { type: "Failed", data: { reason: "capture unavailable" } } } },
  ];
  mock.pushEvents(events);
  expect(ended.mock.calls).toEqual(events.map((event) => [event.data]));
  expect(ordinary).not.toHaveBeenCalled();
  await mock.unsubscribe(id);
  mock.pushEvents(events);
  expect(ended).toHaveBeenCalledTimes(3);
  await mock.unsubscribe(uiId);
});
