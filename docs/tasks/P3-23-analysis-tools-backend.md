---
id: P3-23
title: 研究工具的后端：选点限制、搜索参数、自动补完征子、终局点目
depends_on: [P3-22]
paths:
  - crates/go-core/**
  - crates/katago-protocol/**
  - crates/analysis/**
  - crates/document/**
  - crates/app-core/**
  - tools/fake-katago/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
  - fixtures/**
  - docs/ARCHITECTURE.md
---

## 目标

P3 第 7 批「研究工具」的分析部分。界面在 P3-25。Java 参考只学行为（`../lizzieyzy-next`，路径相对 `src/main/java/featurecat/lizzie/`）：

- 选点限制：`gui/Menu.java` 8291–8662（顶栏）、`gui/RightClickMenu.java`、`analysis/Leelaz.java` 22896–23030；
- 自动补完征子：`rules/Board.java` 7213–7252；
- 终局点目：`rules/Board.java` 6962–7185、`gui/ScoreResult.java`。

**和 Java 不同的地方**（维护者的决定见 ROADMAP 第 7 批）：

- 选点限制只作用于**轮走方**：KataGo 的 `allowMoves` 只能有一项。Java 同时限制双方，主变化因此没有意义。
- 限制一直有效，直到清除或换了棋局。不区分「右键只当前局面、顶栏跨局面」两种寿命。
- 征子按气真正读出来，不做 Java 的几何外推。
- 终局点目的结果用标准格式写进 RE（`B+3.5` / `W+3.5` / `0`），不写本地化文字。

## 要做

1. **选点限制**
   - 状态：`AnalysisRestriction { kind: Allow | Avoid, points, depth }`。`Allow` 和 `Avoid` 互斥，设置一种就清掉另一种。状态属于当前文档：打开或新建棋局时清空，不进 SGF，不进设置。
   - 深度保存在设置里，每种各一个：「仅第一手」（1）、「自定义 N」（1–200，默认 5）、「整个搜索」（用一个足够大的值，写明取值和理由）。
   - 命令：
     - `set_analysis_restriction(kind, points)`：替换整个点集；
     - `add_restriction_points(kind, points)` 和 `remove_restriction_points(points)`，供右键「只分析此点 / 增加分析此点 / 去除分析此点 / 不分析此点」；
     - `clear_analysis_restriction()`；
     - `set_restriction_depth(kind, depth)`。
     界面需要的状态通过视图或事件给出（选哪种由你定），必须包含点集、种类和深度。
   - 下发：只有**实时分析**带限制，在查询里写 `allowMoves` 或 `avoidMoves`：`[{ "player": <轮走方>, "moves": [...], "untilDepth": N }]`。在 `katago-protocol` 里把这两个字段做成类型字段，不再走 `extensions`。陪练（`actor/coach/play.rs`）改用类型字段，行为不变。
   - **受限分析只用于显示**：
     - 不读也不写持久分析缓存；
     - 不作为节点保存的分析，所以不进胜率图、问题手、整盘分析和批量分析，也不写进 SGF；
     - 整盘分析和批量分析不受限制影响。
     设置、增删或清除限制时，当前局面立即重新分析；清除限制不能让缓存或已保存的分析失效。
   - 人机对局、引擎对局和陪练进行中，拒绝设置限制并返回类型化错误。
   - 格式以 KataGo 的 Analysis Engine 文档为准（可以联网只读查看；仓库里只有 `allowMoves` 的样例）。`tools/fake-katago` 支持 `avoidMoves`（目前只发警告），并让两种限制都按 `player` 和 `untilDepth` 在根节点生效，供测试验证。
2. **搜索参数**（Java「设置 → 引擎参数」，顶栏的 PDA / WRN 输入框）
   - 设置里加 `SearchParams { playout_doubling_advantage, wide_root_noise, root_policy_temperature }`，可选，默认不设（不发）。
   - 有值时，**每个**分析查询都在 `overrideSettings` 里带上对应的 KataGo 字段，包括实时、整盘、批量、对局、陪练。不重启引擎。
   - 取值范围对照 KataGo 文档，越界返回类型化错误。
   - 有值时，这些参数参与分析缓存和结果的身份，和默认参数的结果互不混用。
   - 命令：`search_params()`、`set_search_params(params)`（`None` 表示恢复默认）。改了以后当前局面立即重新分析。
3. **自动补完征子**（`go-core` 纯函数加 app-core 命令）
   - `go-core`：给定局面和轮走方，判断是否正在征子，并读出结果。
     - 被征的一方：刚被叫吃（1 气）且轮到它长，或刚长出、只有 2 气且轮到征方叫吃。
     - 征方每步在两口气里选能把对方压回 1 气的那一点，并考虑对方长出时吃掉征方某块叫吃之子的反击。
     - 被征方长出后有 3 气以上，或者吃掉了征方的子，就是逃出；被征方被吃，或者长出后仍只有 1 气，就是征死。
     - 返回着手序列和结果：征死、逃出、不是征子。
     - 长度以盘面为限，写清停止条件。
   - 命令 `continue_ladder()`：
     - 从当前节点按读出的序列追加着手，到征死前的最后一次叫吃（不下最后的提子），或者到逃出为止；
     - 当前节点已有不同的后续时成为新分支，相同的后续沿用已有子节点；
     - 整个序列是一次可撤销的编辑；
     - 返回结果种类和追加的手数；
     - 不是征子时返回类型化错误，不改棋谱。
   - 用 golden 局面加 proptest 测试：征死、在引征子处逃出、吃子逃出、靠盘边、不是征子，以及对局不变量（每一步都合法）。
4. **终局点目**
   - `go-core` 纯函数：输入局面、死子集合、无目点集合、计分规则（数子 / 数目）、贴目和让子数，输出：
     - 每个交叉点的归属；
     - 黑白双方的算式明细：活子、地、死子、提子、贴目、让子补偿；
     - 结果。
   - 规则：
     - 数子：子加地，让子补偿照棋谱规则（中国规则补 N、AGA 补 N−1）；
     - 数目：地加提子（含死子），不补让子；
     - 平局结果为 `0`。
   - 命令：
     - `start_scoring()`：进入点目，暂停实时分析，返回视图。规则默认跟随棋谱规则。当前节点的分析有 ownership 时，按黑方视角的 ownership 预标死子：整块棋的平均归属明显属于对方（阈值写进文档）；没有就全部算活。
     - `toggle_scoring_point(point)`：点子翻转**这一整块**（相连的同色子）的死活；点空点切换无目点。
     - `set_scoring_rule(rule)`。
     - `commit_scoring()`：把结果写进 RE，作为一次可撤销的编辑，然后退出点目，恢复之前的分析状态。
     - `cancel_scoring()`：退出，不改棋谱。
   - 点目时棋谱被改动（跳转、编辑、换棋局），自动退出点目。
   - 对局、引擎对局、陪练进行中，拒绝进入点目，返回类型化错误。
5. 新的设置和视图字段在生成的 TS 里可选，mock 补默认值；新命令在 mock 里是脚本化的 `command`。

## 不做

- 界面（P3-25）。
- Java 的「持续分析 / 不分析」开关：新版的限制本来就持续到清除为止。
- 让限制作用于整盘分析或批量分析。
- 点目窗口的本地化文字。界面拿明细自己排版。

## 验收标准

1. 选点限制：
   - 两种限制的查询格式（轮走方、深度）有测试；
   - 受限结果不进缓存、不进节点分析、不进 SGF，清除后缓存和已保存的分析都还在，有测试；
   - 打开新棋局时清空、对局中拒绝，有测试；
   - fake-katago 的 avoidMoves 有测试。
2. 搜索参数：每类查询都带 `overrideSettings`，有测试；越界拒绝，有测试；结果按参数隔离，有测试。
3. 征子：golden 和 proptest 覆盖上面列的每种情况；`continue_ladder` 的分支、沿用已有子节点、一次撤销、不是征子时不改棋谱，各有测试。
4. 点目：两种规则、让子补偿、平局、整块翻转、无目点、ownership 预标、写 RE、撤销、棋谱改动时退出、对局中拒绝，各有测试。
5. `scripts/verify.sh --e2e` 通过；`docs/ARCHITECTURE.md` 写明限制只作用于显示、搜索参数的缓存隔离、点目的计分公式。
