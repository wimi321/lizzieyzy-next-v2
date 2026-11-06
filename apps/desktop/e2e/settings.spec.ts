import { expect, test } from "@playwright/test";
import { documentChanged, midGame } from "./fixtures";
import type { MockScript } from "./mock-backend";

const LIMITS = { limit_time: true, max_analyze_time_seconds: 600, limit_playout: false, limit_playouts: 100000 };

test("设置 → 综合设置 sets the per-move analysis limits", async ({ page }) => {
  const saved = { ...LIMITS, max_analyze_time_seconds: 300, limit_playout: true, limit_playouts: 50000 };
  const script: MockScript = {
    initialEvents: [documentChanged(midGame(), 3)],
    commands: [
      { command: "settings", result: { version: 1, engine: { program: "/opt/katago", model: "/opt/model.bin.gz", config: "", extra_args: [] }, pondering: true } },
      { command: "analysisLimits", result: LIMITS },
      { command: "setAnalysisLimits", result: saved },
    ],
  };
  await page.addInitScript((initial) => {
    window.__mockInitialScript = initial;
  }, script);
  await page.goto("/");
  await page.getByTestId("menubar").getByRole("menuitem", { name: "Settings" }).click();
  await page.locator('.menu-row[data-menu-id="comprehensiveSettings"]').click();
  const dialog = page.getByTestId("settings-dialog");
  await expect(page.getByTestId("settings-seconds")).toHaveValue("600");
  await expect(page.getByTestId("settings-playouts")).toBeDisabled();
  await expect(dialog).toHaveScreenshot("settings.png");

  for (const bad of ["0", "5000000000"]) {
    await page.getByTestId("settings-seconds").fill(bad);
    await expect(page.getByTestId("settings-invalid")).toBeVisible();
    await expect(page.getByTestId("settings-save")).toBeDisabled();
  }
  await page.getByTestId("settings-seconds").fill("300");
  await page.getByTestId("settings-limit-playout").check();
  await page.getByTestId("settings-playouts").fill("50000");
  await page.getByTestId("settings-save").click();
  await expect(dialog).toBeHidden();
  await expect
    .poll(() => page.evaluate(() => window.__mockBackend?.recordedCalls() ?? []))
    .toContainEqual({ command: "setAnalysisLimits", args: [saved] });
});

test("a move analysed to its limit gets Java's message box; Space resumes and OK closes it", async ({ page }) => {
  const script: MockScript = {
    initialEvents: [documentChanged(midGame(), 3), { type: "AnalysisStateChanged", data: { pondering: false, pause_reason: "TimeLimit" } }],
    commands: [
      { command: "settings", result: { version: 1, engine: { program: "/opt/katago", model: "/opt/model.bin.gz", config: "", extra_args: [] }, pondering: true } },
      { command: "togglePondering", result: { pondering: true, pause_reason: null } },
    ],
  };
  await page.addInitScript((initial) => {
    window.__mockInitialScript = initial;
  }, script);
  await page.goto("/");
  const dialog = page.getByTestId("limit-dialog");
  await expect(dialog).toContainText("Ponder Stopped");
  await expect(dialog).toHaveScreenshot("limit-dialog.png");

  // The box is not modal: Space still reaches the board and resumes the analysis.
  await page.keyboard.press("Space");
  await expect
    .poll(() => page.evaluate(() => window.__mockBackend?.recordedCalls() ?? []))
    .toContainEqual({ command: "togglePondering", args: [] });
  await page.getByTestId("limit-ok").click();
  await expect(dialog).toBeHidden();
});
