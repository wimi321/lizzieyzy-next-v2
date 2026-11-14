import { expect, test, type Page } from "@playwright/test";
import type { EngineProfile, Settings } from "../src/api/backend";
import { documentChanged, midGame } from "./fixtures";
import type { MockCommand, MockScript } from "./mock-backend";

test.use({ viewport: { width: 1280, height: 820 }, deviceScaleFactor: 1, locale: "zh-CN" });

const B18 = { program: "/opt/katago", model: "/opt/weights/kata1-b18c384nbt.bin.gz", config: "", extra_args: [] };
const LOCAL = { id: "e1", name: "B18 均衡", profile: B18 };
const SETTINGS = { version: 1, engine: B18, pondering: false, fox_history: [], engines: [LOCAL], main_engine_id: "e1" } as unknown as Settings;
const ZHIZI_PROFILE: EngineProfile = { program: "", model: "", config: "", extra_args: [], remote: { type: "Zhizi", data: { options: { plan: "OnDemand1x", weight: "10b512t" } } } };
const ZHIZI = { id: "z1", name: "智子云", profile: ZHIZI_PROFILE };
const OVERVIEW = {
  account: { phone: "138****0000", membership: true, membership_expires_at: "2026-12-31T23:59:59+08:00" },
  balance: { remaining_balance_yuan: "36.80", yesterday_consumption_yuan: "2.40", current_connections: 1 },
  recent_usage: { total: 1, page: 0, page_size: 5, items: [{ started_at: "2026-11-13T20:10:00+08:00", duration_seconds: 5400, total_cost_yuan: "4.50", finished: true }] },
};
const CHANGED = { type: "Remote", data: { type: "HostKeyChanged", data: { old_fingerprint: "SHA256:Xb0gZ3q2c6YvYk3m0pVv2kM8n1tJp0qC4dHq7wE9sLk", new_fingerprint: "SHA256:7nQ2rT0aX4bKc9ZpL1sYw8mDe3UfGh6JvBi5NoRtCxQ", path: "/Users/demo/Library/Application Support/LizzieYzy Next 2/ssh/known_hosts" } } } as const;

const row = (page: Page, id: string) => page.locator(`.menu-row[data-menu-id="${id}"]`);
const settingsMenu = (page: Page) => page.getByTestId("menubar").locator('[data-menu-id="settings"]');

async function start(page: Page, commands: MockCommand[]) {
  const script: MockScript = { initialEvents: [documentChanged(midGame(), 3)], commands: [{ command: "settings", result: SETTINGS }, ...commands] };
  await page.addInitScript((initial) => {
    window.__mockInitialScript = initial;
  }, script);
  await page.goto("/");
  await expect(page.getByTestId("board")).toHaveAttribute("data-ready", "true");
}

async function calls(page: Page) {
  return page.evaluate(() => window.__mockBackend?.recordedCalls() ?? []);
}

test("远程算力: log in to 智子, see the account, and make a pay-as-you-go plan the main engine", async ({ page }) => {
  await start(page, [
    { command: "zhiziStatus", result: { logged_in: false, store_available: true } },
    { command: "zhiziLoginPassword", result: { logged_in: true, remembered: false, store_available: true } },
    { command: "zhiziOverview", result: OVERVIEW },
    { command: "openZhiziWebsite", result: undefined, error: { type: "BrowserUnavailable" } },
    { command: "zhiziWebsite", result: "https://www.zhizigo.com/" },
    { command: "addEngine", result: { ...SETTINGS, engines: [LOCAL, ZHIZI] } },
    { command: "selectMainEngine", result: { ...SETTINGS, engine: ZHIZI_PROFILE, engines: [LOCAL, ZHIZI], main_engine_id: "z1" } },
  ]);
  await settingsMenu(page).click();
  await row(page, "remoteCompute").click();
  const dialog = page.getByTestId("remote-dialog");
  await expect(dialog.getByTestId("zhizi-login")).toBeVisible();
  await expect(dialog).toHaveScreenshot("zhizi-login.png");

  await dialog.getByTestId("zhizi-account").fill("13800000000");
  await dialog.getByTestId("zhizi-password").fill("demo password");
  await dialog.getByTestId("zhizi-submit").click();
  await expect(dialog.getByTestId("zhizi-balance")).toHaveText("余额 ¥36.80 · 昨日消费 ¥2.40");
  await dialog.getByTestId("zhizi-plan").selectOption("OnDemand1x");
  await expect(dialog).toHaveScreenshot("zhizi-account.png");
  // 充值 opens the official site; without a browser the address is shown instead.
  await dialog.getByTestId("zhizi-recharge").click();
  await expect(dialog.locator(".remote-failure")).toHaveText("无法打开浏览器，请手动打开智子官网：https://www.zhizigo.com/");

  await dialog.getByTestId("zhizi-connect").click();
  await expect(dialog.getByTestId("remote-current")).toHaveText("当前使用：智子云");
  await expect(page.getByTestId("menubar").getByRole("menuitem").last()).toHaveText("[2]: 智子云");
  expect(await calls(page)).toEqual([
    { command: "settings", args: [] },
    { command: "zhiziStatus", args: [] },
    { command: "zhiziLoginPassword", args: ["13800000000", "demo password", false] },
    { command: "zhiziOverview", args: [false] },
    { command: "openZhiziWebsite", args: [] },
    { command: "zhiziWebsite", args: [] },
    { command: "addEngine", args: ["智子云", ZHIZI_PROFILE] },
    { command: "selectMainEngine", args: ["z1"] },
  ]);
});

