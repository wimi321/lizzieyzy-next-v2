---
id: P3-06
title: 形势、热点图与工具栏自定义的后端：显示设置、按需的 policy 和按选点 ownership、纯网络
depends_on: [P3-05]
paths:
  - crates/katago-protocol/**
  - crates/engine-session/**
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

P3 第 4 批：Java 用户常用的显示细节。Java 参考在 `../lizzieyzy-next`，只学行为：

- `gui/Menu.java` 的「显示 → KataGo相关设置 → Kata形势显示」及其子菜单「在纯网络中显示」；
- 「显示 → 小棋盘设置」：热点图、变化图、仅棋盘棋子；
- 「显示 → 工具栏」：顶部工具栏、底部工具栏和它们的「自定义内容」；
- 「分析 → 纯网络」「分析 → 策略网络」；
- `LizzieFrame.togglePolicy`、`Leelaz.toggleHeatmap`，以及 `Config` 里对应的默认值。

本卡只做后端：显示设置的保存，按需请求 policy 和每个选点的 ownership，以及纯网络。画法由界面卡 P3-07 负责。

## 要做

1. **显示设置新增字段。** 在 `DisplaySettings` 里新增以下字段，照 P2-06 的约定：读写走现有的 `set_display_settings`，生成的 TS 里可选，旧设置文件补默认值，非法值返回类型化错误。默认值照 Java。
   - `estimate_location`：`Off`（默认）、`Hidden`（照常请求但不画）、`Main`、`Sub`、`Both`。
   - `estimate_style` 和 `pure_estimate_style`：都是同一个枚举，有五种画法：
     - `TransparentSmall`：小方块，透明度表示占有率。`estimate_style` 默认用它。
     - `Transparent`：方块，透明度表示占有率。
     - `TransparentOffStones`：同上，但不画在活子上。
     - `BigSquare`：棋子下画大方块。`pure_estimate_style` 默认用它。
     - `BySize`：方块大小表示占有率。
   - `remember_estimate`（默认 false）：为假时，启动后形势显示回到 `Off`。这一条由后端在加载设置时执行。
   - `moves_ownership`（默认 true）和 `estimate_shortcut`（默认 false）。
   - `sub_board_heat`：`None`（默认）、`Policy`（第一感）、`AfterSearch`（计算后）。
   - `sub_board_raw`（默认 false）：小棋盘只显示棋盘上的棋子，不显示变化。
   - 工具栏：
     - `top_toolbar_layout`：`Separate`（默认）或 `Combined`（合并在菜单栏上）。显示与隐藏仍用现有的 `show_top_toolbar`。
     - `top_toolbar_wrap`（默认 false）。
     - `top_toolbar_hidden` 和 `bottom_toolbar_hidden`：要隐藏的项目 ID 列表，默认为空。这些 ID 由界面定义，后端只校验格式：最多 64 项，每项 1–64 个字符，只能是字母、数字、`-` 和 `_`。保存时去掉重复的项。
2. **按需的 policy 和按选点 ownership。**
   - 新增命令 `set_analysis_extras(AnalysisExtras { policy, moves_ownership })`。这是运行时状态，不保存，界面按显示设置开关它。打开时，实时分析的查询带上 `includePolicy` / `includeMovesOwnership`；关闭时不带，以减少数据量。
   - `AnalysisView` 新增 `policy`：可选，长度为 宽×高+1（最后一项是 pass），是轮到的一方的落子概率，非法点为 -1，和 KataGo 给的一样。policy 不是胜负值，不换成黑方视角，这一点写进 `ARCHITECTURE.md`。
   - 每个选点新增可选的 `ownership`：长度为 宽×高，换成黑方视角，和根节点的 ownership 一样。
   - 当前局面的结果缺少已打开的附加数据时（例如分析已暂停或已达上限，或者结果来自缓存），后端补一个 1 次计算的查询来取得它，不改变暂停 / 继续的状态。
   - 持久缓存和整盘分析不必保存这两项。读回不带这两项的旧结果不能出错。
3. **纯网络。**
   - 新增命令 `set_pure_network(bool)`，状态反映在 `AnalysisState` 里（新字段可选）。
   - 打开后，显示中的局面改用 1 次计算的查询，带 policy 和 ownership，取得神经网络的原始输出：胜率、目差、policy、形势。翻到别的局面，就对新局面再查一次。
   - 关闭后，回到打开前的实时分析状态：原来在分析就继续分析，原来暂停就保持暂停。
   - 整盘分析或批量分析进行中时，返回类型化错误，不打开。
   - 不保存，重启后是关闭的。
4. **fake-katago**：请求里带 `includePolicy` / `includeMovesOwnership` 时，输出对应的字段，用来写测试。
5. 新字段和新命令照约定：生成的 TS 里可选，mock 补默认值，mock 里是脚本化的 `command`。

## 不做

- 界面和所有画法：P3-07。
- 浮动棋盘、布局模式、放大小棋盘和胜率图、恢复默认面板尺寸：功能清单定为 P3 后段，另排。
- 把 Java 工具栏里各项分组的含义搬进后端：项目 ID 归界面管。

## 验收标准

1. 显示设置：每个新字段的默认值、读写、非法值报错、旧设置文件补默认值、`remember_estimate` 为假时启动后回到 `Off`、工具栏 ID 的格式校验和去重，各有测试。
2. 附加数据（用 fake-katago 测）：
   - 打开时查询带上对应参数，关闭时不带；
   - policy 的长度和非法点的值正确；选点 ownership 是黑方视角（白棋走时要反过来）；
   - 暂停时打开附加数据，用 1 次计算补齐，且仍是暂停状态；
   - 读回没有这两项的缓存结果不出错。
3. 纯网络（用 fake-katago 测）：打开后用 1 次计算的查询；翻手后查新局面；关闭后恢复原来的分析状态；整盘分析或批量分析进行中时返回错误。
4. `scripts/verify.sh --e2e` 通过；新依赖每个写一行理由；`docs/ARCHITECTURE.md` 更新。
