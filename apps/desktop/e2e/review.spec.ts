import { expect, test, type Page } from "@playwright/test";
import type { WireAppEvent, WholeGameProgress } from "../src/api/backend";
import { analysedGame, documentChanged } from "./fixtures";
import type { MockCommand } from "./mock-backend";

test.use({ viewport: { width: 1280, height: 820 } });

const running = (completed: number): WholeGameProgress => ({ completed, total: 121, visits: 3000, status: { type: "Running" } });

async function start(page: Page, events: WireAppEvent[], commands: MockCommand[] = []) {
  await page.goto("/");
  await page.evaluate(
    ([initialEvents, scripted]) => window.__mockBackend?.setScript({ initialEvents, commands: scripted }),
    [events, commands] as const,
  );
}

async function calls(page: Page) {
  return page.evaluate(() => window.__mockBackend?.recordedCalls() ?? []);
}

test("auto analysis starts with the chosen preset, shows progress and pauses", async ({ page }) => {
  await start(page, [documentChanged(analysedGame(), 7)], [
    { command: "startWholeGameAnalysis", result: running(0), events: [{ type: "WholeGameProgress", data: running(40) }] },
    { command: "pauseWholeGameAnalysis", result: { ...running(40), status: { type: "Paused" } } },
  ]);
  await page.getByTestId("auto-analysis").click();
  await expect(page.getByTestId("auto-analysis-panel")).toContainText("121");
  await page.getByTestId("auto-analysis-deep").click();
  await page.getByTestId("auto-analysis-start").click();

  const chip = page.getByTestId("auto-analysis");
  await expect(chip).toContainText("40 / 121");
  await chip.click();
  await page.getByTestId("auto-analysis-pause").click();
  await expect(chip).toHaveAttribute("data-state", "Paused");
  await expect.poll(() => calls(page)).toEqual([
    { command: "startWholeGameAnalysis", args: [{ visits: 3000 }] },
    { command: "pauseWholeGameAnalysis", args: [] },
  ]);
});
