---
id: P1-38
title: 打开棋谱后自动生成胜率曲线
depends_on: [P1-23]
paths:
  - crates/app-core/**
  - crates/analysis/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/**
  - apps/desktop/e2e/**
  - docs/ARCHITECTURE.md
---

## 目标

维护者 2026-11-01：「lizzieyzy next 打开棋谱以后，都会自动出个胜率曲线。我们这个重构版本也这样吧」。

LizzieYzy Next 的做法：
- 打开棋谱后，如果主线还有没分析的手，就自动跑一遍「闪电分析」，几秒钟画出整盘胜率曲线；
- 每手默认只算 1 次，配置项 `analysisMaxVisits = 1`；
- 开关是「综合设置 → 打开棋谱后自动快速分析」，配置项 `auto-quick-analyze-on-load`，默认打开。

分工：后端由 Codex 做。界面由 Claude 在同一分支上做，包括开关的位置、进度显示和「分析 → 整盘速览（闪电）」的接线。Codex 在前端只改 `src/api/` 和 mock-backend。

## 要做

1. **行为参照 Java 版：**
   - `gui/LizzieFrame.java` 的 `shouldAutoQuickAnalyzeLoadedGame`、`startAutomaticQuickAnalysis`、`ensureAnalysisResumedAfterLoad` 及相关部分；
   - `gui/AutomaticQuickAnalysisTask.java`、`gui/AutomaticQuickAnalysisEngineAdapter.java`；
   - 以及它们的测试。
2. **设置。** 新增「打开棋谱后自动快速分析」，默认开。旧的设置文件没有这一项时，按开处理。补上读取和修改它的命令。
3. **自动速览。** 新打开一盘棋后自动开始。新打开包括本地文件、导入、野狐棋谱。用现有的整盘分析机制（P1-23），每手 1 次计算量。
   - 在下面这些情况下，不自动开始：
     - 开关关闭；
     - 主线已经全部分析过（缓存命中也算）；
     - 正在进行手动整盘分析；
     - 用户对这盘棋取消过自动速览。同一盘棋不再自动开始，换一盘棋再恢复，照 Java 的 `userCancelledQuickAnalysisRoot`。
   - **不影响实时分析**：P1-23 已经有两个搜索槽，实时分析照常进行，不暂停。
   - 用户手动开始整盘分析时，自动速览让位；手动分析算到的结果照常覆盖。
4. **区分来源。** 整盘分析的进度里注明这次是自动速览还是手动分析，例如 `WholeGameProgress` 加一个字段。界面据此决定显示方式。ts-rs 绑定、mock-backend 跟着改。

## 验收标准

1. 集成测试（fake-katago）：
   - 打开棋谱后自动开始 1 次计算量的整盘分析，进度标为自动，实时分析不中断；
   - 开关关闭时不开始；
   - 已经全部分析过时不开始；
   - 取消后，同一盘棋重新打开不再自动开始，换一盘棋恢复；
   - 手动整盘分析会取代自动速览；
   - 设置能保存，旧设置文件默认开启。
2. `scripts/verify.sh` 通过，绑定一致性测试通过。
