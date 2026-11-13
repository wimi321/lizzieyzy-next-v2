import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { UpdateCheck, UpdateDownloadProgressListener, UpdateDownloadStatus } from "@/api/backend";
import zh from "../locales/zh-CN.json";
import { DiagnosticsDialog } from "./DiagnosticsDialog";
import { UpdateDialog } from "./UpdateDialog";
import { localeOf } from "./useRelease";

afterEach(cleanup);

const idle: UpdateDownloadStatus = { phase: "Idle", downloaded_bytes: 0, total_bytes: null, version: null, error: null };

function updateBackend(check: () => Promise<UpdateCheck>) {
  let listener: UpdateDownloadProgressListener = () => {};
  const backend = {
    checkUpdate: vi.fn(check),
    downloadUpdate: vi.fn(() => Promise.resolve<UpdateDownloadStatus>({ ...idle, phase: "Downloading", total_bytes: 52_428_800, version: "0.3.0" })),
    cancelUpdateDownload: vi.fn(() => Promise.resolve<UpdateDownloadStatus>({ ...idle, phase: "Cancelled" })),
    updateDownloadStatus: vi.fn(() => Promise.resolve(idle)),
    installUpdate: vi.fn(() => Promise.resolve<UpdateDownloadStatus>({ ...idle, phase: "Installing" })),
    subscribeUpdateDownloadProgress: vi.fn((next: UpdateDownloadProgressListener) => {
      listener = next;
      return Promise.resolve(1);
    }),
    unsubscribe: vi.fn(() => Promise.resolve()),
  };
  return { backend, progress: (status: UpdateDownloadStatus) => act(() => listener(status)) };
}

function open(backend: ReturnType<typeof updateBackend>["backend"], onAutoCheck = vi.fn()) {
  render(<UpdateDialog backend={backend} autoCheck labels={zh.release.update} onAutoCheck={onAutoCheck} onClose={vi.fn()} />);
}

test("设置 → 语言 picks the interface language; 跟随系统 keeps the system's", () => {
  expect(localeOf("System", "ja-JP")).toBe("ja-JP");
  expect(localeOf("ZhCn", "en-US")).toBe("zh-CN");
  expect(localeOf("En", "zh-CN")).toBe("en");
});

test("a new version can be downloaded with progress, cancelled, and installed once downloaded", async () => {
  const { backend, progress } = updateBackend(() => Promise.resolve({ available: true, version: "0.3.0", notes: "胜率图更快", published_at: "2026-11-20T08:00:00Z" }));
  open(backend);
  expect((await screen.findByTestId("update-state")).textContent).toBe("有新版本 0.3.0");
  expect(screen.getByTestId("update-notes").textContent).toBe("胜率图更快");
  fireEvent.click(screen.getByTestId("update-download"));
  progress({ ...idle, phase: "Downloading", downloaded_bytes: 26_214_400, total_bytes: 52_428_800, version: "0.3.0" });
  expect((await screen.findByTestId("update-progress")).textContent).toContain("已下载 25.0 / 50.0 MB");
  fireEvent.click(screen.getByTestId("update-cancel"));
  await vi.waitFor(() => expect(screen.queryByTestId("update-progress")).toBeNull());

  progress({ ...idle, phase: "Downloaded", downloaded_bytes: 52_428_800, total_bytes: 52_428_800, version: "0.3.0" });
  fireEvent.click(await screen.findByTestId("update-install"));
  expect(backend.installUpdate).toHaveBeenCalledTimes(1);
});

test("latest, unsigned builds and failures each say so, with nothing to download", async () => {
  const latest = updateBackend(() => Promise.resolve({ available: false, version: null, notes: null, published_at: null }));
  open(latest.backend);
  expect((await screen.findByTestId("update-state")).getAttribute("data-state")).toBe("latest");
  expect(screen.queryByTestId("update-download")).toBeNull();
  cleanup();

  const unsigned = updateBackend(() => Promise.reject({ type: "UpdateSigningNotConfigured" }));
  open(unsigned.backend);
  await vi.waitFor(() => expect(screen.getByTestId("update-state").getAttribute("data-state")).toBe("notConfigured"));
  expect(screen.getByTestId("update-state").textContent).toContain("GitHub Releases");
  cleanup();

  const offline = updateBackend(() => Promise.reject({ type: "App", data: "network unreachable" }));
  const onAutoCheck = vi.fn();
  open(offline.backend, onAutoCheck);
  await vi.waitFor(() => expect(screen.getByTestId("update-state").textContent).toContain("network unreachable"));
  fireEvent.click(screen.getByTestId("update-auto-check"));
  expect(onAutoCheck).toHaveBeenCalledWith(false);
});

test("诊断与日志 shows the log folder, counts the full log down, and exports a bundle to show in its folder", async () => {
  vi.useFakeTimers();
  const backend = {
    logDirectory: vi.fn(() => Promise.resolve("/data/logs")),
    fullLogStatus: vi.fn(),
    exportDiagnostics: vi.fn(() => Promise.resolve("/data/diagnostics/lizzie-20261112.zip")),
    revealInFolder: vi.fn(() => Promise.resolve()),
  };
  const onStop = vi.fn();
  render(
    <DiagnosticsDialog backend={backend} fullLog={{ enabled: true, remaining_seconds: 1800 }} labels={zh.release.diagnostics} onStart={vi.fn()} onStop={onStop} onClose={vi.fn()} />,
  );
  await act(async () => {});
  expect(screen.getByTestId("diagnostics-log-dir").textContent).toBe("/data/logs");
  expect(screen.getByTestId("diagnostics-full-log").textContent).toBe("已开启，剩余 30:00");
  act(() => vi.advanceTimersByTime(61_000));
  expect(screen.getByTestId("diagnostics-full-log").textContent).toBe("已开启，剩余 28:59");
  fireEvent.click(screen.getByTestId("diagnostics-stop"));
  expect(onStop).toHaveBeenCalledTimes(1);

  fireEvent.click(screen.getByTestId("diagnostics-export"));
  await act(async () => {});
  expect(screen.getByTestId("diagnostics-bundle").textContent).toContain("lizzie-20261112.zip");
  fireEvent.click(screen.getByTestId("diagnostics-reveal"));
  fireEvent.click(screen.getByTestId("diagnostics-open-logs"));
  expect(backend.revealInFolder.mock.calls).toEqual([["/data/diagnostics/lizzie-20261112.zip"], ["/data/logs"]]);
  vi.useRealTimers();
});
