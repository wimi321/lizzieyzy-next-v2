---
id: P1-30
title: 棋力评估：分阶段评估
depends_on: [P1-26]
paths:
  - crates/strength/**
  - crates/app-core/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/**
  - apps/desktop/e2e/**
  - fixtures/strength/**
  - docs/ARCHITECTURE.md
---

## 目标

维护者 2026-10-31：「棋力评估少了分段评估的功能」。

照 LizzieYzy Next 棋力评估窗口里的「分阶段」页来做：把一盘棋分成布局、中盘、官子三段，分别估出双方在每一段的段位。分界线可以拖动。

分工：后端由 Codex 做。评估窗口里的「分阶段」页由 Claude 在同一分支上做。Codex 在前端只改 `src/api/` 和 mock-backend。

## 要做

1. **行为照 Java 版。** 参照 `gui/StrengthPhases.java` 和它的测试：
   - 默认分界是 60 和 150，分别是布局和中盘的最后一手；
   - 每段至少 10 手。分界不合法时用默认值，规则照 `normalize`；
   - 每段、每方只用自己在这段里下的棋，段位用整盘评估的同一套算法来估（Java 的 `summarizeSamples`）；
   - 已分析的棋少于 6 手时，不给段位（`ranked`）；
   - 标出每方表现最好和最差的阶段。两者差距不到 0.3 段就算发挥平稳，不标（`extreme`）；
   - 最差阶段比最好阶段低 0.6 段以上时，标记为值得复盘（`worthReviewing`）；
   - 没下到的阶段，标记为未下到。
2. **接口。**
   - strength crate 提供纯函数。
   - app-core 的棋力评估查询接受可选的分界（两个手数）。
   - 返回内容：三段结果、实际使用的分界、每方最好和最差的阶段、是否值得复盘。
   - 整盘分析结果更新时，这些结果跟着更新。
3. **兼容。** 现有整盘评估的输出（包括已有的 `opening` / `middlegame` / `endgame` 统计）保持不变。Tauri 命令、ts-rs 绑定、mock-backend 跟着改。

## 验收标准

1. 单元测试：
   - 分界规整：缺省、两条太近、顺序颠倒；
   - 每段的估计，等于把这段的样本单独送进整盘评估得到的结果；
   - 样本不足时不给段位；
   - 最好和最差阶段的判定，以及发挥平稳的判定；
   - 没下到的阶段。
2. 集成测试（fake-katago）：整盘分析之后，按默认分界和自定义分界查询，都能拿到三段结果。
3. `scripts/verify.sh` 通过，绑定一致性测试通过。
