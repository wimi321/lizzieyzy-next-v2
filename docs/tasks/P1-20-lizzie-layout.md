---
id: P1-20
title: 主窗口按 LizzieYzy Next 布局重做
depends_on: [P1-17]
paths:
  - apps/desktop/src/**
  - apps/desktop/index.html
  - apps/desktop/e2e/**
  - apps/desktop/src-tauri/tauri.conf.json
---

## 目标

维护者试用后反馈"界面跟 LizzieYzy Next 完全不一样"。新版的用户都是 LizzieYzy Next 的老用户，主窗口要延续 Java 版的布局、外观和操作习惯，参照 Java 版 README 里的 `assets/interface-overview-2026-04.png`。界面由 Claude 做。

## 要做

- 顶部：
  - 窗口内菜单栏：文件、显示、棋局、分析、设置……，只放已经实现的功能，做好一个加一个；
  - 一排工具栏：打开、保存、野狐棋谱、导航、暂停分析、引擎状态等。
- 左栏：
  - 黑白双方、规则、贴目、提子；
  - 胜率条，黑 % | 白 %；
  - "黑领先 x.x 目，最后一手 (#n)：±x% ±x 目"；
  - 深色胜率/目差图，点击和拖动跳转。
- 中间：
  - 木纹棋盘；
  - 候选点圈里三行：胜率、计算量、目差；最佳点绿色，其余按原版配色；显示候选顺序号。
- 右栏：
  - 候选点表格：序号、胜率、计算量、占比、目数；
  - 变化树，数据由 P1-21 提供。
- 窗口缩放时棋盘尽量大，侧栏宽度合理。
- 后端暂缺的数据先留好位置，由 P1-21 补：对局双方信息、完整变化树、变化预览。

## 验收标准

1. Claude 先截图给维护者。维护者对照 Java 版，确认布局和外观。
2. Vitest、Playwright 通过；截图基线在 Linux 上重新生成。
3. 原有功能都还能用，E2E 通过：打开、保存、翻手、胜率图跳转、暂停分析、引擎设置。
