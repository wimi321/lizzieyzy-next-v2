import { expect, test, type Page } from "@playwright/test";
import { documentChanged, midGame } from "./fixtures";
import { benchmarkDemoScript, type MockScript } from "./mock-backend";

async function start(page: Page) {
  const demo = benchmarkDemoScript();
  const script: MockScript = {
    initialEvents: [documentChanged(midGame(), 3), ...(demo.initialEvents ?? [])],
    commands: [
      {
        command: "settings",
        result: { version: 1, engine: { program: "/opt/katago", model: "/opt/default.bin.gz", config: "", extra_args: [], canonical_model_name: "kata1-tf3-b11c768-s12002M-d6304M.bin.gz" }, pondering: true },
      },
      ...(demo.commands ?? []),
    ],
  };
  await page.addInitScript((initial) => {
    window.__mockInitialScript = initial;
  }, script);
  await page.goto("/");
}

test("设置 → 引擎一键设置 tunes KataGo when asked, shows what it measured and can undo it", async ({ page }) => {
  await start(page);
  await page.getByTestId("menubar").getByRole("menuitem", { name: "Settings" }).click();
  await page.locator('.menu-row[data-menu-id="autoSetup"]').click();
  const dialog = page.getByTestId("benchmark-dialog");
  // Java's 一键设置 opens on its overview; 性能优化 is the benchmark.
  await expect(dialog.getByTestId("tools-overview")).toBeVisible();
  await dialog.getByTestId("tools-tab-benchmark").click();
  await expect(dialog.getByTestId("benchmark-state")).toHaveText("Not tuned yet");

  await dialog.getByTestId("benchmark-start").click();
  await expect(dialog.getByTestId("benchmark-state")).toHaveText("Tuned");
  await expect(dialog.getByTestId("benchmark-search-speed")).toHaveText("2449/s");
  await expect(dialog.getByTestId("benchmark-facts")).toContainText("Batch size2");
  await expect(dialog).toHaveScreenshot("benchmark-done.png");

  await dialog.getByTestId("benchmark-reset").click();
  await expect(dialog.getByTestId("benchmark-state")).toHaveText("Not tuned yet");
  await expect
    .poll(() => page.evaluate(() => window.__mockBackend?.recordedCalls().map((call) => call.command) ?? []))
    .toEqual(["settings", "startBenchmark", "resetBenchmark"]);
});
