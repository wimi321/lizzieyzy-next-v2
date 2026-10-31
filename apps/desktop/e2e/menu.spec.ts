import { expect, test, type Page } from "@playwright/test";
import { documentChanged, emptyNineteen } from "./fixtures";
import type { MockCommand, MockScript } from "./mock-backend";

test.use({ viewport: { width: 1280, height: 820 }, locale: "zh-CN" });

const ENGINE = {
  program: "/opt/homebrew/bin/katago",
  model: "/opt/katago/kata1-b18c384nbt-s9996604416-d4316597426.bin.gz",
  config: "",
  extra_args: [],
};

/** The app reads its settings as soon as it mounts, so they lead the script. */
async function start(page: Page, commands: MockCommand[] = []) {
  const script: MockScript = {
    initialEvents: [documentChanged(emptyNineteen(), 1)],
    commands: [{ command: "settings", result: { version: 1, engine: ENGINE, pondering: true, fox_history: [] } }, ...commands],
  };
  await page.addInitScript((initial) => {
    window.__mockInitialScript = initial;
  }, script);
  await page.goto("/");
  await expect(page.getByTestId("board")).toBeVisible();
}

const row = (page: Page, id: string) => page.locator(`.menu-row[data-menu-id="${id}"]`);

test("LizzieYzy Next's menus sit above the toolbar, in Java's order", async ({ page }) => {
  await start(page);
  const bar = page.getByTestId("menubar");
  await expect(bar.getByRole("menuitem")).toHaveText([
    "文件",
    "显示",
    "棋局",
    "分析",
    "编辑",
    "同步",
    "帮助",
    "设置",
    "kata1-b18c384nbt",
  ]);

  await bar.getByRole("menuitem", { name: "文件" }).click();
  const popup = page.getByTestId("menu-popup");
  await expect(popup).toHaveCount(1);
  await expect(row(page, "open")).not.toHaveAttribute("aria-disabled");
  await expect(row(page, "newBoard")).toHaveAttribute("aria-disabled", "true");
  await expect(popup).toHaveScreenshot("file-menu.png");

  // While a menu is open, the strip switches on hover and submenus open on hover.
  await bar.getByRole("menuitem", { name: "显示" }).hover();
  await row(page, "panel").hover();
  await expect(popup).toHaveCount(2);
  await expect(row(page, "winrateGraph")).toHaveAttribute("aria-checked", "true");
  await page.keyboard.press("Escape");
  await expect(popup).toHaveCount(1);
  await page.keyboard.press("Escape");
  await expect(popup).toHaveCount(0);
});

test("menus and Java's keys reach the review tools", async ({ page }) => {
  await start(page, [{ command: "setPondering", result: undefined }]);
  const bar = page.getByTestId("menubar");

  await bar.getByRole("menuitem", { name: "同步" }).click();
  await row(page, "foxKifu").click();
  await expect(page.getByTestId("fox-dialog")).toBeVisible();
  await page.getByTestId("fox-close").click();
  await expect(page.getByTestId("fox-dialog")).toBeHidden();

  await bar.getByRole("menuitem", { name: "分析" }).click();
  await row(page, "autoAnalyze").click();
  await expect(page.getByTestId("auto-analysis-panel")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("auto-analysis-panel")).toBeHidden();

  // 空格 starts and stops analysis, as Java's 开始/停止 分析(空格).
  await page.keyboard.press("Space");
  await expect(page.getByTestId("toggle-pondering")).toHaveAttribute("data-state", "paused");
  const calls = await page.evaluate(() => window.__mockBackend?.recordedCalls() ?? []);
  expect(calls).toEqual([
    { command: "settings", args: [] },
    { command: "setPondering", args: [false] },
  ]);

  await bar.getByRole("menuitem", { name: "帮助" }).click();
  await row(page, "about").click();
  const about = page.getByTestId("about-dialog");
  await expect(about).toContainText("LizzieYzy Next 2");
  await expect(about).toHaveScreenshot("about.png");
  await page.keyboard.press("Escape");
  await expect(about).toBeHidden();
});
