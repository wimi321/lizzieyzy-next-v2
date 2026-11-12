---
id: P2-04
title: 棋谱编辑（三）后端：插入着手、指定颜色落子、改棋子、添加为分支、试下
depends_on: [P2-03]
paths:
  - crates/document/**
  - crates/sgf/**
  - crates/app-core/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
  - docs/ARCHITECTURE.md
---

## 目标

P2 第 1 批「SGF 编辑」的第三部分后端：改动已经下过的着手，以及试下。界面由 Claude 另做，这张卡不碰 `apps/desktop/src` 里 `api/` 以外的文件。

Java 参考在 `../lizzieyzy-next`，只学行为。每一项你参考了哪个函数、确认到的行为是什么，写进汇报。

## 要做

1. **指定颜色落子。** Java：「编辑 → 添加黑子、添加白子、交替落子」，入口在 `gui/Menu.java`。
   - 添加黑子、添加白子模式下，连续落同一种颜色，不检查轮到谁。增加一个指定颜色、跳过轮走方检查的落子命令；落子规则（提子、禁着点、劫）照常检查。
   - 写进 SGF 的形式照 Java。
2. **插入着手。** Java：`LizzieFrame.insertMove`。
   - `insert_move(move)`：在当前手和它后面的着手之间插入一手，后面的着手照旧接上。后面的着手因此变得不合法时怎么处理，照 Java。
   - 一次撤销恢复原样。
3. **改已经下过的棋子。** 棋盘右键菜单里的「反色」「删除」「移动」。Java：`Board.editMove(coords, isSwitch, isDelete)`，「移动」的实现你在 Java 里找。
   - 按棋盘上的点找到下出这颗子的那一手，照 Java 改：反色是改颜色，删除是去掉那一手，移动是换到另一个点。之后的着手怎么接，照 Java。
   - 每个操作一次撤销恢复原样。
4. **跳到此步、回看。** 棋盘右键单击棋子。Java：`Board.moveNumberByCoord`、`getBoardHistoryNodeByCoords` 和右键菜单里「回看」的实现。
   - 提供一个只读查询：棋盘上某点的棋子是哪个节点下的。界面用它跳过去。「回看」如果和「跳到此步」不同，照 Java 补上需要的数据。
5. **添加为分支。** Java：`LizzieFrame.addSuggestionAsBranch`、`BoardRenderer.addSuggestionAsBranch`。
   - `add_variation(moves)`：把一串着手（通常是某个候选点的变化）作为当前手下面的一个分支加进棋谱。焦点停在哪里，照 Java。已有相同的开头时怎么合并，照 Java。
   - 一次撤销恢复原样。
6. **试下。** Java：`LizzieFrame.tryPlay`。
   - `start_trial()`：从当前局面开始试下。试下期间可以随意落子、翻手、分析，原棋谱不受影响。
   - `end_trial()`：回到原棋谱、原来那一手，原棋谱和它的撤销记录、保存状态都不变。
   - `DocumentView` 增加 `trial`（是否在试下中）。试下期间保存、最近打开、文件名怎么处理，照 Java；照 Java 做不合理的地方，选最保守的做法，写进汇报。
   - 死活题会话已经有「替换文档、退出时恢复」的做法，可以参照，但两者不能互相干扰：试下中开始死活题、死活题中开始试下，结果要明确，并有测试。
7. 新字段照约定：生成的 TS 里可选，mock 补默认值；会改状态的命令在 mock 里是脚本化的 `command`，只读查询是带默认值的 `query`。

## 不做

- 起始局面、摆子、标记：P2-03。
- 界面。

## 验收标准

1. 每个新命令都有 app-core 的集成测试：正常路径、撤销和重做、错误路径。
2. 插入、反色、删除、移动各有一条对照 Java 的测试，用一盘有分支的棋谱，核对之后的主线和分支。
3. 试下：开始、落子、结束之后，原棋谱的内容、当前手、`modified`、`can_undo` 都和开始前一样。
4. Tauri 层和 TS API 的测试覆盖每个新命令；`docs/ARCHITECTURE.md` 的 4.3 和 4.7 更新。
