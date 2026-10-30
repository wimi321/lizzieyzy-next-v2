---
id: P1-10
title: 胜率图、导航与工具栏
depends_on: [P1-09, P1-13]
paths:
  - apps/desktop/src/graph/**
  - apps/desktop/src/toolbar/**
  - apps/desktop/src/state/**
  - apps/desktop/src/locales/*.json
  - apps/desktop/src/App.tsx
  - apps/desktop/src/App.test.tsx
  - apps/desktop/src/api/backend.ts
  - apps/desktop/package.json
  - apps/desktop/e2e/**
---

## 目标

做出复盘的基本操作面板。

## 要做

- 胜率图：黑方胜率曲线、目差（副轴或独立小图）、已分析范围、当前手标记；点击或拖动跳转。数据来自 `DocumentView.graph`（P1-13）；收到 `AnalysisUpdated` 时，用其中的 `summary` 覆盖对应节点的那一项。
- 工具栏：
  - 打开、保存。
  - 首手、上一手、下一手、末手，显示当前手数。
  - 暂停 / 恢复分析。
  - 引擎状态灯：Starting / Ready / Restarting / Failed，悬停显示原因。
- 所有文字走 i18n，zh-CN 和 en 都要有。关键元素加 `data-testid`，给 E2E 用。

## 验收标准

1. Vitest：坐标映射、跳转手数的计算。
2. Playwright：
   - 点击图上的位置，跳到正确的手数；拖动时连续跳转。
   - 状态灯的四种状态（由 mock 驱动）。
3. 截图基线：有分析数据的胜率图、没有数据时的空状态。
4. zh-CN 和 en 的键集合一致（用测试保证）。
