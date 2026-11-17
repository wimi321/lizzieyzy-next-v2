import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";
import type { CaptureCells, CaptureRecognition, WireAppEvent } from "../src/api/backend";
import { documentChanged, midGame } from "./fixtures";
import type { MockCommand, MockScript } from "./mock-backend";

// The capture window is tested against a stand-in "screenshot": v2's own board baseline that
// fixtures/board-capture/wood-top-left was cropped from, with that fixture's points and cells.
const SHOT = readFileSync(fileURLToPath(new URL("./board.spec.ts-snapshots/mid-game-chromium-linux.png", import.meta.url))).toString("base64");
const ENGINE = { program: "/opt/homebrew/bin/katago", model: "/opt/katago/kata1-b18c384nbt.bin.gz", config: "", extra_args: [] };
const CELLS: CaptureCells = {
  rows: 6,
  columns: 6,
  cells: ["......", "......", "....W.", "...B.B", "......", "..B..."].flatMap((row) => [...row].map((c) => (c === "B" ? "Black" : c === "W" ? "White" : "Empty"))),
};
const RECOGNITION: CaptureRecognition = { grid: { origin: { x: 49.5, y: 49.5 }, gap_x: 36, gap_y: 36, columns: 6, rows: 6, corner: "TopLeft" }, cells: CELLS };

const calls = (page: Page) => page.evaluate(() => window.__mockBackend?.recordedCalls() ?? []);
const push = (page: Page, events: WireAppEvent[]) => page.evaluate((list) => window.__mockBackend?.pushEvents(list), events);

test.describe("the main window", () => {
  test.use({ viewport: { width: 1280, height: 820 }, deviceScaleFactor: 1, locale: "zh-CN" });

  async function start(page: Page, commands: MockCommand[]) {
    const script: MockScript = { initialEvents: [documentChanged(midGame(), 3)], commands: [{ command: "settings", result: { version: 1, engine: ENGINE, pondering: false, fox_history: [] } }, ...commands] };
    await page.addInitScript((initial) => {
      window.__mockInitialScript = initial;
    }, script);
    await page.goto("/");
    await expect(page.getByTestId("board")).toHaveAttribute("data-ready", "true");
  }

  test("分析 → 抓取死活题 shows Java's three steps; 参数设置 checks its ranges; 开始抓取 starts; a finished capture opens 死活题", async ({ page }) => {
    await start(page, [
      { command: "setCaptureSettings", result: { black_offset: 120, black_percent: 30, white_offset: 120, white_percent: 15, gray_offset: 80 } },
      { command: "startScreenCapture", result: { session: { id: 7, width: 748, height: 748, scale_factor: 1 }, logical_width: 748, logical_height: 748 } },
    ]);
    await page.getByTestId("menubar").getByRole("menuitem", { name: "分析" }).click();
    await page.locator('.menu-row[data-menu-id="captureTsumeGo"]').click();
    const dialog = page.getByTestId("capture-dialog");
    await expect(dialog.locator("li")).toHaveCount(3);
    await expect(dialog.locator("img").nth(2)).toHaveJSProperty("complete", true);
    await expect(dialog).toHaveScreenshot("capture-dialog.png");

    await dialog.getByTestId("capture-settings").click();
    const settings = page.getByTestId("capture-settings-dialog");
    await settings.getByTestId("capture-field-black_percent").fill("300");
    await settings.getByTestId("capture-settings-save").click();
    await expect(settings.getByTestId("capture-settings-error")).toHaveText("黑最低占比要是 1 到 100 之间的整数。");
    await settings.getByTestId("capture-field-black_percent").fill("30");
    await settings.getByTestId("capture-settings-save").click();
    await page.getByTestId("capture-start").click();
    await expect(dialog).toHaveCount(0);
    await expect.poll(async () => (await calls(page)).map((call) => call.command)).toEqual(["settings", "setCaptureSettings", "startScreenCapture"]);
    expect((await calls(page))[1]!.args).toEqual([{ black_offset: 120, white_offset: 120, black_percent: 30, white_percent: 15, gray_offset: 80 }]);
    expect(await page.evaluate(() => localStorage.getItem("lizzieyzy-capture-board-size"))).toBe("19");

    await push(page, [{ type: "CaptureEnded", data: { outcome: { type: "Finished" } } }]);
    await expect(page.getByTestId("tsumego-dialog")).toBeVisible();
  });

  test("macOS without 屏幕录制 explains where to allow it", async ({ page }) => {
    await start(page, [
      { command: "capturePermission", result: "Missing" },
      { command: "requestCapturePermission", result: "Missing" },
      { command: "openCapturePermissionSettings", result: undefined },
    ]);
    await page.keyboard.press("Shift+T");
    const explanation = page.getByTestId("capture-permission");
    await expect(explanation).toContainText("系统设置 → 隐私与安全性 → 屏幕录制");
    await explanation.getByTestId("capture-open-settings").click();
    await expect.poll(async () => (await calls(page)).map((call) => call.command)).toEqual(["settings", "capturePermission", "requestCapturePermission", "openCapturePermissionSettings"]);
  });
});

