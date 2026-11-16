import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";
import type { AccelerationPlan, AccelerationProgress, AssetDownloadStatus, Settings } from "@/api/backend";
import zh from "../locales/zh-CN.json";
import type { ToolsTab } from "../engines/useEngineTools";
import { AccelerationHint, AccelerationPanel } from "./AccelerationPanel";
import { defaultChoice, failureOf, hintGpu, size } from "./format";
import { useAcceleration } from "./useAcceleration";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

const RTX = { name: "NVIDIA GeForce RTX 3070 Laptop GPU", compute_capability: 86, driver_version: "560.76", nvidia: true };
const NVIDIA: AccelerationPlan = {
  reason: "NvidiaDetected",
  gpus: [RTX],
  current: "eigen",
  options: [
    { backend: "cuda", profile_id: "cuda12.8-cudnn9", installed: false, download_bytes: 1_833_244_425, recommended: true, supported: true, note: "DriverMayBeTooOld" },
    { backend: "tensorrt", profile_id: "cuda12.8-cudnn9-tensorrt", installed: false, download_bytes: 3_674_989_020, recommended: true, supported: true, note: "DriverMayBeTooOld" },
    { backend: "opencl", profile_id: "opencl", installed: true, download_bytes: 0, recommended: false, supported: true, note: null },
  ],
};
const OLD_NVIDIA: AccelerationPlan = {
  ...NVIDIA,
  gpus: [{ name: "NVIDIA GeForce GTX 1060", compute_capability: 61, driver_version: "576.02", nvidia: true }],
  options: [
    { backend: "cuda", profile_id: "cuda12.8-cudnn9", installed: false, download_bytes: 1_833_244_425, recommended: true, supported: true, note: null },
    { backend: "tensorrt", profile_id: "cuda12.8-cudnn9-tensorrt", installed: false, download_bytes: 3_674_989_020, recommended: false, supported: false, note: null },
    { backend: "opencl", profile_id: "opencl", installed: false, download_bytes: 3_127_940, recommended: false, supported: true, note: null },
  ],
};
const AMD: AccelerationPlan = {
  reason: "NoNvidiaGpu",
  gpus: [{ name: "AMD Radeon RX 6700 XT", compute_capability: null, driver_version: null, nvidia: false }],
  current: "eigen",
  options: [
    { backend: "cuda", profile_id: "cuda12.8-cudnn9", installed: false, download_bytes: 1_833_244_425, recommended: false, supported: false, note: null },
    { backend: "tensorrt", profile_id: "cuda12.8-cudnn9-tensorrt", installed: false, download_bytes: 3_674_989_020, recommended: false, supported: false, note: null },
    { backend: "opencl", profile_id: "opencl", installed: false, download_bytes: 3_127_940, recommended: true, supported: true, note: null },
  ],
};
const NOTHING: AccelerationPlan = { ...AMD, gpus: [], options: AMD.options!.map((option) => ({ ...option, recommended: false })) };
const MAC: AccelerationPlan = { reason: "UnsupportedPlatform", gpus: [], current: "external", options: [] };
const ON_CUDA: AccelerationPlan = { ...NVIDIA, current: "cuda", options: NVIDIA.options!.map((option) => (option.backend === "cuda" ? { ...option, installed: true, download_bytes: 0 } : option)) };
const SETTINGS = { version: 1, acceleration_hint_dismissed: false } as Settings;
const DOWNLOADING: AssetDownloadStatus = { phase: "Downloading", asset_id: "cuda12.8-cudnn9", downloaded_bytes: 524_288_000, total_bytes: 1_833_244_425, path: null, error: null };

