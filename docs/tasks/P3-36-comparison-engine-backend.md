---
id: P3-36
title: 对比引擎的后端：副引擎会话、跟随主引擎分析、双引擎布局、LZ2 读写
depends_on: [P3-33]
paths:
  - crates/app-core/**
  - crates/document/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
  - docs/ARCHITECTURE.md
---

## 目标

P3 第 10 批「对比引擎」（Java 的副引擎和双引擎模式）的后端。用户在双引擎布局里选一个副引擎，它和主引擎分析同一个局面，结果并排显示，方便比较两个引擎或两个模型。界面在 P3-37。

Java 参考只学行为（`../lizzieyzy-next`，路径相对 `src/main/java/featurecat/lizzie/`）：

- 副引擎：`Lizzie.leelaz2`；
  - `analysis/EngineManager.java`：
    - `switchEngine` 12099–12300；
    - `reStartEngine2` 4644–4720；
    - `killThisEngines2` 4803–4813；
    - `killOtherEngines` 4732–4744；
    - `killAllEngines` 4384–4430；
    - `rejectSameEngineSelection` 11570–11578。
- 跟随主引擎：`analysis/Leelaz.java` 12115–12128、12510–12531；`rules/Board.java` 2263–2295。
- 每个节点的副槽：`rules/BoardData.java` 25–148、663–799。
- LZ2 / LZOP2：`rules/SGFParser.java` 127–139、1556–1557、2061–2069、4587–4620；`rules/SgfAnalysisPayloadWriter.java`。
- 双引擎模式的进出：`gui/LizzieFrame.java` 3313–3351。

**和 Java 不同的地方**（决定见 ROADMAP 第 10 批）：

- 副引擎从引擎列表里选，本地或远程的 KataGo 分析引擎都可以；不能和主引擎相同（同 Java）。GTP 引擎不能当副引擎。
- 双引擎布局就是开关：布局是双引擎、并且选了副引擎时运行，否则不运行。
  - 离开双引擎布局时关闭副引擎进程，释放显存。Java 只停分析，进程留着。
- 副引擎收到的分析请求和主引擎完全相同：同一个节点、规则、贴目、选点限制、搜索参数和分析上限。新版用第二个协调器跟随同一个焦点；Java 是把主引擎的 GTP 命令转发过去。
- 副引擎失败不影响主引擎，状态单独报告。

## 要做

1. **选择**
   - `Settings.comparison_engine_id: Option<String>`：旧文件缺省 None，版本不变。必须引用列表里的条目，且不等于 `main_engine_id`。
   - `select_comparison_engine(id: Option<String>) -> Settings`：
     - None 表示不选；
     - 选主引擎返回类型化错误 `SameAsMain`；
     - 副引擎正在运行时换引擎，沿用现有的待定试启动事务：新会话 Ready 后接管，失败保留旧的并返回原因。
   - `select_main_engine(id)` 选到副引擎时返回 `SameAsComparison`（同 Java）。
   - 删除的条目是副引擎时：清除选择并关闭。修改正在运行的副引擎的档案时：按现有事务重启。
   - `restart_comparison_engine()`。
   - `close_comparison_engine()`：关闭进程并清除选择（同 Java 的「关闭当前引擎」）。
   - `close_other_engines()`：关闭主引擎以外的分析引擎，也就是副引擎，同样清除选择。
   - `close_all_engines()`：主引擎按现有 `close_engine` 的语义关闭，副引擎关闭并清除选择。
   - 这些命令都返回完整的 `Settings`。
2. **双引擎布局**
   - `LayoutMode` 增加 `DoubleEngine`，生成 TS。
   - 副引擎在「当前布局是 DoubleEngine，并且 `comparison_engine_id` 有值」时运行：
     - 启动时满足就启动；
     - 布局或选择改变时启动或关闭。
   - 启动沿用现有试启动的期限和调优延长规则。失败报告 `Failed { reason }`，不自动重试；主引擎和文档照常。
   - 运行中崩溃：沿用 engine-session 现有的重启规则，状态照实报告。
3. **分析**
   - 第二个协调器。指纹按副引擎的档案计算，所以内存和持久缓存的命名空间与主引擎自然分开。持久缓存同样读写。
   - 主协调器的每次 `Focus` / `clear_focus` 都同样发给副协调器。以下各项和主引擎相同：
     - pondering 意图（空格暂停时两边一起停）；
     - 分析上限、实时设置；
     - 规则和贴目覆盖；
     - 选点限制、搜索参数。
   - 整盘分析、批量分析、测速、对局和陪练、网页旁观的推送只用主引擎。这些功能清掉主协调器的焦点时，副协调器也清掉。
   - 主引擎没有运行（未配置、失败、关闭）时，副引擎照常按文档的焦点分析。
4. **事件和查询**
   - `comparison_state() -> ComparisonView { engine_id, status, analysis }`：
     - `status`：Stopped / Starting / Analyzing / Idle / Failed { reason } 等，按新版现有的引擎状态类型；
     - `analysis`：当前节点的副引擎结果，没有为 null。
   - 事件：
     - `ComparisonChanged(ComparisonView)`：状态或选择变化时发；
     - `ComparisonAnalysisUpdated(AnalysisView)`：结果更新时发，`node_id` / `revision` / `source` 的规则同主引擎的 `AnalysisUpdated`。
   - 只在双引擎布局时推送副引擎的结果。
5. **LZ2 / LZOP2**
   - 读：打开棋谱时，LZ2 / LZOP2 按 LZ / LZOP 的现有规则解析成副槽的保存结果（来源 `Saved`）。双引擎布局下，当前节点没有副引擎的实时或缓存结果时，推送保存结果（副引擎没有运行时也推，方便看旧棋谱）。
   - `DocumentView` 增加 `has_comparison_analysis: bool`（生成 TS 为可选字段，mock 补默认值）：棋谱里有 LZ2 或 LZOP2。界面用它提示进入双引擎布局（Java 的确认框）。
   - 写：保存时，副引擎在本次会话得到的结果按现有 LZ / LZOP 的写法写成 LZ2 / LZOP2。每个节点保留计算量最高的一份（同 Java）。
   - 其余规则同 LZ：
     - 有规则或贴目覆盖时不导出新结果；
     - 「清除分析」（此手、全局）同样清副槽，并可撤销；
     - 纯净棋谱照旧去掉。
6. **命令授权和类型**
   - 新命令只授权主窗口；生成 TS。
   - mock：`comparison_state` 是查询，不写脚本时回答「未选择、已停止」；ARCHITECTURE 同步。

## 不做

- 界面（P3-37）。
- GTP 引擎当副引擎，以及 Java 在不同引擎之间的命令转换（`lz-*` ↔ `kata-*`、Leela 0.11 的轮询）。
- 胜率图的第二条线、两个引擎的差异标记（Java 也没有）。
- 引擎快速切换（保留旧引擎进程）。
- 副引擎的整盘分析和问题手。

## 验收标准

- 用 fake-katago 跑两个会话：
  - 选副引擎；选和主引擎相同的被拒；把副引擎选为主引擎被拒；
  - 布局切到双引擎时启动，切走时进程退出；重新启动应用时恢复；
  - 两个会话收到相同的查询（节点、规则、贴目、选点限制、搜索参数），导航、暂停、分析上限两边一致；
  - 副引擎启动失败或运行中崩溃时，主引擎分析不受影响，状态是 Failed；重启后恢复；
  - 换副引擎：试启动成功后接管，失败保留旧的；删除条目；修改档案；
  - 关闭副引擎、关闭其他引擎、关闭全部引擎；
  - 缓存分开：副引擎的结果不出现在主引擎的缓存命名空间里，反之亦然；
  - 整盘分析或对局清掉焦点时，副引擎也停。
- LZ2 / LZOP2：
  - 读入后在双引擎布局下作为保存结果推送；
  - 保存后写出，往返一致；
  - 有覆盖时不导出；
  - 清除分析和撤销。
- 新命令只授权主窗口；生成的 TS 类型和 mock 同步。
- `scripts/verify.sh --e2e` 通过；`check_budgets.py` 通过。
