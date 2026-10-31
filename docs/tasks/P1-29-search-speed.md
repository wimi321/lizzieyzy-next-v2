---
id: P1-29
title: 实时分析速度
depends_on: [P1-27]
paths:
  - crates/analysis/**
  - crates/app-core/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/**
  - apps/desktop/e2e/**
  - docs/ARCHITECTURE.md
---

## 目标

维护者 2026-10-31：「没有显示出实时分析的速度，也加上吧」。工具栏在计算量旁边显示当前的搜索速度，单位是每秒计算量。

分工：后端由 Codex 做。工具栏上的显示由 Claude 在同一分支上做。Codex 在前端只改 `src/api/` 和 mock-backend。

## 要做

- **怎么算。** 只针对实时分析，不含整盘分析。
  - 每收到一份中间结果，记下到达时间（`tokio::time::Instant`）和根节点计算量。
  - 速度 =（最近 3 秒内最后一份结果的计算量 − 第一份的计算量）÷ 两份之间的时间差。
  - 时间差不足 0.5 秒时，没有速度。
- **什么时候重算，什么时候保留。**
  - 换局面、引擎重启、查询重新发出时，从头重算。
  - 搜索停下后（到了计算量上限或暂停分析），保留最后一次的速度。
- **算在哪里。** 放在分析协调器（`analysis`）或 app-core，二选一，在汇报里说明理由。
- **送到界面。** 速度随实时分析结果一起送到界面：`AnalysisView` 加一个字段，例如 `visits_per_second: number | null`。ts-rs 绑定和 mock-backend 跟着改。mock-backend 要给出一个固定、合理的值。

## 验收标准

1. 单元测试，用 tokio 的暂停时间：
   - 结果均匀到达时，算出的速度正确；
   - 不足 0.5 秒时为空；
   - 换局面后清零，从头重算；
   - 搜索停下后保留最后的速度。
2. 集成测试（fake-katago）：实时分析一段时间后，`AnalysisView` 带有速度。
3. `scripts/verify.sh` 通过，绑定一致性测试通过。