function fake(plan: AccelerationPlan) {
  let listener: ((progress: AccelerationProgress) => void) | null = null;
  const backend = {
    accelerationPlan: vi.fn().mockResolvedValue(plan),
    enableAcceleration: vi.fn(),
    disableAcceleration: vi.fn().mockResolvedValue({ profile: null, checks: [] }),
    dismissAccelerationHint: vi.fn().mockResolvedValue(undefined),
    subscribeAccelerationProgress: vi.fn(async (next: (progress: AccelerationProgress) => void) => {
      listener = next;
      return 3;
    }),
    unsubscribe: vi.fn().mockResolvedValue(undefined),
    cancelAssetDownload: vi.fn().mockResolvedValue(DOWNLOADING),
    pauseAssetDownload: vi.fn().mockResolvedValue({ ...DOWNLOADING, phase: "Paused" }),
    startAssetDownload: vi.fn().mockResolvedValue(DOWNLOADING),
    settings: vi.fn().mockResolvedValue(SETTINGS),
  };
  return { backend, push: (progress: AccelerationProgress) => act(() => listener?.(progress)) };
}

type Fake = ReturnType<typeof fake>["backend"];

function Harness({ backend, settings, download = null, onSettings, openTab }: { backend: Fake; settings: Settings | null; download?: AssetDownloadStatus | null; onSettings(settings: Settings): void; openTab(tab: ToolsTab): void }) {
  const acceleration = useAcceleration(backend as unknown as Parameters<typeof useAcceleration>[0], { settings, toolsOpen: true, onSettings, openTab });
  return (
    <>
      <AccelerationPanel acceleration={acceleration} download={download} labels={zh.acceleration} />
      <AccelerationHint acceleration={acceleration} labels={zh.acceleration} />
    </>
  );
}

async function show(plan: AccelerationPlan, settings: Settings | null = { ...SETTINGS, acceleration_hint_dismissed: true }) {
  const { backend, push } = fake(plan);
  const onSettings = vi.fn();
  const openTab = vi.fn();
  const view = render(<Harness backend={backend} settings={settings} onSettings={onSettings} openTab={openTab} />);
  await act(async () => undefined);
  const rerender = (download: AssetDownloadStatus | null) => view.rerender(<Harness backend={backend} settings={settings} download={download} onSettings={onSettings} openTab={openTab} />);
  return { backend, push, onSettings, openTab, rerender };
}

const text = (id: string) => screen.getByTestId(id).textContent ?? "";
const radio = (backend: string) => screen.getByTestId(`accel-option-${backend}`).querySelector("input") as HTMLInputElement;

describe("the plan's rules", () => {
  test("the default choice, the hint's GPU and sizes", () => {
    expect(defaultChoice(NVIDIA)).toBe("cuda");
    expect(defaultChoice(ON_CUDA)).toBe("tensorrt");
    expect(defaultChoice(AMD)).toBe("opencl");
    expect(defaultChoice(NOTHING)).toBe("opencl");
    expect(defaultChoice(MAC)).toBeNull();
    expect(hintGpu(NVIDIA)).toBe(RTX.name);
    expect(hintGpu(AMD)).toBe("AMD Radeon RX 6700 XT");
    expect(hintGpu(NOTHING)).toBeNull();
    expect(hintGpu(ON_CUDA)).toBeNull();
    expect(hintGpu(MAC)).toBeNull();
    expect(size(1_833_244_425)).toBe("1.7 GB");
    expect(size(3_127_940)).toBe("3.0 MB");
  });

  test("rejected commands are read from Rust's typed errors", () => {
    const wrap = (data: unknown) => ({ type: "Acceleration", data });
    expect(failureOf(wrap({ type: "Failed", data: { reason: "out of memory", stderr_tail: ["a", "b"] } }))).toEqual({ kind: "Failed", reason: "out of memory", tail: ["a", "b"] });
    expect(failureOf(wrap({ type: "TimedOut", data: { stderr_tail: ["c"] } }))).toEqual({ kind: "TimedOut", tail: ["c"] });
    expect(failureOf(wrap({ type: "Cancelled", data: { stderr_tail: [] } }))).toEqual({ kind: "Cancelled", tail: [] });
    expect(failureOf(wrap({ type: "Busy" }))).toEqual({ kind: "Busy" });
    expect(failureOf(wrap({ type: "UnsupportedBackend" }))).toEqual({ kind: "Unsupported" });
    expect(failureOf({ type: "Engine", data: "boom" })).toEqual({ kind: "Other", reason: "boom" });
    expect(failureOf(new Error("lost"))).toEqual({ kind: "Other", reason: "lost" });
  });
});

