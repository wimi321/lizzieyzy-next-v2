import { expect, test, type Page } from "@playwright/test";
import type { EngineDiscovery } from "../src/api/backend";
import { documentChanged, emptyNineteen } from "./fixtures";
import type { MockCommand, MockScript } from "./mock-backend";

const KATAGO = "/opt/homebrew/bin/katago";
const KATA1 = "/opt/homebrew/share/katago/kata1-b18c384nbt-s9996604416-d4316597426.bin.gz";
const G170 = "/opt/homebrew/share/katago/g170e-b20c256x2-s5303129600-d1228401921.bin.gz";
const FOUND: EngineDiscovery = { programs: [KATAGO], models: [KATA1, G170], configs: [] };

/** Scripts the start-up commands too: the app asks for settings as soon as it mounts. */
async function firstRun(page: Page, commands: MockCommand[]) {
  const script: MockScript = {
    initialEvents: [documentChanged(emptyNineteen(), 1)],
    commands: [
      { command: "settings", result: { version: 1, engine: null, pondering: true } },
      { command: "detectEngines", result: FOUND },
      ...commands,
    ],
  };
  await page.addInitScript((initial) => {
    window.__mockInitialScript = initial;
  }, script);
  await page.goto("/");
  return page.getByTestId("engine-setup");
}

async function calls(page: Page) {
  return page.evaluate(() => window.__mockBackend?.recordedCalls() ?? []);
}

test("first run opens setup with the recommended engine files selected", async ({ page }) => {
  const dialog = await firstRun(page, []);
  await expect(dialog).toBeVisible();
  await expect(page.getByTestId("setup-program")).toHaveValue(KATAGO);
  await expect(page.getByTestId("setup-model")).toHaveValue(KATA1);
  await expect(page.getByTestId("setup-model").locator("option", { hasText: "recommended" })).toHaveText(/kata1-b18c384nbt/);
  await expect(page.getByTestId("setup-config")).toHaveValue("");
  await page.getByTestId("setup-model").selectOption(G170);
  await expect(page.getByTestId("setup-model")).toHaveValue(G170);
});

test("browsing replaces a detected file with the chosen one", async ({ page }) => {
  await firstRun(page, [{ command: "pickEngineFile", result: "/custom/bin/katago-opencl" }]);
  await page.getByTestId("setup-program-browse").click();
  await expect(page.getByTestId("setup-program")).toHaveValue("/custom/bin/katago-opencl");
  await expect.poll(async () => (await calls(page)).at(-1)).toEqual({ command: "pickEngineFile", args: ["program"] });
});

test("a successful start saves through try-then-save and closes setup", async ({ page }) => {
  const dialog = await firstRun(page, [{ command: "tryConfigureEngine", result: undefined }]);
  await page.getByTestId("setup-start").click();
  await expect(dialog).toBeHidden();
  await expect.poll(async () => (await calls(page)).at(-1)).toEqual({
    command: "tryConfigureEngine",
    args: [{ program: KATAGO, model: KATA1, config: null, extra_args: [] }],
  });
});

test("a failed start explains why and keeps the dialog open", async ({ page }) => {
  const dialog = await firstRun(page, [
    { command: "tryConfigureEngine", result: undefined, error: "engine setup failed: model file is not readable" },
  ]);
  await page.getByTestId("setup-start").click();
  await expect(page.getByTestId("setup-error")).toContainText("model file is not readable");
  await expect(dialog).toBeVisible();
  await expect(page.getByTestId("setup-start")).toBeEnabled();
});

test("setup can be reopened from the header and closed again", async ({ page }) => {
  await page.goto("/");
  await page.evaluate(
    (commands) => window.__mockBackend?.setScript({ initialEvents: [], commands }),
    [{ command: "detectEngines", result: FOUND }] satisfies MockCommand[],
  );
  await page.getByTestId("open-setup").click();
  await expect(page.getByTestId("engine-setup")).toBeVisible();
  await page.getByTestId("setup-close").click();
  await expect(page.getByTestId("engine-setup")).toBeHidden();
});
