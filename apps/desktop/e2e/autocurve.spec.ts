import { expect, test, type Page } from "@playwright/test";
import type { WholeGameProgress } from "../src/api/backend";
import { documentChanged, midGame } from "./fixtures";
import type { MockCommand, MockScript } from "./mock-backend";

const QUICK: WholeGameProgress = { completed: 54, total: 211, visits: 1, status: { type: "Running" }, source: "AutomaticQuick" };

async function start(page: Page, commands: MockCommand[]) {
  const script: MockScript = {
    initialEvents: [documentChanged(midGame(), 3), { type: "WholeGameProgress", data: QUICK }],
    commands: [
      {
        command: "settings",
        result: { version: 1, engine: { program: "/opt/katago", model: "/opt/model.bin.gz", config: "", extra_args: [] }, pondering: true, auto_quick_analyze_on_load: true },
      },
      ...commands,
    ],
  };
  await page.addInitScript((initial) => {
    window.__mockInitialScript = initial;
  }, script);
  await page.goto("/");
}

const calls = (page: Page) => page.evaluate(() => window.__mockBackend?.recordedCalls().map((call) => call.command) ?? []);

test("an opened game's graph pass shows quietly by the graph and stops there", async ({ page }) => {
  const cancelled: WholeGameProgress = { ...QUICK, status: { type: "Cancelled" } };
  await start(page, [{ command: "cancelWholeGameAnalysis", result: cancelled, events: [{ type: "WholeGameProgress", data: cancelled }] }]);
  await expect(page.getByTestId("curve-progress")).toContainText("Drawing 54/211");
  await expect(page.getByTestId("auto-analysis")).not.toContainText("54 / 211");
  await page.getByTestId("curve-progress-stop").click();
  await expect(page.getByTestId("curve-progress")).toHaveCount(0);
  await expect.poll(() => calls(page)).toEqual(["settings", "cancelWholeGameAnalysis"]);
});

test("the auto-analysis panel turns the pass off, and 整盘速览（闪电） runs one by hand", async ({ page }) => {
  const manual: WholeGameProgress = { completed: 0, total: 211, visits: 1, status: { type: "Running" }, source: "Manual" };
  await start(page, [
    { command: "setAutoQuickAnalyzeOnLoad", result: false },
    { command: "startWholeGameAnalysis", result: manual, events: [{ type: "WholeGameProgress", data: manual }] },
  ]);
  await page.getByTestId("auto-analysis").click();
  await page.getByTestId("auto-curve").click();
  await expect(page.getByTestId("auto-curve")).not.toBeChecked();
  await page.keyboard.press("Escape");

  await page.getByTestId("menubar").getByRole("menuitem", { name: "Analyze" }).click();
  await page.locator('.menu-row[data-menu-id="flashAnalyzeAllGame"]').click();
  await expect(page.getByTestId("auto-analysis")).toContainText("0 / 211");
  await expect
    .poll(() => page.evaluate(() => window.__mockBackend?.recordedCalls() ?? []))
    .toEqual([
      { command: "settings", args: [] },
      { command: "setAutoQuickAnalyzeOnLoad", args: [false] },
      { command: "startWholeGameAnalysis", args: [{ visits: 1, scope: { type: "Line" }, force: false }] },
    ]);
});