test.describe("the capture window", () => {
  // The screenshot's own size: one CSS pixel per screenshot pixel.
  test.use({ viewport: { width: 748, height: 748 }, deviceScaleFactor: 1, locale: "zh-CN" });

  test("three clicks with Java's preview, a correction, a board size, then the new game", async ({ page }) => {
    const script: MockScript = {
      commands: [
        { command: "captureImage", result: new ArrayBuffer(0) },
        // The pointer passing the third point asks for a preview; the click asks again.
        { command: "recognizeCapture", result: RECOGNITION },
        { command: "recognizeCapture", result: RECOGNITION },
        { command: "finishCapture", result: 5 },
      ],
    };
    await page.addInitScript(
      ({ initial, shot }) => {
        const bytes = Uint8Array.from(atob(shot), (char) => char.charCodeAt(0));
        initial.commands![0]!.result = bytes.buffer;
        window.__mockInitialScript = initial;
        localStorage.setItem("lizzieyzy-capture-board-size", "19");
      },
      { initial: script, shot: SHOT },
    );
    await page.goto("/?window=capture&session=7");
    const shot = page.locator(".capture-image");
    await expect(shot).toHaveJSProperty("naturalWidth", 748);
    await expect(page.getByTestId("capture-hint")).toHaveText("步骤 1：左键点击题目角上的交叉点（棋盘的角）");

    await page.mouse.click(49.5, 49.5);
    // The pointer is near the top: the bar keeps out of its way at the bottom.
    await expect(page.getByTestId("capture-bar")).toHaveClass(/low/);
    await page.mouse.move(85.5, 85.5);
    await expect(page.locator(".capture-preview line")).toHaveCount(10);
    await page.mouse.click(85.5, 85.5);
    await expect(page.getByTestId("capture-page")).toHaveAttribute("data-stage", "2");
    await page.mouse.click(229.5, 229.5);
    await expect(page.getByTestId("capture-page")).toHaveAttribute("data-stage", "confirm");
    await expect(page.getByTestId("capture-size")).toHaveText("6 × 6 · 左上角");
    await expect(page.locator(".capture-dot")).toHaveCount(4);

    // Correct the empty point at row 0, column 0: empty → black.
    await page.mouse.click(49.5, 49.5);
    await expect(page.locator('.capture-dot[data-cell="Black"]')).toHaveCount(4);
    await expect(page.getByTestId("capture-board-19")).toHaveAttribute("aria-checked", "true");
    // While correcting, the bar keeps to the half of the screen away from the problem.
    await page.mouse.move(600, 600);
    await expect(page.getByTestId("capture-bar")).toHaveClass(/low/);
    await expect(page).toHaveScreenshot("capture-confirm.png");
    await page.getByTestId("capture-board-9").click();
    await page.getByTestId("capture-finish").click();
    await expect.poll(async () => (await calls(page)).map((call) => call.command)).toEqual(["captureImage", "recognizeCapture", "recognizeCapture", "finishCapture"]);
    const recorded = await calls(page);
    // Mouse events report whole CSS pixels; at one CSS pixel per screenshot pixel those are the points.
    expect(recorded[2]!.args).toEqual([7, [{ x: 49, y: 49 }, { x: 85, y: 85 }, { x: 229, y: 229 }]]);
    const [, result] = recorded[3]!.args as [number, { corner: string; board_size: number; cells: CaptureCells }];
    expect(result.corner).toBe("TopLeft");
    expect(result.board_size).toBe(9);
    expect(result.cells.cells[0]).toBe("Black");
    expect(result.cells.cells.filter((cell) => cell !== "Empty")).toHaveLength(5);
  });
});
