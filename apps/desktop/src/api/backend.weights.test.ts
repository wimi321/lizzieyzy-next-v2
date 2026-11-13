import { beforeEach, expect, test, vi } from "vitest";
import type { AssetDownloadStatus, EngineDiagnostics, Settings, WireAppEvent } from "./backend";

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

const running: AssetDownloadStatus = {
  phase: "Downloading", asset_id: "b11", downloaded_bytes: 1024, total_bytes: 4096,
  path: null, error: null,
};
const diagnostics: EngineDiagnostics = {
  profile: null,
  checks: [{ kind: "ConfigValid", status: "Error", path: "/data/analysis.cfg", detail: "invalid", repairs: ["RegenerateConfig"] }],
};

test("weight and diagnostic IPC forwards IDs, repair kinds, replies and errors", async () => {
  const backend = await import("./backend");
  const cases: [() => Promise<unknown>, string, object | undefined][] = [
    [backend.officialWeights, "official_weights", undefined],
    [() => backend.startAssetDownload("b11"), "start_asset_download", { assetId: "b11" }],
    [backend.cancelAssetDownload, "cancel_asset_download", undefined],
    [backend.assetDownloadStatus, "asset_download_status", undefined],
    [backend.accelerationPlan, "acceleration_plan", undefined],
    [() => backend.useOfficialWeight("b11"), "use_official_weight", { assetId: "b11" }],
    [backend.diagnoseEngine, "diagnose_engine", undefined],
    [() => backend.repairEngine("UseBundledModel"), "repair_engine", { repair: "UseBundledModel" }],
    [() => backend.repairEngine("InstallRuntime"), "repair_engine", { repair: "InstallRuntime" }],
  ];
  for (const [call, name, args] of cases) {
    ipc.invoke.mockResolvedValueOnce(diagnostics);
    expect(await call()).toBe(diagnostics);
    expect(ipc.invoke).toHaveBeenLastCalledWith(...(args ? [name, args] : [name]));
  }
  const error = { type: "App", data: "asset size or SHA-256 verification failed" };
  ipc.invoke.mockRejectedValueOnce(error);
  await expect(backend.startAssetDownload("b11")).rejects.toBe(error);
});

test("download events share the ordered connection and are excluded from existing UI consumers", async () => {
  const backend = await import("./backend");
  ipc.invoke.mockImplementation(async (command) => {
    if (command === "subscribe") {
      ipc.channels[0]!.onmessage({ type: "AssetDownloadProgress", data: running });
      return 61;
    }
  });
  const ui = vi.fn();
  const download = vi.fn();
  const benchmark = vi.fn();
  const ids = await Promise.all([
    backend.subscribe(ui), backend.subscribeAssetDownloadProgress(download), backend.subscribeBenchmarkProgress(benchmark),
  ]);
  expect(ipc.channels).toHaveLength(1);
  expect(download.mock.calls).toEqual([[running]]);
  expect(ui).not.toHaveBeenCalled();
  expect(benchmark).not.toHaveBeenCalled();
  await backend.unsubscribe(ids[0]!);
  await backend.unsubscribe(ids[2]!);
  const completed: AssetDownloadStatus = { ...running, phase: "Completed", downloaded_bytes: 4096, path: "/data/weights/b11.bin.gz" };
  ipc.channels[0]!.onmessage({ type: "AssetDownloadProgress", data: completed });
  expect(download).toHaveBeenLastCalledWith(completed);
  await backend.unsubscribe(ids[1]!);
  expect(ipc.invoke).toHaveBeenLastCalledWith("unsubscribe", { id: 61 });
  ipc.channels[0]!.onmessage({ type: "AssetDownloadProgress", data: running });
  expect(download).toHaveBeenCalledTimes(2);
});

test("mock queries supply defaults without consuming the next mutation; progress is scripted", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const cancelled: AssetDownloadStatus = { ...running, phase: "Cancelled" };
  mock.setScript({ commands: [
    { command: "startAssetDownload", result: running, events: [{ type: "AssetDownloadProgress", data: running }] },
    { command: "cancelAssetDownload", result: cancelled, events: [{ type: "AssetDownloadProgress", data: cancelled }] },
  ] });
  expect(await mock.officialWeights()).toEqual([]);
  expect((await mock.assetDownloadStatus()).phase).toBe("Idle");
  expect((await mock.accelerationPlan()).reason).toBe("UnsupportedPlatform");
  expect((await mock.accelerationPlan()).archives_only).toBe(false);
  expect(await mock.diagnoseEngine()).toEqual({ profile: null, checks: [] });
  expect(mock.recordedCalls()).toEqual([]);
  const ui = vi.fn();
  const progress = vi.fn();
  const uiId = await mock.subscribe(ui);
  const progressId = await mock.subscribeAssetDownloadProgress(progress);
  expect(await mock.startAssetDownload("b11")).toEqual(running);
  expect(await mock.assetDownloadStatus()).toEqual(running);
  expect(await mock.cancelAssetDownload()).toEqual(cancelled);
  expect(await mock.assetDownloadStatus()).toEqual(cancelled);
  expect(progress.mock.calls).toEqual([[running], [cancelled]]);
  expect(ui).not.toHaveBeenCalled();
  expect(mock.recordedCalls()).toEqual([
    { command: "startAssetDownload", args: ["b11"] }, { command: "cancelAssetDownload", args: [] },
  ]);
  await mock.unsubscribe(progressId);
  await mock.unsubscribe(uiId);
});

