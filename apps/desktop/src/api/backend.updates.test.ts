import { beforeEach, expect, test, vi } from "vitest";
import type { CommandError, UpdateDownloadStatus, WireAppEvent } from "./backend";

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

const downloading: UpdateDownloadStatus = {
  phase: "Downloading", downloaded_bytes: 1024, total_bytes: 4096, version: "0.2.0", error: null,
};

test("unsigned builds expose the typed reason unchanged for all update operations", async () => {
  const api = await import("./backend");
  const error: CommandError = { type: "UpdateSigningNotConfigured" };
  for (const call of [api.checkUpdate, api.downloadUpdate, api.installUpdate]) {
    ipc.invoke.mockRejectedValueOnce(error);
    await expect(call()).rejects.toBe(error);
  }
});

test("update IPC forwards replies and signature errors while new methods remain optional", async () => {
  const api = await import("./backend");
  const check = { available: true, version: "0.2.0", notes: "Release notes", published_at: "2026-11-12T00:00:00Z" };
  ipc.invoke.mockResolvedValueOnce(check);
  expect(await api.checkUpdate()).toBe(check);
  expect(ipc.invoke).toHaveBeenLastCalledWith("check_update");
  const cases: [() => Promise<unknown>, string][] = [
    [api.downloadUpdate, "download_update"],
    [api.cancelUpdateDownload, "cancel_update_download"],
    [api.updateDownloadStatus, "update_download_status"],
    [api.installUpdate, "install_update"],
  ];
  for (const [call, command] of cases) {
    ipc.invoke.mockResolvedValueOnce(downloading);
    expect(await call()).toBe(downloading);
    expect(ipc.invoke).toHaveBeenLastCalledWith(command);
  }
  const error = { type: "App", data: "update signature verification failed" };
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(api.downloadUpdate()).rejects.toBe(error);
});

test("update progress uses the shared ordered connection and leaves existing UI subscribers unchanged", async () => {
  const api = await import("./backend");
  ipc.invoke.mockImplementation(async (command) => {
    if (command === "subscribe") {
      ipc.channels[0]!.onmessage({ type: "UpdateDownloadProgress", data: downloading });
      return 92;
    }
  });
  const ui = vi.fn();
  const updates = vi.fn();
  const weights = vi.fn();
  const ids = await Promise.all([
    api.subscribe(ui), api.subscribeUpdateDownloadProgress(updates), api.subscribeAssetDownloadProgress(weights),
  ]);
  expect(ipc.channels).toHaveLength(1);
  expect(updates.mock.calls).toEqual([[downloading]]);
  expect(ui).not.toHaveBeenCalled();
  expect(weights).not.toHaveBeenCalled();
  await api.unsubscribe(ids[0]!);
  await api.unsubscribe(ids[2]!);
  const downloaded: UpdateDownloadStatus = { ...downloading, phase: "Downloaded", downloaded_bytes: 4096 };
  ipc.channels[0]!.onmessage({ type: "UpdateDownloadProgress", data: downloaded });
  expect(updates.mock.calls).toEqual([[downloading], [downloaded]]);
  await api.unsubscribe(ids[1]!);
  expect(ipc.invoke).toHaveBeenLastCalledWith("unsubscribe", { id: 92 });
  ipc.channels[0]!.onmessage({ type: "UpdateDownloadProgress", data: downloading });
  expect(updates).toHaveBeenCalledTimes(2);
});

test("mock updates and progress only replay scripts, including cancellation and installation", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const cancelled: UpdateDownloadStatus = { ...downloading, phase: "Cancelled" };
  const downloaded: UpdateDownloadStatus = { ...downloading, phase: "Downloaded", downloaded_bytes: 4096 };
  const completed: UpdateDownloadStatus = { ...downloaded, phase: "Completed" };
  mock.setScript({ commands: [
    { command: "checkUpdate", result: { available: true, version: "0.2.0" } },
    { command: "downloadUpdate", result: downloading, events: [{ type: "UpdateDownloadProgress", data: downloading }] },
    { command: "cancelUpdateDownload", result: cancelled, events: [{ type: "UpdateDownloadProgress", data: cancelled }] },
    { command: "updateDownloadStatus", result: cancelled },
    { command: "downloadUpdate", result: downloaded, events: [{ type: "UpdateDownloadProgress", data: downloaded }] },
    { command: "installUpdate", result: completed, events: [{ type: "UpdateDownloadProgress", data: completed }] },
  ] });
  const ui = vi.fn();
  const updates = vi.fn();
  const uiId = await mock.subscribe(ui);
  const updateId = await mock.subscribeUpdateDownloadProgress(updates);
  const check = await mock.checkUpdate();
  expect(check.available).toBe(true);
  expect(check.notes).toBeNull();
  expect(check.published_at).toBeNull();
  expect(await mock.downloadUpdate()).toEqual(downloading);
  expect(await mock.cancelUpdateDownload()).toEqual(cancelled);
  expect(await mock.updateDownloadStatus()).toEqual(cancelled);
  expect(await mock.downloadUpdate()).toEqual(downloaded);
  expect(await mock.installUpdate()).toEqual(completed);
  expect(updates.mock.calls).toEqual([[downloading], [cancelled], [downloaded], [completed]]);
  expect(ui).not.toHaveBeenCalled();
  expect(mock.recordedCalls()).toEqual([
    { command: "checkUpdate", args: [] }, { command: "downloadUpdate", args: [] },
    { command: "cancelUpdateDownload", args: [] }, { command: "updateDownloadStatus", args: [] },
    { command: "downloadUpdate", args: [] }, { command: "installUpdate", args: [] },
  ]);
  await mock.unsubscribe(updateId);
  await mock.unsubscribe(uiId);
  // The status is a read-only query: unscripted, it answers idle.
  expect((await mock.updateDownloadStatus()).phase).toBe("Idle");
  for (const call of [mock.checkUpdate, mock.downloadUpdate, mock.cancelUpdateDownload, mock.installUpdate]) {
    await expect(call()).rejects.toThrow("Expected mock command (none)");
  }
});

test("mock update defaults preserve explicit zero, null and failures without changing scripted shapes", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  mock.setScript({ commands: [
    { command: "checkUpdate", result: {} },
    { command: "updateDownloadStatus", result: {} },
    { command: "downloadUpdate", result: { phase: "Failed", downloaded_bytes: 0, total_bytes: null, error: "signature" } },
  ] });
  const check = await mock.checkUpdate();
  expect(check.available).toBe(false);
  expect(check.version).toBeNull();
  expect(check).toEqual({});
  const idle = await mock.updateDownloadStatus();
  expect(idle.phase).toBe("Idle");
  expect(idle.downloaded_bytes).toBe(0);
  expect(idle.total_bytes).toBeNull();
  expect(idle.version).toBeNull();
  expect(idle.error).toBeNull();
  const failed = await mock.downloadUpdate();
  expect(failed).toEqual({ phase: "Failed", downloaded_bytes: 0, total_bytes: null, error: "signature" });
  expect(failed.version).toBeNull();
  const progress = vi.fn();
  const id = await mock.subscribeUpdateDownloadProgress(progress);
  mock.pushEvents([{ type: "UpdateDownloadProgress", data: { phase: "Cancelled", downloaded_bytes: 0 } }]);
  const event = progress.mock.calls[0]![0] as UpdateDownloadStatus;
  expect(event.phase).toBe("Cancelled");
  expect(event.downloaded_bytes).toBe(0);
  expect(event.total_bytes).toBeNull();
  expect(event.version).toBeNull();
  expect(event.error).toBeNull();
  await mock.unsubscribe(id);
});
