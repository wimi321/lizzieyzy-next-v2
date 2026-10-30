---
id: P1-14
title: 首次设置界面
depends_on: [P1-09, P1-11]
paths:
  - apps/desktop/src/settings/**
  - apps/desktop/src/locales/*.json
  - apps/desktop/src/App.tsx
  - apps/desktop/src/App.test.tsx
  - apps/desktop/src/api/backend.ts
  - apps/desktop/src/styles/**
  - apps/desktop/e2e/**
---

## 目标

没有配置引擎时，用户在一个界面里就能接上 KataGo。界面由 Claude 做，后端是 P1-11。

## 要做

- 没有配置引擎时自动打开设置界面；也能从界面上随时打开。
- 显示探测结果，默认选中推荐项；可以用打开对话框手动选择程序、模型、配置。配置不选时使用应用自带的默认配置。
- 点"启动"后先试后存：显示正在启动；成功后关闭界面、开始分析；失败时显示可读的原因和 stderr 末尾几行，设置不变。
- 所有文字走 i18n，zh-CN 和 en 都要有；关键元素加 `data-testid`。

## 验收标准

1. Vitest 和 Playwright（mock 后端）：探测结果的展示和选择、手动选择文件、启动成功、启动失败四种流程。
2. 在本机（Homebrew KataGo）首次启动时，能自动探测到引擎并进入 Ready（Claude 验收时实测）。
3. macOS 安装包能走通 ACCEPTANCE.md 中 P1 手工冒烟的第 1–6 步。