describe("引擎一键设置 → 显卡提速", () => {
  test("NVIDIA: CUDA chosen and recommended, TensorRT recommended too, an old driver noted, OpenCL installed", async () => {
    await show(NVIDIA);
    expect(text("accel-gpus")).toBe("NVIDIA GeForce RTX 3070 Laptop GPU（驱动 560.76）");
    expect(text("accel-current")).toBe("CPU");
    expect(radio("cuda").checked).toBe(true);
    const cuda = text("accel-option-cuda");
    expect(cuda).toContain("推荐");
    expect(cuda).toContain("需下载 1.7 GB");
    expect(cuda).toContain(zh.acceleration.oldDriver);
    expect(text("accel-option-tensorrt")).toContain("需下载 3.4 GB");
    expect(text("accel-option-opencl")).toContain("已安装");
    expect(text("accel-option-opencl")).not.toContain("推荐");
    expect(screen.queryByTestId("accel-disable")).toBeNull();
    fireEvent.click(radio("tensorrt"));
    expect(radio("tensorrt").checked).toBe(true);
  });

  test("an older NVIDIA card cannot use TensorRT; it is greyed with the reason", async () => {
    await show(OLD_NVIDIA);
    expect(radio("tensorrt").disabled).toBe(true);
    expect(text("accel-option-tensorrt")).toContain(zh.acceleration.unsupported.tensorrt);
    expect(text("accel-option-cuda")).not.toContain(zh.acceleration.oldDriver);
  });

  test("only another graphics card: OpenCL is recommended, CUDA and TensorRT are greyed", async () => {
    await show(AMD);
    expect(text("accel-gpus")).toBe("AMD Radeon RX 6700 XT");
    expect(radio("opencl").checked).toBe(true);
    expect(text("accel-option-opencl")).toContain("推荐");
    expect(radio("cuda").disabled).toBe(true);
    expect(text("accel-option-cuda")).toContain(zh.acceleration.unsupported.cuda);
  });

  test("nothing detected: OpenCL can still be tried, nothing is recommended", async () => {
    await show(NOTHING);
    expect(text("accel-gpus")).toBe(zh.acceleration.noGpu);
    expect(radio("opencl").checked).toBe(true);
    expect(screen.getByTestId("tools-acceleration").textContent).not.toContain("推荐");
  });

  test("another system: the reason only, no buttons", async () => {
    await show(MAC);
    expect(screen.getByTestId("tools-acceleration").textContent).toContain(zh.acceleration.reasons.UnsupportedPlatform);
    expect(screen.queryByTestId("accel-enable")).toBeNull();
    expect(screen.queryByTestId("accel-disable")).toBeNull();
  });

  test("enabling goes through download, start and tuning, then offers the benchmark", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "Date"] });
    const { backend, push, onSettings, openTab, rerender } = await show(AMD);
    let finish: (settings: Settings) => void = () => undefined;
    backend.enableAcceleration.mockReturnValue(new Promise<Settings>((resolve) => (finish = resolve)));
    fireEvent.click(screen.getByTestId("accel-enable"));
    expect(backend.enableAcceleration).toHaveBeenCalledWith("opencl");
    expect(screen.getByTestId<HTMLButtonElement>("accel-enable").disabled).toBe(true);
    expect(radio("opencl").disabled).toBe(true);
    rerender({ ...DOWNLOADING, asset_id: "opencl", downloaded_bytes: 1_048_576, total_bytes: 3_127_940 });
    expect(text("accel-run")).toContain("正在下载 OpenCL 组件：1.0 / 3.0 MB");
    push({ backend: "opencl", phase: "Starting", output_tail: [] });
    act(() => vi.advanceTimersByTime(4000));
    expect(text("accel-run")).toContain("正在启动 OpenCL 引擎…（已用 4 秒）");
    push({ backend: "opencl", phase: "Tuning", output_tail: ["Tuning 40/55 ..."] });
    act(() => vi.advanceTimersByTime(60_000));
    expect(text("accel-run")).toBe(`${zh.acceleration.tuningOpenCL.replace("{seconds}", "64")}${zh.acceleration.cancel}`);
    // Late or unrelated events never revive or replace the run.
    push({ backend: "cuda", phase: "Starting", output_tail: [] });
    expect(screen.getByTestId("accel-run").dataset.phase).toBe("Tuning");
    backend.accelerationPlan.mockResolvedValue({ ...AMD, current: "opencl" });
    await act(async () => finish({ ...SETTINGS, main_engine_id: "e1" } as Settings));
    await act(async () => undefined);
    expect(onSettings).toHaveBeenCalledWith(expect.objectContaining({ main_engine_id: "e1" }));
    expect(text("accel-outcome")).toContain("已改用 OpenCL。");
    expect(screen.queryByTestId("accel-run")).toBeNull();
    push({ backend: "opencl", phase: "Tuning", output_tail: [] });
    expect(screen.queryByTestId("accel-run")).toBeNull();
    expect(text("accel-current")).toBe("OpenCL");
    fireEvent.click(screen.getByTestId("accel-benchmark"));
    expect(openTab).toHaveBeenCalledWith("benchmark");
  });

  test("TensorRT's first start says it builds its cache", async () => {
    const { backend, push } = await show(NVIDIA);
    backend.enableAcceleration.mockReturnValue(new Promise(() => undefined));
    fireEvent.click(radio("tensorrt"));
    fireEvent.click(screen.getByTestId("accel-enable"));
    push({ backend: "tensorrt", phase: "Tuning", output_tail: [] });
    expect(text("accel-run")).toContain("第一次使用 TensorRT 要先生成优化缓存");
  });

  test("the download pauses and resumes the profile, and can be cancelled", async () => {
    const { backend, rerender } = await show(NVIDIA);
    backend.enableAcceleration.mockReturnValue(new Promise(() => undefined));
    fireEvent.click(screen.getByTestId("accel-enable"));
    rerender(DOWNLOADING);
    fireEvent.click(screen.getByTestId("accel-pause"));
    expect(backend.pauseAssetDownload).toHaveBeenCalled();
    rerender({ ...DOWNLOADING, phase: "Paused" });
    expect(text("accel-run")).toContain("已暂停：500.0 / 1748.3 MB");
    fireEvent.click(screen.getByTestId("accel-pause"));
    expect(backend.startAssetDownload).toHaveBeenCalledWith("cuda12.8-cudnn9");
    rerender({ ...DOWNLOADING, phase: "Installing" });
    expect(text("accel-run")).toContain("正在安装 CUDA 组件…");
    expect(screen.queryByTestId("accel-pause")).toBeNull();
    fireEvent.click(screen.getByTestId("accel-cancel"));
    expect(backend.cancelAssetDownload).toHaveBeenCalled();
  });

  test("a failed start says the previous engine is back, with KataGo's last lines to copy", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    const { backend } = await show(NVIDIA);
    backend.enableAcceleration.mockRejectedValue({ type: "Acceleration", data: { type: "Failed", data: { reason: "CUDA error: out of memory", stderr_tail: ["Cuda backend thread 0: Found GPU", "CUDA error: out of memory"] } } });
    await act(async () => fireEvent.click(screen.getByTestId("accel-enable")));
    const outcome = text("accel-outcome");
    expect(outcome).toContain("没有成功，已退回原来的引擎。");
    expect(outcome).toContain("CUDA error: out of memory");
    expect(screen.getByTestId("accel-output").querySelector("summary")?.textContent).toBe("KataGo 的输出（最后 2 行）");
    await act(async () => fireEvent.click(screen.getByTestId("accel-copy")));
    expect(writeText).toHaveBeenCalledWith("Cuda backend thread 0: Found GPU\nCUDA error: out of memory");
    expect(text("accel-copy")).toBe("已复制");
    expect(screen.getByTestId<HTMLButtonElement>("accel-enable").disabled).toBe(false);
  });

  test("a timeout and a cancel are told apart", async () => {
    const { backend } = await show(NVIDIA);
    backend.enableAcceleration.mockRejectedValueOnce({ type: "Acceleration", data: { type: "TimedOut", data: { stderr_tail: [] } } });
    await act(async () => fireEvent.click(screen.getByTestId("accel-enable")));
    expect(text("accel-outcome")).toContain("CUDA 引擎没能在规定时间内启动。");
    backend.enableAcceleration.mockRejectedValueOnce({ type: "Acceleration", data: { type: "Cancelled", data: { stderr_tail: [] } } });
    await act(async () => fireEvent.click(screen.getByTestId("accel-enable")));
    expect(screen.getByTestId("accel-outcome").dataset.outcome).toBe("cancelled");
    expect(text("accel-outcome")).toBe("已取消，仍在用原来的引擎。");
  });

  test("改回 CPU restores the bundled engine and reloads the settings", async () => {
    const { backend, onSettings } = await show(ON_CUDA);
    expect(text("accel-current")).toBe("CUDA");
    expect(text("accel-option-cuda")).toContain("正在使用");
    expect(radio("cuda").disabled).toBe(true);
    expect(radio("tensorrt").checked).toBe(true);
    backend.accelerationPlan.mockResolvedValue(NVIDIA);
    await act(async () => fireEvent.click(screen.getByTestId("accel-disable")));
    expect(backend.disableAcceleration).toHaveBeenCalled();
    expect(onSettings).toHaveBeenCalledWith(SETTINGS);
    expect(text("accel-outcome")).toBe("已改回 CPU 引擎。");
    expect(screen.queryByTestId("accel-disable")).toBeNull();
  });
});

