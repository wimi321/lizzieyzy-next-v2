import { expect, test, type Page } from "@playwright/test";
import type { EngineDiscovery } from "../src/api/backend";
import { documentChanged, emptyNineteen } from "./fixtures";
import type { MockCommand, MockScript } from "./mock-backend";

const KATAGO = "/opt/homebrew/bin/katago";
const KATA1 = "/opt/homebrew/share/katago/kata1-b18c384nbt-s9996604416-d4316597426.bin.gz";
const G170 = "/opt/homebrew/share/katago/g170e-b20c256x2-s5303129600-d1228401921.bin.gz";
const FOUND: EngineDiscovery = { programs: [KATAGO], models: [KATA1, G170], configs: [] };
const B11 = "/Applications/LizzieYzy Next.app/Contents/app/weights/default.bin.gz";
const B11_PROFILE = { program: KATAGO, model: B11, config: "", extra_args: [], canonical_model_name: "kata1-tf3-b11c768-s12002M-d6304M.bin.gz" };
const NOTHING_STARTED: MockCommand = {
  command: "autoConfigureEngine",
  result: B11_PROFILE,
  error: {
    type: "AutoSetup",
    data: { type: "AllFailed", data: { attempts: [{ program: KATAGO, model: KATA1, reason: "engine exited with status 1" }], reason: "every pair failed" } },
  },
};

/**
 * Scripts the start-up commands too: the app asks for settings as soon as it mounts and,
 * with no engine saved, tries automatic setup. Setup opens only when that fails.
 */
async function firstRun(page: Page, commands: MockCommand[], auto: MockCommand = NOTHING_STARTED) {
  const opens = auto.error !== undefined;
  const script: MockScript = {
    initialEvents: [documentChanged(emptyNineteen(), 1)],
    commands: [
      { command: "settings", result: { version: 1, engine: null, pondering: true } },
      auto,
      ...(opens ? [{ command: "detectEngines", result: FOUND } as const] : []),
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

test("first run sets the engine up by itself, B11 named from the catalog, without opening setup", async ({ page }) => {
  await firstRun(page, [], { command: "autoConfigureEngine", result: B11_PROFILE });
  await expect(page.getByTestId("menubar").getByRole("menuitem").last()).toHaveText("kata1-tf3-b11c768");
  await expect(page.getByTestId("engine-setup")).toHaveCount(0);
  await expect.poll(async () => (await calls(page)).map((call) => call.command)).toEqual(["settings", "autoConfigureEngine"]);
});

test("when automatic setup fails, setup opens, says why and preselects the recommended files", async ({ page }) => {
  const dialog = await firstRun(page, []);
  await expect(dialog).toBeVisible();
  await expect(page.getByTestId("setup-notice")).toContainText("None of the 1 program and model pairs found would start");
  await expect(page.getByTestId("setup-notice")).toContainText("katago + kata1-b18c384nbt-s9996604416-d4316597426.bin.gz: engine exited with status 1");
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

test("reopening working settings offers Apply only after a change", async ({ page }) => {
  const saved = { program: KATAGO, model: KATA1, config: "/Users/me/Library/Application Support/LizzieYzy Next 2/analysis.cfg", extra_args: [] };
  const script: MockScript = {
    initialEvents: [
      documentChanged(emptyNineteen(), 1),
      { type: "EngineStatusChanged", data: { type: "Ready", data: { version: "KataGo 1.16.5" } } },
    ],
    commands: [
      { command: "settings", result: { version: 1, engine: saved, pondering: true } },
      { command: "detectEngines", result: FOUND },
    ],
  };
  await page.addInitScript((initial) => {
    window.__mockInitialScript = initial;
  }, script);
  await page.goto("/");
  await expect(page.getByTestId("engine-status")).toHaveAttribute("data-state", "Ready");
  await expect(page.getByTestId("engine-setup")).toBeHidden();

  await page.getByTestId("open-setup").click();
  const apply = page.getByTestId("setup-start");
  await expect(page.getByTestId("engine-setup").getByRole("heading")).toHaveText("Engine settings");
  await expect(page.getByTestId("setup-config")).toHaveValue(saved.config);
  await expect(apply).toHaveText("Apply");
  await expect(apply).toBeDisabled();
  await page.getByTestId("setup-model").selectOption(G170);
  await expect(apply).toBeEnabled();
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
