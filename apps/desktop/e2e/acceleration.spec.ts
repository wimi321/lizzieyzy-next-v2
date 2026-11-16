import { expect, test, type Page } from "@playwright/test";
import type { AccelerationPlan, CommandError, Settings, WireAppEvent } from "../src/api/backend";
import { documentChanged, midGame } from "./fixtures";
import type { MockCommand, MockScript } from "./mock-backend";

test.use({ viewport: { width: 1280, height: 820 }, deviceScaleFactor: 1, locale: "zh-CN" });

const CPU = { program: "/app/katago/katago.exe", model: "/app/weights/b11.bin.gz", config: "", extra_args: [] };
const SETTINGS: Settings = {
  version: 1,
  engine: CPU,
  pondering: false,
  fox_history: [],
  move_rank: { mode: "Auto", winrate_thresholds: [1, 3, 6, 12, 24], score_thresholds: [0.5, 1.5, 3, 6, 12] },
  tsumego: { to_play: "Inherit", attacker: "Auto", ko_threat: "Both", wall_distance: 1 },
  engines: [{ id: "e1", name: "KataGo", profile: CPU }],
  main_engine_id: "e1",
  acceleration_hint_dismissed: false,
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
const NVIDIA: AccelerationPlan = {
  reason: "NvidiaDetected",
  gpus: [{ name: "NVIDIA GeForce RTX 3070 Laptop GPU", compute_capability: 86, driver_version: "560.76", nvidia: true }],
  current: "eigen",
  options: [
    { backend: "cuda", profile_id: "cuda12.8-cudnn9", installed: true, download_bytes: 0, recommended: true, supported: true, note: "DriverMayBeTooOld" },
    { backend: "tensorrt", profile_id: "cuda12.8-cudnn9-tensorrt", installed: false, download_bytes: 1_849_074_844, recommended: true, supported: true, note: "DriverMayBeTooOld" },
    { backend: "opencl", profile_id: "opencl", installed: false, download_bytes: 3_127_940, recommended: false, supported: true, note: null },
  ],
};

async function start(page: Page, commands: MockCommand[], settings: Settings = SETTINGS) {
  const script: MockScript = { initialEvents: [documentChanged(midGame(), 3)], commands: [{ command: "settings", result: settings }, ...commands] };
  await page.addInitScript((initial) => {
    window.__mockInitialScript = initial;
  }, script);
  await page.goto("/");
  await expect(page.getByTestId("board")).toHaveAttribute("data-ready", "true");
}

const push = (page: Page, events: WireAppEvent[]) => page.evaluate((list) => window.__mockBackend?.pushEvents(list), events);
const calls = (page: Page) => page.evaluate(() => (window.__mockBackend?.recordedCalls() ?? []).map((call) => call.command));
const progress = (phase: "Downloading" | "Starting" | "Tuning", tail: string[] = []): WireAppEvent => ({ type: "AccelerationProgress", data: { backend: "opencl", phase, output_tail: tail } });

test("the hint leads to 显卡提速; OpenCL downloads, starts and tunes, then the benchmark is offered", async ({ page }) => {
  const downloading = { phase: "Downloading", asset_id: "opencl", downloaded_bytes: 1_048_576, total_bytes: 3_127_940, path: null, error: null } as const;
  await start(page, [
    { command: "accelerationPlan", result: AMD },
    { command: "accelerationPlan", result: AMD },
    {
      command: "enableAcceleration",
      result: { ...SETTINGS, engine: { ...CPU, program: "/data/runtimes/opencl/v1/bin/katago.exe" } },
      hold: true,
      events: [{ type: "AssetDownloadProgress", data: downloading }, progress("Downloading")],
    },
    { command: "accelerationPlan", result: { ...AMD, current: "opencl", options: AMD.options!.map((option) => (option.backend === "opencl" ? { ...option, installed: true, download_bytes: 0 } : option)) } },
  ]);
  const hint = page.getByTestId("accel-hint");
  await expect(hint).toContainText("检测到 AMD Radeon RX 6700 XT，开启显卡提速后分析会快很多。");
  await hint.getByTestId("accel-hint-open").click();
  await expect(hint).toBeHidden();
  const dialog = page.getByTestId("benchmark-dialog");
  const panel = dialog.getByTestId("tools-acceleration");
  await expect(dialog.getByTestId("tools-tab-acceleration")).toHaveAttribute("aria-selected", "true");
  await expect(panel.getByTestId("accel-current")).toHaveText("CPU");
  await expect(panel.getByTestId("accel-option-opencl").locator("input")).toBeChecked();
  await expect(panel.getByTestId("accel-option-cuda").locator("input")).toBeDisabled();

  await panel.getByTestId("accel-enable").click();
  const run = panel.getByTestId("accel-run");
  await expect(run).toContainText("正在下载 OpenCL 组件：1.0 / 3.0 MB");
  await expect(panel.getByTestId("accel-enable")).toBeDisabled();
  await push(page, [{ type: "AssetDownloadProgress", data: { ...downloading, phase: "Completed", downloaded_bytes: 3_127_940 } }, progress("Starting")]);
  await expect(run).toHaveAttribute("data-phase", "Starting");
  await expect(run).toContainText("正在启动 OpenCL 引擎…");
  await push(page, [progress("Tuning", ["Tuning 40/55 ..."])]);
  await expect(run).toContainText("第一次使用 OpenCL 要先调优，可能需要几分钟，请不要关闭软件。");

  await page.evaluate(() => window.__mockBackend?.releaseHeld());
  await expect(panel.getByTestId("accel-outcome")).toContainText("已改用 OpenCL。");
  await expect(run).toBeHidden();
  await expect(panel.getByTestId("accel-current")).toHaveText("OpenCL");
  await expect(panel.getByTestId("accel-option-opencl")).toContainText("正在使用");
  await panel.getByTestId("accel-benchmark").click();
  await expect(dialog.getByTestId("tools-tab-benchmark")).toHaveAttribute("aria-selected", "true");
  expect(await calls(page)).toEqual(["settings", "accelerationPlan", "accelerationPlan", "enableAcceleration", "accelerationPlan"]);
});

test("a failed start keeps the previous engine and shows KataGo's last lines", async ({ page }) => {
  const failed: CommandError = {
    type: "Acceleration",
    data: {
      type: "Failed",
      data: {
        reason: "CUDA error: the provided PTX was compiled with an unsupported toolchain",
        stderr_tail: [
          "2026-11-16 08:20:31+0800: TensorRT backend thread 0: Found GPU NVIDIA GeForce RTX 3070 Laptop GPU memory 8589410304 compute capability major 8 minor 6",
          "2026-11-16 08:20:31+0800: TensorRT backend thread 0: Initializing (may take a long time)",
          "CUDA error: the provided PTX was compiled with an unsupported toolchain",
        ],
      },
    },
  };
  await start(
    page,
    [
      { command: "accelerationPlan", result: NVIDIA },
      { command: "enableAcceleration", result: SETTINGS, error: failed },
      { command: "accelerationPlan", result: NVIDIA },
    ],
    { ...SETTINGS, acceleration_hint_dismissed: true },
  );
  await page.getByTestId("menubar").locator('[data-menu-id="settings"]').click();
  await page.locator('.menu-row[data-menu-id="autoSetup"]').click();
  const dialog = page.getByTestId("benchmark-dialog");
  await dialog.getByTestId("tools-tab-acceleration").click();
  const panel = dialog.getByTestId("tools-acceleration");
  await expect(panel.getByTestId("accel-gpus")).toHaveText("NVIDIA GeForce RTX 3070 Laptop GPU（驱动 560.76）");
  await panel.getByTestId("accel-option-tensorrt").click();
  await panel.getByTestId("accel-enable").click();
  const outcome = panel.getByTestId("accel-outcome");
  await expect(outcome).toContainText("没有成功，已退回原来的引擎。");
  await expect(outcome).toContainText("unsupported toolchain");
  await outcome.locator("summary").click();
  await expect(outcome.locator("pre")).toContainText("Initializing (may take a long time)");
  await expect(panel.getByTestId("accel-current")).toHaveText("CPU");
  await expect(page.getByTestId("accel-hint")).toBeHidden();
  await expect(dialog).toHaveScreenshot("acceleration-failed.png");
  expect(await calls(page)).toEqual(["settings", "accelerationPlan", "enableAcceleration", "accelerationPlan"]);
});
