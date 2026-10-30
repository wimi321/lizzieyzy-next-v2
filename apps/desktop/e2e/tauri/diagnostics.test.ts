// @vitest-environment node
import { execFile } from "node:child_process";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { recordWindowsProcesses } from "./diagnostics.js";

vi.mock("node:child_process", () => ({ execFile: vi.fn() }));

let directory: string;
beforeEach(async () => {
  directory = await mkdtemp(join(tmpdir(), "lizzie process diagnostics "));
  vi.spyOn(console, "log").mockImplementation(() => {});
});
afterEach(async () => {
  vi.restoreAllMocks();
  vi.mocked(execFile).mockReset();
  await rm(directory, { recursive: true, force: true });
});

function complete(error: Error | null, stdout: string, stderr = "") {
  const call = vi.mocked(execFile).mock.calls[0];
  if (!call) throw new Error("process inspection was not started");
  const callback = call.at(-1) as (error: Error | null, stdout: string, stderr: string) => void;
  callback(error, stdout, stderr);
}

test("saves and prints the process snapshot without a WebDriver session", async () => {
  const snapshot = JSON.stringify({ applicationAlive: true, nativeDriverAlive: true,
    processes: [{ Name: "lizzieyzy-desktop.exe", ProcessId: 42 }] });
  const saving = recordWindowsProcesses(directory, "navigation", join(directory, "lizzieyzy-desktop.exe"));
  expect(execFile).toHaveBeenCalledWith("powershell.exe", expect.any(Array), expect.objectContaining({
    timeout: 5000, env: expect.objectContaining({ TAURI_E2E_PROCESS_NAME: "lizzieyzy-desktop.exe" }),
  }), expect.any(Function));
  complete(null, snapshot);
  await saving;
  expect(await readFile(join(directory, "navigation-processes.json"), "utf8")).toBe(snapshot);
  expect(console.log).toHaveBeenCalledWith(expect.stringContaining(snapshot));
});

test("preserves partial diagnostics when process inspection fails or times out", async () => {
  const saving = recordWindowsProcesses(directory, "navigation", "lizzieyzy-desktop.exe");
  complete(new Error("CIM timed out"), "partial output", "diagnostic stderr");
  await saving;
  expect(JSON.parse(await readFile(join(directory, "navigation-processes.json"), "utf8"))).toMatchObject({
    error: "CIM timed out", stdout: "partial output", stderr: "diagnostic stderr",
  });
});
