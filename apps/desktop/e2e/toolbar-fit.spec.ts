import { expect, test, type Page } from "@playwright/test";
import { documentChanged, midGame } from "./fixtures";
import type { MockCommand, MockScript } from "./mock-backend";

test.use({ viewport: { width: 1280, height: 820 }, deviceScaleFactor: 1, locale: "zh-CN" });

const ENGINE = { program: "/opt/homebrew/bin/katago", model: "/opt/katago/kata1-b18c384nbt.bin.gz", config: "", extra_args: [] };

async function start(page: Page, commands: MockCommand[] = []) {
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

/** Where each group's shown buttons begin and end, left to right. */
function groups(page: Page) {
  return page.getByTestId("bottom-toolbar").evaluate((bar) =>
    Array.from(bar.children).map((group) => {
      const boxes = Array.from(group.children)
        .filter((child) => getComputedStyle(child).visibility !== "hidden")
        .map((child) => child.getBoundingClientRect());
      return { start: Math.min(...boxes.map((box) => box.left)), end: Math.max(...boxes.map((box) => box.right)) };
    }),
  );
}

test("远程算力 keeps its place as the window narrows; buttons never overlap, and the rest move into 更多", async ({ page }) => {
  await start(page);
  for (const width of [1280, 1100, 900]) {
    await page.setViewportSize({ width, height: 820 });
    await expect(page.getByTestId("tool-remote")).toBeVisible();
    await expect(page.getByTestId("toggle-ownership")).toBeVisible();
    await expect
      .poll(async () => {
        const [left, center, right] = await groups(page);
        return left !== undefined && center !== undefined && right !== undefined && left.end <= center.start && center.end <= right.start;
      })
      .toBe(true);
  }
  await expect(page.getByTestId("bottom-delete")).toBeHidden();
  await page.getByTestId("tool-more").click();
  const more = page.getByTestId("tool-more-menu");
  await expect(more.getByRole("menuitem")).toHaveText(["更新官方权重", "自动播放", "设为主分支", "手数", "清空棋盘", "删除", "试下", "终局点目", "纯网络"]);
  await expect(page.getByTestId("bottom-toolbar")).toHaveScreenshot("bottom-toolbar-narrow.png");
});

test("in a very narrow window 远程算力 opens from 更多", async ({ page }) => {
  await start(page, [{ command: "zhiziStatus", result: { logged_in: false, store_available: true } }]);
  await page.setViewportSize({ width: 700, height: 820 });
  await expect(page.getByTestId("tool-remote")).toBeHidden();
  await page.getByTestId("tool-more").click();
  await page.getByTestId("more-tool-remote").click();
  await expect(page.getByTestId("tool-more-menu")).toBeHidden();
  await expect(page.getByTestId("remote-dialog")).toBeVisible();
});
