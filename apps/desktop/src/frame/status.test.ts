import { expect, test } from "vitest";
import zh from "../locales/zh-CN.json";
import { statusLine } from "./status";

const ready = { type: "Ready", data: { version: "1.16.0" } } as const;

test("the status line names the engine, its state and the key; analysis read from the file says so", () => {
  const base = { engine: ready, name: "B11", configured: true, autoAnalyzing: false, pondering: false, labels: zh.frame.status, states: zh.engine };
  expect(statusLine(base)).toEqual({ text: "B11 · 已暂停 · 空格 继续", detail: "1.16.0" });
  expect(statusLine({ ...base, saved: true }).text).toBe("B11 · 已暂停 · 空格 继续 · 分析来自棋谱");
  expect(statusLine({ ...base, configured: false, name: null }).text).toBe("未加载引擎 · 已暂停 · 空格 继续");
});
