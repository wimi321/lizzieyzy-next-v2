import { expect, test, type Page } from "@playwright/test";
import type { DocumentView, Settings } from "../src/api/backend";
import type { CommentarySettings, CommentaryUpdated } from "../src/api/commentary";
import { documentChanged, midGame } from "./fixtures";
import type { MockCommand, MockScript } from "./mock-backend";

test.use({ viewport: { width: 1280, height: 820 }, deviceScaleFactor: 1, locale: "zh-CN" });

const ENGINE = { program: "/opt/homebrew/bin/katago", model: "/opt/katago/kata1.bin.gz", config: "", extra_args: [] };
const SERVICE = { base_url: "https://ai.example.org/v1", model: "demo-model", remember_key: true };
const SETTINGS = { version: 1, engine: ENGINE, pondering: false, fox_history: [], commentary: SERVICE } as unknown as Settings;
const KEY = { has_key: true, stored: true, available: true };
const ANSWER = [
  "### 这一手的关键",
  "",
  "白棋的重点是先把**右边的厚味**变成实地，KataGo 的首选 `R10` 正是这个意思；实战下在左边，节奏慢了一步。",
  "",
  "- 黑棋如果抢到 R10，右边白棋会变成需要治理的孤棋",
  "- 首选让白棋领先约 2 目，实战之后变成基本均势",
].join("\n");
const updated = (view: CommentaryUpdated) => ({ type: "CommentaryUpdated", data: view }) as const;

async function start(page: Page, settings: Settings, commands: MockCommand[], view: DocumentView = midGame()) {
  const script: MockScript = { initialEvents: [documentChanged(view, 3)], commands: [{ command: "settings", result: settings }, ...commands] };
  await page.addInitScript((initial) => {
    window.__mockInitialScript = initial;
  }, script);
  await page.goto("/");
  await expect(page.getByTestId("board")).toHaveAttribute("data-ready", "true");
}

async function calls(page: Page) {
  return page.evaluate(() => window.__mockBackend?.recordedCalls() ?? []);
}

test("AI 解说 shows the answer while it is written, then finished and added to the comment", async ({ page }) => {
  const evidence = "第 31 手之后的局面\n落子方: 白\n根节点搜索: 1600 次\n候选 #1 R10 胜率 56.2% 领先 2.1";
  await start(page, SETTINGS, [
    { command: "commentaryKeyStatus", result: KEY },
    { command: "startCommentary", result: { request_id: 1, phase: "Preparing" }, events: [updated({ request_id: 1, phase: "Generating", text: ANSWER.slice(0, 48), evidence })] },
  ]);
  await page.getByTestId("commentary-button").click();
  const panel = page.getByTestId("commentary-window");
  await expect(panel.getByTestId("commentary-start")).toHaveText("开始解说");
  await panel.getByTestId("commentary-start").click();
  await expect(panel.getByTestId("commentary-stop")).toBeVisible();
  await expect(panel.getByTestId("commentary-status")).toContainText("正在生成解说");
  await expect(panel).toHaveScreenshot("commentary-generating.png");

  await page.evaluate(
    ([text, sent]) => window.__mockBackend?.pushEvents([{ type: "CommentaryUpdated", data: { request_id: 1, phase: "Completed", text, evidence: sent } }]),
    [ANSWER, evidence] as const,
  );
  await expect(panel.getByTestId("commentary-status")).toContainText("已加入棋谱评论");
  await expect(panel.locator(".markdown li")).toHaveCount(2);
  await expect(panel).toHaveScreenshot("commentary-done.png");
  expect(await calls(page)).toEqual([
    { command: "settings", args: [] },
    { command: "commentaryKeyStatus", args: [] },
    { command: "startCommentary", args: [{ mode: { type: "NextMove" }, follow_up: null }] },
  ]);
});

test("AI 解说设置: 连接 AI opens it; saving stores the service, then the key", async ({ page }) => {
  const none = { has_key: false, stored: false, available: true };
  const saved: CommentarySettings = { ...SERVICE, rank: "5k", style: "Balanced", terminology: "Medium", detail: "Standard", variations: "Moderate", write_to_comment: true };
  await start(page, { ...SETTINGS, commentary: undefined }, [
    { command: "commentaryKeyStatus", result: none },
    { command: "commentaryKeyStatus", result: none },
    { command: "setCommentarySettings", result: saved },
    { command: "setCommentaryKey", result: KEY },
  ]);
  await page.getByTestId("commentary-button").click();
  const panel = page.getByTestId("commentary-window");
  await expect(panel.getByTestId("commentary-start")).toHaveText("连接 AI");
  await panel.getByTestId("commentary-start").click();
  const setup = page.getByTestId("commentary-setup");
  await setup.getByTestId("commentary-url").fill(SERVICE.base_url);
  await setup.getByTestId("commentary-key").fill("sk-demo");
  await setup.getByTestId("commentary-model").fill("demo-model");
  await setup.getByTestId("commentary-remember").check();
  await expect(setup).toHaveScreenshot("commentary-settings.png");

  await setup.getByTestId("commentary-save").click();
  await expect(setup).toBeHidden();
  await expect(panel.getByTestId("commentary-start")).toHaveText("开始解说");
  expect(await calls(page)).toEqual([
    { command: "settings", args: [] },
    { command: "commentaryKeyStatus", args: [] },
    { command: "commentaryKeyStatus", args: [] },
    { command: "setCommentarySettings", args: [saved] },
    { command: "setCommentaryKey", args: ["sk-demo", true] },
  ]);
});

test("the comment panel shows the AI 解说 block under its own heading, apart from the user's note", async ({ page }) => {
  const block = ["[LizzieYzy AI Commentary BEGIN]", "generatedAt=2026-11-14T09:30:12Z", "model=demo-model", "", ANSWER, "", "追问：讲简单一点", "", "先把右边走厚，再去别处。", "[LizzieYzy AI Commentary END]"];
  await start(page, SETTINGS, [], { ...midGame(), comment: `右边的厚味要记得利用。\n\n${block.join("\n")}` });
  const notes = page.getByTestId("comment");
  await expect(notes.getByTestId("comment-ai").locator("h4")).toHaveText("追问：讲简单一点");
  await expect(notes.getByTestId("comment-text").locator("> p")).toHaveText("右边的厚味要记得利用。");
  await expect(notes).toHaveScreenshot("comment-ai-block.png");
});