test("mock new DTO defaults preserve explicit values and all mutations require a script", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  mock.setScript({ commands: [
    { command: "officialWeights", result: [{ id: "b11", downloaded: true, published_at: null }] },
    { command: "assetDownloadStatus", result: { phase: "Failed", downloaded_bytes: 0, error: "checksum" } },
    { command: "accelerationPlan", result: { reason: "NoNvidiaGpu" } },
    { command: "diagnoseEngine", result: { checks: [{ kind: "ModelReadable" }] } },
    { command: "repairEngine", result: diagnostics },
    { command: "useOfficialWeight", result: { engine: null } as Settings },
  ] });
  const weight = (await mock.officialWeights())[0]!;
  expect(weight.id).toBe("b11");
  expect(weight.downloaded).toBe(true);
  expect(weight.size_bytes).toBe(0);
  expect(weight.path).toBeNull();
  const status = await mock.assetDownloadStatus();
  expect(status.phase).toBe("Failed");
  expect(status.downloaded_bytes).toBe(0);
  expect(status.total_bytes).toBe(0);
  expect(status.error).toBe("checksum");
  expect((await mock.accelerationPlan()).recommended_assets).toEqual([]);
  const report = await mock.diagnoseEngine();
  expect(report.profile).toBeNull();
  expect(report.checks?.[0]?.status).toBe("Skipped");
  expect(await mock.repairEngine("RegenerateConfig")).toEqual(diagnostics);
  expect(await mock.useOfficialWeight("b11")).toEqual({ engine: null });
  await expect(mock.startAssetDownload("b11")).rejects.toThrow("Expected mock command");
  await expect(mock.cancelAssetDownload()).rejects.toThrow("Expected mock command");
  await expect(mock.repairEngine("UseBundledProgram")).rejects.toThrow("Expected mock command");
  await expect(mock.useOfficialWeight("b11")).rejects.toThrow("Expected mock command");
});

test("runtime installation remains a scripted repair and forwards progress before the resulting diagnosis", async () => {
  const mock = await vi.importActual<typeof import("../../e2e/mock-backend")>("../../e2e/mock-backend");
  const installed: EngineDiagnostics = {
    profile: null,
    checks: [{ kind: "RuntimeComponents", status: "Ok", path: "/data/runtime/cuda", detail: null, repairs: [] }],
  };
  const downloading: AssetDownloadStatus = { ...running, asset_id: "runtime-cuda_cudart" };
  const installing: AssetDownloadStatus = { ...downloading, phase: "Installing", downloaded_bytes: 4096 };
  const completed: AssetDownloadStatus = { ...downloading, phase: "Completed", downloaded_bytes: 4096, path: "/data/runtime/cuda" };
  mock.setScript({ commands: [
    { command: "accelerationPlan", result: { reason: "NvidiaDetected", archives_only: false } },
    { command: "repairEngine", result: installed, events: [
      { type: "AssetDownloadProgress", data: downloading },
      { type: "AssetDownloadProgress", data: installing },
      { type: "AssetDownloadProgress", data: completed },
    ] },
    { command: "diagnoseEngine", result: installed },
  ] });
  const progress = vi.fn();
  const ui = vi.fn();
  const progressId = await mock.subscribeAssetDownloadProgress(progress);
  const uiId = await mock.subscribe(ui);
  const plan = await mock.accelerationPlan();
  expect(plan.archives_only).toBe(false);
  expect(plan.recommended_assets).toEqual([]);
  expect(await mock.repairEngine("InstallRuntime")).toEqual(installed);
  expect(progress.mock.calls).toEqual([[downloading], [installing], [completed]]);
  expect(await mock.diagnoseEngine()).toEqual(installed);
  expect(ui).not.toHaveBeenCalled();
  expect(mock.recordedCalls()).toEqual([
    { command: "accelerationPlan", args: [] },
    { command: "repairEngine", args: ["InstallRuntime"] },
    { command: "diagnoseEngine", args: [] },
  ]);
  await expect(mock.repairEngine("InstallRuntime")).rejects.toThrow("Expected mock command");
  await mock.unsubscribe(progressId);
  await mock.unsubscribe(uiId);
});
