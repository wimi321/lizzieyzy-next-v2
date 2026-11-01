---
id: P1-39
title: 单步分析限制：分析够了自动停
depends_on: [P1-38]
paths:
  - crates/app-core/**
  - crates/analysis/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/**
  - apps/desktop/e2e/**
  - docs/ARCHITECTURE.md
---

## 目标

维护者 2026-11-01：「lizzieyzy next 上棋盘分析超过一定程度就自动停止了，免得忘了关一直分析占硬件，学习一下呢」。

LizzieYzy Next 的「单步分析限制」：
- 同一步分析超过最大时间后，自动停止分析，按空格或新落一子可以继续；
- 限制分析时长默认开，上限 600 秒（`limit-time`、`max-analyze-time-seconds`）；
- 限制分析计算量默认关，打开后上限 100000（`limit-playout`、`limit-playouts`）；
- 停下时提示「超过时间或计算量限制，计算已停止，按空格键继续」。

分工：后端由 Codex 做。停止时的提示和「设置 → 综合设置」里的设置项，由 Claude 在同一分支上做。Codex 在前端只改 `src/api/` 和 mock-backend。

## 要做

1. **行为参照 Java 版：**
   - `analysis/Leelaz.java` 中 `stopByLimit` 附近的判断（约 7525–7550 行）；
   - `startPonderTime` 在什么时候重置；
   - `Config.java` 的 `limitTime`、`maxAnalyzeTimeMillis`、`limitPlayout`、`limitPlayouts`；
   - `ConfigDialog2.java` 的对应设置；
   - 以及它们的测试。
2. **设置。** 新增四项：限制时长开关、秒数、限制计算量开关、计算量。默认值照 Java。旧设置文件缺这几项时用默认值。补上读取和修改的命令。
3. **实时分析的停止条件。** 计时和计数针对「当前这一步」，以下任一满足就停止实时分析：
   - 时间：当前局面的实时分析已经持续了上限秒数；
   - 计算量：根节点计算量超过上限。

   停止后：
   - 这是一次临时暂停。持久保存的「分析开关」设置不改，下次启动照常分析；
   - 停下时留下最后的分析结果，界面照常显示。
4. **继续的规则。** 照 Java：
   - 按空格（切换分析）继续；
   - 新落一子继续。

   导航到别的局面时是否继续，照 Java 的实际行为来做，并在汇报里说明。
5. **不受限制的情况。** 整盘分析（手动和自动速览）不受这个限制。
6. **事件。** 停止时发出带原因的事件或状态（时间或计算量），界面据此提示。Tauri 命令、ts-rs 绑定和 mock-backend 跟着改。

## 验收标准

1. 单元测试和集成测试（fake-katago，tokio 暂停时间）：
   - 到时间停止；
   - 到计算量停止；
   - 开关关闭不停；
   - 停止后不改变持久的分析开关；
   - 按空格或落子后继续；
   - 整盘分析不受影响；
   - 旧设置文件使用默认值。
2. `scripts/verify.sh` 通过，绑定一致性测试通过。
