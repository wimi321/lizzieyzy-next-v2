---
id: P2-03
title: 棋谱编辑（二）后端：起始局面、摆子、标记、交换落子顺序
depends_on: [P2-01]
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

P2 第 1 批「SGF 编辑」的第二部分后端：起始局面设置、摆子、棋盘标记、交换落子顺序。界面由 Claude 另做，这张卡不碰 `apps/desktop/src` 里 `api/` 以外的文件。

Java 参考在 `../lizzieyzy-next`，只学行为。每一项你参考了哪个函数、确认到的行为是什么，写进汇报。

## 要做

1. **起始局面。** Java：`rules/Board.java` 的 `setSetupMode`、`addStone`、`removeStone`、`setupClearAll`、`convertCurrentPositionToStartingPosition`；`gui/LizzieFrame.java` 的 `toggleSetupMode`、`setupClearAllCommand`、`convertCurrentPositionToStartingPositionCommand`；菜单在 `gui/Menu.java`「棋局 → 起始局面设置」。
   - `set_setup_stone(point, color)`：放置黑子、放置白子；`color` 为空时擦除该点。改的是哪个节点（根节点还是当前节点）、后面已有着手时怎么处理，照 Java。
   - `clear_setup()`：清空全部设置子。
   - `set_first_player(color)`：黑先、白先。
   - `convert_to_start_position()`：将当前盘面设为起始局面。棋局信息怎么保留、原来的着手和分支怎么处理，照 Java。
   - 都能撤销和重做，返回 `Revision`。`convert_to_start_position` 如果按 Java 是换掉整个文档，就照 `new_game` 的处理，并在汇报里说明。
2. **标记。** Java：`LizzieFrame.setMarkupType`；读写 `TR`、`SQ`、`CR`、`MA`、`LB` 的地方在 `gui/BoardRenderer.java`（约 4025 行起）和 `gui/IndependentMainBoard.java`（约 1146 行起）。
   - `set_markup(point, kind)`：`kind` 是三角、方块、圆圈、叉、字母、数字、橡皮擦之一。字母和数字取这一手上还没用过的下一个（A、B、C……，1、2、3……），规则照 Java。同一点再放同种标记是去掉还是不变，照 Java。
   - `clear_markup()`：清掉当前手的全部标记。
   - 都能撤销和重做。
   - `DocumentView` 增加当前手的标记列表 `markup`：每项是点、种类、字母或数字的文字。导入的棋谱里已有的标记也要列出来。
3. **交换落子顺序。** Java：顶部工具栏 `toolbar.change-turn`，入口在 `gui/Menu.java`。
   - `toggle_to_play()`：照 Java 写哪个属性、写在哪个节点。能撤销和重做。
   - 换了轮走方，分析要跟着换局面（`AnalysisKey` 已含轮走方，确认一下）。
4. 新字段照约定：生成的 TS 里可选，`e2e/mock-backend.ts` 补默认值（`markup` 默认空列表）；会改状态的命令在 mock 里是脚本化的 `command`。

## 不做

- 插入着手、指定颜色落子、改棋子、添加为分支、试下：P2-04。
- 界面。

## 验收标准

1. 每个新命令都有 app-core 的集成测试：正常路径、撤销和重做、错误路径（点在棋盘外、节点不存在）。
2. 摆子和标记保存成 SGF 再读入，内容一致；导入的 `TR`、`SQ`、`CR`、`MA`、`LB` 原样出现在 `markup` 里，没有改过的标记保存时原样写回。
3. 字母和数字取下一个的规则有对照 Java 的测试。
4. 交换落子顺序之后，分析请求里的轮走方跟着变（fake-katago 测试）。
5. Tauri 层和 TS API 的测试覆盖每个新命令；`docs/ARCHITECTURE.md` 的 4.3 和 4.7 更新。