describe("the one-time hint", () => {
  test("shown on a GPU machine running the CPU engine; 去开启 opens the page without dismissing it for good", async () => {
    const { backend, openTab } = await show(NVIDIA, SETTINGS);
    expect(text("accel-hint")).toContain("检测到 NVIDIA GeForce RTX 3070 Laptop GPU，开启显卡提速后分析会快很多。");
    fireEvent.click(screen.getByTestId("accel-hint-open"));
    expect(openTab).toHaveBeenCalledWith("acceleration");
    expect(screen.queryByTestId("accel-hint")).toBeNull();
    expect(backend.dismissAccelerationHint).not.toHaveBeenCalled();
  });

  test.each(["accel-hint-dismiss", "accel-hint-close"])("%s saves that it is not shown again", async (id) => {
    const { backend } = await show(AMD, SETTINGS);
    expect(text("accel-hint")).toContain("AMD Radeon RX 6700 XT");
    fireEvent.click(screen.getByTestId(id));
    expect(backend.dismissAccelerationHint).toHaveBeenCalledTimes(1);
    expect(screen.queryByTestId("accel-hint")).toBeNull();
  });

  test.each<[string, AccelerationPlan]>([
    ["nothing recommended", NOTHING],
    ["already on a GPU", ON_CUDA],
    ["another system", MAC],
  ])("not shown: %s", async (_name, plan) => {
    await show(plan, SETTINGS);
    expect(screen.queryByTestId("accel-hint")).toBeNull();
  });

  test("once dismissed, the plan is not even read for it", async () => {
    const { backend } = fake(NVIDIA);
    render(<HookOnly backend={backend} settings={{ ...SETTINGS, acceleration_hint_dismissed: true }} />);
    await act(async () => undefined);
    expect(backend.accelerationPlan).not.toHaveBeenCalled();
    expect(screen.queryByTestId("accel-hint")).toBeNull();
  });
});

/** The main window without the dialog open: only the hint can appear. */
function HookOnly({ backend, settings }: { backend: Fake; settings: Settings }) {
  const acceleration = useAcceleration(backend as unknown as Parameters<typeof useAcceleration>[0], { settings, toolsOpen: false, onSettings: () => undefined, openTab: () => undefined });
  return <AccelerationHint acceleration={acceleration} labels={zh.acceleration} />;
}
