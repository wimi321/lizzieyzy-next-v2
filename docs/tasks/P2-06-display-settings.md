---
id: P2-06
title: 「显示」菜单（一）后端：显示设置的保存、每颗子的手数和来源
depends_on: [P2-04]
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

P2 第 2 批「显示」菜单的后端。菜单里的开关要记住，重启后还在；棋盘上的手数和每一手的评价标记要知道每颗子是哪一手下的。界面由 Claude 另做，这张卡不碰 `apps/desktop/src` 里 `api/` 以外的文件。

Java 参考在 `../lizzieyzy-next`，只学行为。设置项的默认值和取值范围照 `Config.java`（读写在 `uiConfig` 的 `optBoolean`、`optInt` 一带）；维护者自己的 Java 配置里有几项和默认不同，以维护者的为准：`show-coordinates` 为 true，`show-move-number` 为 false，`only-last-move-number` 为 1，`limit-max-suggestion` 为 10，`show-next-moves` 为 true，`show-suggestion-maxred` 为 false。每一项参考了哪个配置键、默认值是多少，写进汇报。

## 要做

1. **显示设置。** `Settings` 增加 `display`，至少包括下面这些，名字你定，含义照 Java：
   - 坐标；
   - 手数：不显示、最后 1 手、最后 5 手、最后 10 手、全部、最后 N 手（N 可设）；手数从 1 开始；分支内手数从 1 开始、分支内接着主线编号；分支里显示全部手数；
   - 落子评价标记：不显示、最后一手、最后 N 手、全部；
   - 候选点信息：胜率、计算量、目数三行各自开关；目数显示为与最佳点之差（`showScoreAsDiff`）；候选点序号；总是显示黑方胜率；鼠标悬停显示变化图；
   - 棋谱的下一手提示：不显示、简单、带胜率目差，以及触发的最少计算量；
   - 面板：胜率图、评论/问题手、分支面板、信息面板、状态栏；顶部工具栏、底部工具栏；
   - 胜率图：只画胜率、只画目差、两者都画；填充；柱状失误条；
   - 目数是否带贴目（`kataScolreLead`）；
   - 主界面设置里的两个确认对话框开关（新的棋盘、覆盖文件）；棋盘下方显示名字。
   - 读取 `settings()` 时带上 `display`；新命令 `set_display_settings(display)` 整体替换并保存，返回保存后的值。不合法的值（N 为 0 或超出范围）报错，原设置不变。
   - 旧的设置文件没有 `display` 时用上面的默认值；设置版本照现有规则处理。
2. **每颗子的手数和来源。**
   - `BoardView` 增加两个和 `stones` 同序的数组：下出这颗子的那一手的手数，以及那一手的节点 ID；摆子（AB/AW）和空点为 null。
   - 手数照 `GraphView` 的口径（从根数起的着手数，含停一手）。
   - `TreeView` 或 `DocumentView` 增加当前这条路离开主线的那一手的手数（在主线上时为 null），界面用它算「分支内手数从 1 开始」。
   - 这些数据在回放局面时顺带得到，不能为了它们多做一遍和棋谱长度成正比的工作；变化图预览的 `BoardView` 也要带上（预览里的子用预览里的手数）。
3. 新字段照约定：生成的 TS 里可选，`e2e/mock-backend.ts` 补默认值（`display` 用默认设置，两个数组按 `stones` 的长度补 null）；`set_display_settings` 在 mock 里是脚本化的 `command`。

## 不做

- 超级鹰眼：后面的卡。
- 界面。

## 验收标准

1. `display` 的默认值有测试，逐项对照维护者的配置和 Java 的默认值。
2. 保存、重启后读回、旧文件补默认值、非法值报错不改原设置，都有测试。
3. 每颗子的手数和节点：有提子、打劫、摆子、分支、停一手的棋谱各一条测试；变化图预览的数组也有测试。
4. Tauri 层和 TS API 的测试覆盖新命令；`docs/ARCHITECTURE.md` 的 4.3 和 4.7 更新。
