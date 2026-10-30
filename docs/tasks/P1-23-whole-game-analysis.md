---
id: P1-23
title: 整盘自动分析：后端
depends_on: [P1-21]
paths:
  - crates/katago-protocol/**
  - crates/analysis/**
  - crates/app-core/**
  - tools/fake-katago/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
  - docs/ARCHITECTURE.md
---

## 目标

维护者日常复盘用的是"自动分析"：把一盘棋每一手都分析一遍，胜率图填满，问题手（P1-24）和棋力评估（P1-25）都从这份结果算出来。

行为参照 Java 版"整盘精析（推荐）"：
- `src/main/java/featurecat/lizzie/gui/WholeGameAnalysisDialog.java`；
- `src/main/java/featurecat/lizzie/analysis/WholeGameAnalysisOptions.java`。

界面由 Claude 做（P1-26），这张卡只做后端和命令。

## 要做

- 范围：当前显示的这条线，从根到这条变化的最后一手，每个局面都分析，包括根。
- 每手计算量的预设和 Java 版一致：
  - 快速 500、标准 1000（默认）、深入 3000、专业 10000；
  - 也可以自定义，范围 500 到 1,000,000。
- 用分析协议的 `analyzeTurns` 批量提交，优先级低于实时分析：整盘分析进行中，当前局面的实时分析照常更新、不明显变慢。
- 进度事件：已完成 / 总数、状态（进行中、已暂停、已完成、已取消、出错）。
- 控制：
  - 暂停：停掉剩下的；
  - 继续：只补没做完的局面，不重复已完成的；
  - 取消。
- 已经有足够计算量的局面（不少于本次目标）直接跳过。
- 结果：
  - 按当前棋谱保存在内存里，不会被实时分析的 LRU 缓存挤掉；
  - 每个局面保留完整的分析结果：根节点的胜率和目差、候选点列表及其 PV，供 P1-24、P1-25 使用；
  - 同时并入胜率图摘要，图随进度填满；
  - 翻到已分析的局面时，先显示这份结果，再由实时分析接着往深算。
- 打开别的棋谱时取消任务并清空结果；引擎档案变化时同样清空，和 P1-13 的摘要规则一致。分析途中引擎重启时，任务要么接着完成，要么明确报错，不能卡住。
- fake-katago 如果缺 `analyzeTurns` 或优先级相关的行为，按需补上。
- 提供 Tauri 命令和 ts-rs 绑定，mock-backend 跟着更新。

## 验收标准

1. 集成测试（fake-katago）：
   - 开始后进度递增到总数；
   - 这条线上每个局面都有结果，计算量不少于目标；
   - 胜率图每一点都有摘要。
2. 整盘分析进行中，实时分析仍然在当前局面上持续出结果。
3. 取消：之后不再有进度，引擎收到 terminate。暂停再继续：只补剩下的，没有重复提交。
4. 已分析够的局面被跳过；打开另一盘棋后，任务取消、结果清空。
5. 分析途中让引擎退出（故障注入）：任务完成，或者给出明确的错误状态。
6. `cargo test -p analysis` 和 `cargo test -p app-core` 各连续跑 10 遍，再在 CPU 满载下各连跑 10 遍，全部通过。
