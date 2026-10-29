---
id: P1-09
title: 棋盘渲染与交互
depends_on: [P1-08]
paths:
  - apps/desktop/src/board/**
  - apps/desktop/src/state/**
  - apps/desktop/src/styles/**
  - apps/desktop/src/locales/*.json
  - apps/desktop/src/App.tsx
  - apps/desktop/e2e/**
---

## 目标

按 ARCHITECTURE.md 第 6 节实现棋盘，在分析结果不断推送时保持流畅。

## 要做

- 三层 Canvas：
  - 底层：棋盘、网格、星位，坐标可开关。
  - 棋子层：带最后一手标记。
  - 分析层：候选点、所有权、PV 预览。
- 候选点：
  - 颜色按与最佳着的胜率差分档。
  - 文字显示胜率、visits（以 k 为单位）、目差。
  - 最佳着高亮，显示数量可配置。
- 所有权热力图可开关；鼠标悬停在候选点上时，显示 PV（带序号的半透明棋子）。
- 交互：点击空点落子（调用命令）；键盘 ←、→、Home、End 翻手；适配窗口缩放和高 DPI；提供浅色 / 深色主题变量。
- store 只由后端事件更新。关键元素加 `data-testid`，给 E2E 用：棋盘、当前手数、分析所属的手数等。

## 验收标准

1. Vitest：像素与坐标的换算（多种尺寸和 DPR）、颜色分档函数、标签格式化。
2. Playwright（mock 后端）截图基线：空的 19 路、中盘带候选点、开启所有权、悬停显示 PV、9 路。
3. Playwright：点击某点时，用正确的坐标调用落子命令；按键时调用 Step。
4. 性能：mock 以 20Hz 推送 200 帧分析，PerformanceObserver 记录到的超过 50ms 的长任务为 0（在 CI 的 ubuntu 上跑）。
