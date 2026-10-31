import { expect, test, type Page } from "@playwright/test";
import type { TsumegoOptions, TsumegoStatus } from "../src/api/backend";
import { documentChanged, midGame } from "./fixtures";
import type { MockCommand, MockScript } from "./mock-backend";

const SAVED: TsumegoOptions = { to_play: "Inherit", attacker: "Auto", ko_threat: "Both", wall_distance: 1 };
const FRAMED: TsumegoStatus = {
  revision: 3,
  active: true,
  source_node: 3,
  options: { ...SAVED, attacker: "White" },
  attacker: "White",
  ko_warnings: ["Black"],
};

async function start(page: Page, commands: MockCommand[]) {
  const script: MockScript = {
    initialEvents: [documentChanged(midGame(), 3)],
    commands: [
      {
        command: "settings",
        result: { version: 1, engine: { program: "/opt/katago", model: "/opt/model.bin.gz", config: "", extra_args: [] }, pondering: true, tsumego: SAVED },
      },
      ...commands,
    ],
  };
  await page.addInitScript((initial) => {
    window.__mockInitialScript = initial;
  }, script);
  await page.goto("/");
}

test("分析 → 死活题 frames the position, and the toolbar chip leads back", async ({ page }) => {
  await start(page, [
    { command: "generateTsumego", result: 3 },
    { command: "tsumegoStatus", result: FRAMED },
    { command: "tsumegoStatus", result: FRAMED },
    { command: "exitTsumego", result: 3 },
  ]);
  await page.getByTestId("menubar").getByRole("menuitem", { name: "Analyze" }).click();
  await page.locator('.menu-row[data-menu-id="tsumeGoMenu"]').click();
  const dialog = page.getByTestId("tsumego-dialog");
  await expect(dialog).toBeVisible();
  await expect(dialog).toHaveScreenshot("tsumego-dialog.png");

  await page.getByTestId("tsumego-attacker-white").click();
  await page.getByTestId("tsumego-confirm").click();
  await expect(dialog).toBeHidden();
  const chip = page.getByTestId("tsumego-chip");
  await expect(chip).toContainText("White attacks");
  await expect(chip).toHaveAttribute("title", "No room for Black's ko threats");
  await page.getByTestId("tsumego-exit").click();
  await expect(chip).toHaveCount(0);
  await expect
    .poll(() => page.evaluate(() => window.__mockBackend?.recordedCalls().map((call) => call.command) ?? []))
    .toEqual(["settings", "generateTsumego", "tsumegoStatus", "tsumegoStatus", "exitTsumego"]);
});
