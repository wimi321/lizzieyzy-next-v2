---
id: P3-02
title: 四种新界面语言的译文：繁体中文、日语、韩语、泰语
depends_on: [P2-16]
paths:
  - apps/desktop/src/locales/**
---

## 目标

Java 版支持的界面语言，除了简体中文和英文，还有繁体中文、日语、韩语、泰语。本卡补上这四种语言的界面译文，让切换后整个界面都是该语言。设置里的语言选项由 P3-01 加，与本卡互不依赖。

## 要做

1. 在 `apps/desktop/src/locales/` 新增 `zh-TW.json`、`ja.json`、`ko.json`、`th.json`，key 与 `en.json` 完全一致。
2. 译文来源：
   - 先用 Java 版的译文（`locales/legacy/` 下的 `zh-TW.json`、`ja-JP.json`、`ko.json`、`th-TH.json`）。对应方法：拿新 key 在 `zh-CN.json` 和 `en.json` 里的文案，去匹配 Java 的 `zh-CN` 和 `en` 原文，匹配上的用同一个 Java key 的译文。
   - 匹配不上的，参照 `zh-CN.json` 和 `en.json` 翻译。
   - 繁体中文用台湾用语，围棋术语照 Java 的 `zh-TW` 译文。
3. 术语全文统一，例如胜率、目差、计算量、贴目、提子、死活、选点、变化图。汇报里给出每种语言的术语表。
4. 占位符（例如 `{version}`、`{done}`）、快捷键写法和标点符号规则保持原样。界面空间有限，译文不要比英文长太多，可以参照 Java 译文的长度。
5. 在 `locales/index.ts` 里注册：`getMessages` 按语言代码返回对应译文。zh-TW、zh-HK、zh-Hant 返回繁体中文，其他中文返回简体，ja、ko、th 返回各自的语言，其余返回英文。

## 不做

- 菜单和设置里的语言选项、界面上的其他改动：P3-03。

## 验收标准

1. Vitest 测试：每种新语言的 key 集合与 `en.json` 相同；每条译文的占位符集合与英文相同；没有空串；`getMessages` 的语言映射正确。
2. 汇报里给出：每种语言有多少条来自 Java 译文、多少条是新译的，以及术语表。
3. `scripts/verify.sh` 通过。