test("自建算力: a ws:// link is flagged as unencrypted, tested, and used", async ({ page }) => {
  const link = "ws://192.168.1.8:8080/katago?token=demo";
  const remote = { id: "w1", name: "自建算力", profile: { ...B18, program: "", model: "", remote: { type: "WebSocket", data: { host: "192.168.1.8", port: 8080, secure: false, identity: "0".repeat(64) } } } as EngineProfile };
  // Wide enough for the bottom toolbar's optional buttons, 远程算力 among them.
  await page.setViewportSize({ width: 1600, height: 820 });
  await start(page, [
    { command: "zhiziStatus", result: { logged_in: false, store_available: true } },
    { command: "testRemoteEngine", result: "1.16.0" },
    { command: "addRemoteEngine", result: { ...SETTINGS, engines: [LOCAL, remote] } },
    { command: "selectMainEngine", result: { ...SETTINGS, engines: [LOCAL, remote], main_engine_id: "w1" } },
  ]);
  await page.getByTestId("tool-remote").click();
  const dialog = page.getByTestId("remote-dialog");
  await dialog.getByTestId("remote-tab-self").click();
  await dialog.getByTestId("remote-link").fill(link);
  await expect(dialog.getByTestId("remote-insecure")).toBeVisible();
  await dialog.getByTestId("remote-test").click();
  await expect(dialog.getByTestId("remote-version")).toHaveText("已连接：1.16.0");
  await expect(dialog).toHaveScreenshot("remote-self.png");

  await dialog.getByTestId("remote-enable").click();
  await expect(dialog.getByTestId("remote-current")).toHaveText("当前使用：自建算力");
  await expect(page.getByTestId("tool-auto-setup")).toBeDisabled();
  expect((await calls(page)).slice(2)).toEqual([
    { command: "testRemoteEngine", args: [{ type: "WebSocket", data: { url: link } }] },
    { command: "addRemoteEngine", args: ["自建算力", { type: "WebSocket", data: { url: link } }] },
    { command: "selectMainEngine", args: ["w1"] },
  ]);
});

test("设置 → 引擎 → 添加 SSH 引擎: the form, and a changed host key with both fingerprints", async ({ page }) => {
  await start(page, [{ command: "testRemoteEngine", result: "", error: CHANGED }]);
  await settingsMenu(page).click();
  await row(page, "engineConfig").click();
  await page.getByTestId("engine-add-ssh").click();
  const dialog = page.getByTestId("ssh-dialog");
  await dialog.getByTestId("ssh-name").fill("实验室 4090");
  await dialog.getByTestId("ssh-host").fill("gpu.example.org");
  await dialog.getByTestId("ssh-user").fill("go");
  await dialog.getByTestId("ssh-password").fill("demo password");
  await dialog.getByTestId("ssh-command").fill("katago analysis -model kata1-b18.bin.gz -config analysis.cfg");
  await expect(dialog).toHaveScreenshot("ssh-form.png");

  await dialog.getByTestId("ssh-test").click();
  await expect(dialog.getByTestId("ssh-host-key")).toContainText("SHA256:7nQ2rT0aX4bKc9ZpL1sYw8mDe3UfGh6JvBi5NoRtCxQ");
  await expect(dialog.getByTestId("ssh-host-key")).toHaveScreenshot("ssh-host-key.png");
  expect((await calls(page)).slice(1)).toEqual([
    {
      command: "testRemoteEngine",
      args: [{ type: "Ssh", data: { host: "gpu.example.org", port: 22, user: "go", command: "katago analysis -model kata1-b18.bin.gz -config analysis.cfg", auth: { type: "Password", data: { password: "demo password" } } } }],
    },
  ]);
});
