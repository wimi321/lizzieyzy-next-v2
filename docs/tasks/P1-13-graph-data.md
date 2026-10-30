---
id: P1-13
title: app-core：胜率图数据
depends_on: [P1-08]
paths:
  - crates/app-core/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - docs/ARCHITECTURE.md
---

## 目标

给 P1-10 的胜率图提供数据：当前这条线上每一手已知的最好分析。界面只负责画，不在 TS 里攒分析数据，也不判断换没换棋谱。

## 要做

- app-core 为当前文档记录每个节点的分析摘要：黑方胜率、目差、visits。同一节点只保留 visits 最多的一次，和协调器的显示下限一致。打开新棋谱时清空。
- `DocumentView` 增加 `graph: GraphView`：
  - 显示线：从根到当前节点，再沿第一个子节点一直走到底。这样当前节点总在线上，进入变化时图跟着变化走。
  - 线上每个节点一项：`node_id`、手数、摘要（没分析过为 null）。
  - 当前节点在线上的下标。
- `AnalysisUpdated` 不变。界面收到它时，用其中的 node_id 和根胜率、目差、visits 覆盖本地图数据里对应的那一项；协调器保证 visits 不会倒退，所以直接覆盖是对的。在 ARCHITECTURE.md 里写明这条约定。
- 重新生成 TS 绑定，mock-backend 跟着更新类型。

## 验收标准

1. 无界面集成测试（fake-katago）：打开语料，等根节点有分析，前进几手再退回根；`DocumentChanged` 里的图数据对分析过的节点给出 visits 最多的摘要，没分析过的为 null。
2. 进入变化后，显示线经过当前节点并沿该变化的第一个子节点走到底；当前下标正确。
3. 打开另一个棋谱后，图数据里没有旧棋谱的摘要。
4. 绑定一致性测试通过。
5. `cargo test -p app-core` 和桌面 crate 的测试各连续跑 10 遍，再在 CPU 满载下连跑 10 遍，全部通过。
