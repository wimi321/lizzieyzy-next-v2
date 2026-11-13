import { beforeEach, expect, test, vi } from "vitest";
import type { SupportCommands } from "./backend";

const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: ipc.invoke, Channel: class {} }));

beforeEach(() => {
  vi.resetModules();
  ipc.invoke.mockReset();
});

test("logging and diagnostics IPC forwards the backend state, export path and errors", async () => {
  const api = await import("./backend");
  const legacy: SupportCommands = {};
  expect(legacy.exportDiagnostics).toBeUndefined();
  const status = { enabled: true, remaining_seconds: 1800 };
  const cases: [() => Promise<unknown>, string][] = [
    [api.fullLogStatus, "full_log_status"],
    [api.startFullLog, "start_full_log"],
    [api.stopFullLog, "stop_full_log"],
  ];
  for (const [call, name] of cases) {
    ipc.invoke.mockResolvedValueOnce(status);
    expect(await call()).toBe(status);
    expect(ipc.invoke).toHaveBeenLastCalledWith(name);
  }
  ipc.invoke.mockResolvedValueOnce("/data/diagnostics/report.zip");
  expect(await api.exportDiagnostics()).toBe("/data/diagnostics/report.zip");
  expect(ipc.invoke).toHaveBeenLastCalledWith("export_diagnostics");
  const error = { type: "App", data: "diagnostics could not be exported" };
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(api.exportDiagnostics()).rejects.toBe(error);
});

test("mock logging and exports require scripts and fill optional defaults", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  mock.setScript({ commands: [
    { command: "fullLogStatus", result: {} },
    { command: "startFullLog", result: { enabled: true, remaining_seconds: 1800 } },
    { command: "stopFullLog", result: { enabled: false, remaining_seconds: 0 } },
    { command: "exportDiagnostics", result: "/data/diagnostics/report.zip" },
  ] });
  const initial = await mock.fullLogStatus();
  expect(initial.enabled).toBe(false);
  expect(initial.remaining_seconds).toBe(0);
  expect(initial).toEqual({});
  expect(await mock.startFullLog()).toEqual({ enabled: true, remaining_seconds: 1800 });
  expect(await mock.stopFullLog()).toEqual({ enabled: false, remaining_seconds: 0 });
  expect(await mock.exportDiagnostics()).toBe("/data/diagnostics/report.zip");
  expect(mock.recordedCalls()).toEqual([
    { command: "fullLogStatus", args: [] }, { command: "startFullLog", args: [] },
    { command: "stopFullLog", args: [] }, { command: "exportDiagnostics", args: [] },
  ]);
  for (const call of [mock.fullLogStatus, mock.startFullLog, mock.stopFullLog, mock.exportDiagnostics]) {
    await expect(call()).rejects.toThrow("Expected mock command (none)");
  }
});
