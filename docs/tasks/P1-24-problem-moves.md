---
id: P1-24
title: 问题手：后端
depends_on: [P1-23]
paths:
  - crates/document/**
  - crates/app-core/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
  - docs/ARCHITECTURE.md
---

## 目标

和 Java 版一样，从整盘分析（P1-23）的结果里找出问题手，给出每一手的损失和等级，供问题手面板和胜率图下方的失误条使用。

行为参照 Java 版：
- `src/main/java/featurecat/lizzie/analysis/MoveRankDefinition.java`；
- `src/main/java/featurecat/lizzie/analysis/MoveRankEvaluationMode.java`；
- `src/main/java/featurecat/lizzie/gui/BlunderListPanel.java`。

界面由 Claude 做（P1-26）。

## 要做

- 每一手的损失，站在落子方的角度算：
  - 落子前那个局面里最佳候选点的胜率和目差，减去落子后局面的胜率和目差；
  - 还要给出：这手是不是引擎的首选，在候选点里排第几（不在候选点里也要能表示）。
- 等级：照 Java 版 `MoveRankDefinition` 的五级规则和默认阈值。
  - 胜率损失阈值：1、3、6、12、24 个百分点；
  - 目差损失阈值：0.5、1.5、3、6、12 目；
  - 评估模式（只看胜率 / 只看目差 / 同时考虑）照 Java 版。
  - 阈值和模式存进设置，可以修改。
- 问题手列表：
  - 达到设定等级的手，可以按黑白筛选；
  - 排序和 Java 版一致；
  - 每项包含手数、颜色、坐标、胜率损失、目差损失、等级、计算量，以及对应节点 id，用来跳转。
- 失误条数据：这条线上每一手的损失和等级，没分析的为空。
- 只读：不改棋谱，不触发分析。整盘分析进度更新时，列表跟着更新。
- 提供 Tauri 命令或事件和 ts-rs 绑定，mock-backend 跟着更新。

## 验收标准

1. 单元测试：
   - 损失计算：黑白两方的视角、首选手损失为 0、不在候选点里的手；
   - 等级划分：边界值和三种模式，与 Java 版 `MoveRankDefinition` 的测试用例对照。
2. 集成测试（fake-katago）：整盘分析后，问题手列表和失误条数据与预期一致；黑白筛选正确；跳转用的节点 id 正确。
3. 修改阈值后，列表按新阈值更新，设置重启后还在。
4. `scripts/verify.sh` 通过，绑定一致性测试通过。
