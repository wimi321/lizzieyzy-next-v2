import { expect, test, type Page } from "@playwright/test";
import { analysedGame, analysisUpdated, documentChanged, midGameAnalysis } from "./fixtures";
import type { MockCommand, MockScript } from "./mock-backend";

// The window of the reference screenshots, in LizzieYzy Next's own language.
test.use({ viewport: { width: 1429, height: 941 }, locale: "zh-CN" });

const ENGINE = { program: "/opt/katago", model: "/opt/kata1-tf3-b11c768-s12002M-d6304M.bin.gz", config: "", extra_args: [] };

async function start(page: Page, commands: MockCommand[] = []) {
  const script: MockScript = {
    initialEvents: [documentChanged(analysedGame(), 7), analysisUpdated(midGameAnalysis(7, 20))],
    commands: [{ command: "settings", result: { version: 1, engine: ENGINE, pondering: true } }, ...commands],
  };
  await page.addInitScript((initial) => {
    window.__mockInitialScript = initial;
  }, script);
  await page.goto("/");
  await expect(page.getByTestId("board")).toBeVisible();
}

/** The buttons of a toolbar that are usable (or greyed), by the name a user sees or hears. */
function buttons(page: Page, toolbar: string, enabled: boolean) {
  return page
    .getByTestId(toolbar)
    .getByRole("button")
    .evaluateAll(
      (found, wanted) =>
        found
          .filter((button) => (button as HTMLButtonElement).disabled !== wanted)
          .map((button) => button.getAttribute("aria-label") ?? button.textContent?.trim() ?? ""),
      enabled,
    );
}

async function calls(page: Page) {
  return page.evaluate(() => window.__mockBackend?.recordedCalls() ?? []);
}

test("the toolbars hold Java's buttons in Java's order; what the new app lacks stays greyed in place", async ({ page }) => {
  await start(page);
  expect(await buttons(page, "top-toolbar", true)).toEqual(["打开棋谱(O)", "保存棋谱(Ctrl+S)", "野狐棋谱", "棋力评估", "一键设置", "闪电分析", "停一手(P)", "死活"]);
  expect(await buttons(page, "top-toolbar", false)).toEqual([
    "新建空白棋盘",
    "腾讯棋谱",
    "搜索功能",
    "超级鹰眼(Y)",
    "落子评价标记(Alt+M)",
    "交换落子顺序",
    "设为主分支(L)",
    "返回主分支(B)",
    "独立棋局控制面板",
    "标记工具",
    "添加黑子(右键添加白子)",
    "添加白子(右键添加黑子)",
    "交替落子",
    "设置强制计算区域(Alt+鼠标圈选)",
    "允许选点选项",
    "设置强制不计算区域",
    "排除选点选项",
    "清除强制区域(Alt+点击)",
    "新对局",
    "暂停",
    "规则",
    "参数",
    "棋盘",
    "存档",
  ]);
  expect(await buttons(page, "bottom-toolbar", true)).toEqual(["暂停分析", "自动分析", "Kata形势", "|<", "<<", "<", ">", ">>", ">|", "坐标", "更多"]);
  expect(await buttons(page, "bottom-toolbar", false)).toEqual(["远程算力", "更新官方权重", "跳转", "自动播放", "设为主分支", "手数", "清空棋盘", "删除"]);
  await expect(page.getByTestId("tool-komi")).toHaveValue("7.5");

  // The menu strip: the engine as Java captions it, then AI 陪练 and AI 解说, not built yet.
  const strip = page.locator(".menubar-row");
  await expect(page.getByTestId("menubar").getByRole("menuitem").last()).toHaveText("[1]: kata1-tf3-b11c768");
  await expect(strip.getByRole("button", { name: "AI 陪练" })).toBeDisabled();
  await expect(strip.getByRole("button", { name: "AI 解说" })).toBeDisabled();

  await expect(page.getByTestId("top-toolbar")).toHaveScreenshot("top-toolbar.png");
  await expect(page.getByTestId("bottom-toolbar")).toHaveScreenshot("bottom-toolbar.png");
});

test("the working buttons do what Java's do", async ({ page }) => {
  await start(page, [
    { command: "startWholeGameAnalysis", result: { completed: 0, total: 121, visits: 1, status: { type: "Running" }, source: "Manual" } },
    { command: "play", result: 8 },
  ]);

  await page.getByTestId("tool-auto-setup").click();
  await expect(page.getByTestId("benchmark-dialog")).toBeVisible();
  await page.getByTestId("benchmark-close").click();
  await expect(page.getByTestId("benchmark-dialog")).toBeHidden();

  await page.getByTestId("tool-flash").click();
  await page.getByTestId("tool-pass").click();
  await expect.poll(() => calls(page)).toEqual([
    { command: "settings", args: [] },
    { command: "startWholeGameAnalysis", args: [{ visits: 1 }] },
    { command: "play", args: [{ type: "Pass", data: { color: "Black" } }] },
  ]);

  await page.getByTestId("tool-tsumego").click();
  await expect(page.getByTestId("tsumego-dialog")).toBeVisible();
  await page.getByTestId("tsumego-close").click();
  await expect(page.getByTestId("tsumego-dialog")).toBeHidden();

  // 更多 lists the default buttons the bar has no room for; their features are not built yet.
  await page.getByTestId("tool-more").click();
  const more = page.getByTestId("tool-more-menu");
  await expect(more.getByRole("menuitem")).toHaveText(["终局点目", "纯网络"]);
  for (const item of await more.getByRole("menuitem").all()) await expect(item).toBeDisabled();
});

test("the title and status line carry what Java puts there", async ({ page }) => {
  await start(page);
  await page.evaluate(() => window.__mockBackend?.pushEvents([{ type: "EngineStatusChanged", data: { type: "Ready", data: { version: "1.18.2" } } }]));
  await expect(page).toHaveTitle(/^LizzieYzy Next 2 - \[\d+\.\d -?\d+\.\d [\d.]+k?\] kata1-tf3-b11c768 /);
  await expect(page.getByTestId("status-line")).toHaveText("kata1-tf3-b11c768 · 分析中 · 空格 暂停");
  await expect(page.getByTestId("engine-status")).toHaveAttribute("data-state", "Ready");
});
