import { expect, test, type Page } from "@playwright/test";
import type { WebBoardStatus, WireAppEvent } from "../src/api/backend";
import { documentChanged, midGame } from "./fixtures";
import type { MockCommand, MockScript } from "./mock-backend";

test.use({ viewport: { width: 1280, height: 820 }, deviceScaleFactor: 1, locale: "zh-CN" });

const ENGINE = { program: "/opt/homebrew/bin/katago", model: "/opt/katago/kata1-b18c384nbt.bin.gz", config: "", extra_args: [] };
const ROOT = "http://192.168.1.5:9998/Zm9vYmFyYmF6cXV4MTIz/";
const RUNNING: WebBoardStatus = { running: true, port: 9998, url: ROOT, addresses: [ROOT, "http://10.0.0.7:9998/Zm9vYmFyYmF6cXV4MTIz/"], viewers: 2, trial: null };
const changed = (status: WebBoardStatus): WireAppEvent => ({ type: "WebBoardChanged", data: status });

async function start(page: Page, commands: MockCommand[]) {
  const script: MockScript = {
    initialEvents: [documentChanged(midGame(), 3)],
    commands: [{ command: "settings", result: { version: 1, engine: ENGINE, pondering: false, fox_history: [] } }, ...commands],
  };
  await page.addInitScript((initial) => {
    window.__mockInitialScript = initial;
  }, script);
  await page.goto("/");
  await expect(page.getByTestId("board")).toHaveAttribute("data-ready", "true");
}

const row = (page: Page, id: string) => page.locator(`.menu-row[data-menu-id="${id}"]`);
const push = (page: Page, events: WireAppEvent[]) => page.evaluate((list) => window.__mockBackend?.pushEvents(list), events);
const calls = (page: Page) => page.evaluate(() => (window.__mockBackend?.recordedCalls() ?? []).filter((call) => call.command !== "webBoardStatus"));

async function webMenu(page: Page) {
  await page.getByTestId("menubar").getByRole("menuitem", { name: "同步" }).click();
  await row(page, "webBoardMenu").hover();
}

test("同步 → Web 旁观 starts the viewer: the strip's chip, its panel with every address, and stopping", async ({ page }) => {
  await start(page, [
    { command: "startWebBoard", result: RUNNING },
    { command: "setWebBoardOptions", result: undefined },
    { command: "stopWebBoard", result: undefined },
  ]);
  await webMenu(page);
  await expect(row(page, "webBoardToggle")).toHaveText("启动 Web 旁观");
  await expect(row(page, "webBoardCopyUrl")).toHaveAttribute("aria-disabled", "true");
  await expect(row(page, "webBoardAllowTrial")).toHaveText("允许旁观者试下");
  await expect(row(page, "webBoardAllowTrial")).toHaveAttribute("aria-checked", "true");
  await expect(row(page, "webBoardExitTrial")).toHaveAttribute("aria-disabled", "true");
  await row(page, "webBoardToggle").click();

  await expect(page.getByTestId("web-board-chip-open")).toHaveText("网页旁观 · 2 人");
  const panel = page.getByTestId("web-board-panel");
  await expect(panel.getByTestId("web-board-address")).toHaveCount(2);
  await expect(panel.getByTestId("web-board-address").first()).toHaveValue(ROOT);
  await expect(panel.getByTestId("web-board-viewers")).toHaveText("正在看：2 人");
  await expect(panel.getByTestId("web-board-trial")).toHaveText("没有人在试下");
  await expect(page).toHaveScreenshot("web-board-host.png");

  await panel.getByTestId("web-board-allow").uncheck();
  await expect(panel.getByTestId("web-board-allow")).not.toBeChecked();
  await panel.getByTestId("web-board-panel-close").click();
  await expect(panel).toBeHidden();

  await webMenu(page);
  await expect(row(page, "webBoardAllowTrial")).toHaveAttribute("aria-checked", "false");
  await expect(row(page, "webBoardCopyUrl")).not.toHaveAttribute("aria-disabled", "true");
  await expect(row(page, "webBoardToggle")).toHaveText("停止 Web 旁观");
  await row(page, "webBoardToggle").click();
  await expect(page.getByTestId("web-board-chip")).toHaveCount(0);
  expect(await calls(page)).toEqual([
    { command: "settings", args: [] },
    { command: "startWebBoard", args: [] },
    { command: "setWebBoardOptions", args: [{ allow_trial: false }] },
    { command: "stopWebBoard", args: [] },
  ]);
});

test("a viewer's trial: the chip says so, and the host ends it from the chip or the menu", async ({ page }) => {
  const trial = { ...RUNNING, trial: { owner: "viewer-1", since: Date.now() - 3 * 60_000 } };
  await start(page, [
    { command: "forceEndWebTrial", result: undefined },
    { command: "forceEndWebTrial", result: undefined, events: [changed(RUNNING)] },
  ]);
  await push(page, [changed(trial)]);
  const chip = page.getByTestId("web-board-chip");
  await expect(chip).toHaveAttribute("data-trial", "true");
  await expect(chip.getByTestId("web-board-chip-open")).toHaveText("网页旁观者正在试下");
  await chip.getByTestId("web-board-chip-open").click();
  await expect(page.getByTestId("web-board-trial")).toContainText("有旁观者正在试下（3 分钟）");

  await chip.getByTestId("web-board-chip-end").click();
  await webMenu(page);
  await expect(row(page, "webBoardExitTrial")).not.toHaveAttribute("aria-disabled", "true");
  await row(page, "webBoardExitTrial").click();
  await expect(chip).not.toHaveAttribute("data-trial", "true");
  await expect(chip.getByTestId("web-board-chip-open")).toHaveText("网页旁观 · 2 人");
  await expect(page.getByTestId("web-board-trial")).toHaveText("没有人在试下");
  expect((await calls(page)).map((call) => call.command)).toEqual(["settings", "forceEndWebTrial", "forceEndWebTrial"]);
});

test("a viewer that cannot start says why", async ({ page }) => {
  await start(page, [{ command: "startWebBoard", result: RUNNING, error: { type: "WebBoard", data: { type: "Bind", data: { reason: "端口 9998–10007 都被占用" } } } }]);
  await webMenu(page);
  await row(page, "webBoardToggle").click();
  await expect(page.getByTestId("web-board-toast")).toHaveText("网页旁观没能开启：端口 9998–10007 都被占用");
  await expect(page.getByTestId("web-board-chip")).toHaveCount(0);
});
