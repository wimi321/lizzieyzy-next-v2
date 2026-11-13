---
id: P3-04
title: 批量分析与精析的后端：分析范围（局部、全部分支）、精析设置、多棋谱批量分析队列
depends_on: [P3-03]
paths:
  - crates/document/**
  - crates/analysis/**
  - crates/app-core/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
  - docs/ARCHITECTURE.md
---

## 目标

P3 第 3 批：Java 重度用户的复盘方式，一次分析一批棋谱、只分析一段、分析所有分支。Java 参考在 `../lizzieyzy-next`，只学行为：

- `gui/Menu.java` 的「分析」菜单：批量分析（Ctrl+O）、批量精析、停止自动（批量）分析、批量分析进度表、局部精析、全部分支精析、精析设置；
- `LizzieFrame` 的 `openFileWithAna`、`openOrdinaryBatchAnalysis`、`flashAutoAnaSaveAndLoad`、`flashAnalyzeGame`、`flashAnalyzePart`；
- `gui/AnalysisTable.java`（进度表）、`gui/AnalysisPartGame.java`（手数范围）、`gui/AnalysisSettings.java`（精析设置）、`gui/StartAnaDialog.java`。

新版只有一种分析方式（KataGo 分析引擎），所以 Java 的「普通批量」和「闪电批量」合成一个队列，计算量由调用方给出。Java 的独立闪电分析引擎（引擎命令、复用前台引擎、预加载、分析后退出）不迁移。

## 要做

1. **整盘分析的范围。** `WholeGameOptions` 新增两个字段，旧调用方不传时行为不变：
   - `scope`：
     - `Line`（默认）：现在的行为，从根到当前，再沿第一个后续走到底；
     - `Range { first_move, last_move }`：同一条线上的一段，两端都可省略，省略即从第一手、到最后一手；起止超出这条线或起大于止时，返回类型化错误；
     - `AllBranches`：棋谱树里的所有节点，根在前，深度优先。
   - `force`：为真时，已有结果（包括棋谱里保存的分析）的计算量即使够了也重新分析，结果照常替换（照 Java 的「总是覆盖已有分析结果」）。
   - 进度的 `total` 按范围内的节点数算。问题手、棋力评估这些读整盘结果的地方，在 `AllBranches` 下照旧按主线读，不要因多出的分支节点出错，写测试。
   - 计算量的合法范围放宽为 1–1,000,000：Java 的精析允许任意单步计算量。现有预设不变，更新相应的测试。
2. **精析设置。** 设置里新增 `flash_analysis { visits, always_override }`，默认 `visits = 1`、`always_override = false`（Java 的默认值）。有读写命令，照 P2 的约定：先原子保存再生效，失败保留旧值。规则沿用引擎设置里的规则，不另设。
3. **批量分析队列。** 由主 actor 持有，不另起线程。
   - 开始：`start_batch_analysis(BatchOptions { files, visits, scope, force, output })`。`scope` 只接受 `Line`、`Range`、`AllBranches`，`Range` 用于每一盘。`output` 包括输出文件名里的后缀文字（例如「已分析」，由界面按语言给出；校验：不能为空，不含路径分隔符或控制字符，长度有上限）和可选的输出文件夹（不给就放在原棋谱旁边）。
   - 逐盘执行：
     1. 用打开文件的同一条路径打开这一盘（SGF、GIB、编码识别、棋谱里保存的分析都照旧），在主窗口显示；
     2. 按选项做整盘分析；
     3. 完成后存为 `<原文件名去扩展名>_<后缀>_<yyyyMMddHHmmss>.sgf`，时间用本地时间，照 Java；用和「保存」相同的写法，包括 LZ 分析数据；绝不覆盖已有文件，同名时加 `-2`、`-3`……；原棋谱不动；
     4. 接着下一盘。最后一盘结束后，它留在窗口里。
   - 批量打开的棋谱不进「最近打开」，输出文件也不进。
   - 某一盘打不开、分析出错或保存失败时，这一盘记为失败（带原因），接着下一盘。引擎不可用时整个批量停下，状态说明原因。
   - 运行中或暂停时，替换或修改当前棋谱的命令一律返回类型化错误 `BatchAnalysisRunning`：打开、粘贴、新对局、野狐打开、从系统打开、编辑。翻手和显示可以照常用。已有的自动胜率曲线、整盘分析入口同样返回这个错误。
   - 控制命令：
     - 暂停、继续：暂停当前这一盘的整盘分析；
     - 停止：取消当前这一盘，它和后面的都回到「等待」，可以再开始；
     - 加入文件；
     - 删除一个等待中的项目；
     - 上移、下移、优先（移到等待项的最前面）；
     - 清空：清空所有已结束和等待中的项目，正在跑的那一盘除外。
   - 状态快照 `BatchAnalysisStatus`：整体阶段（空闲 / 运行 / 暂停 / 停止 / 完成）；每一项的路径、状态（等待 / 运行中及进度 / 完成及输出路径 / 失败及原因）；当前是第几项。用有界最新槽事件 `BatchAnalysisProgress` 推送，加单独的 `subscribeBatchAnalysisProgress`，并提供 `batch_analysis_status` 查询。
   - 队列不跨重启保存，应用关闭时停止。
4. 新字段、新命令照约定：生成的 TS 里可选，mock 补默认值；mock 里是脚本化的 `command`，只读的查询用带默认值的 `query`，进度可脚本化。

## 不做

- 界面：P3-05。
- Java 的逐手自动分析细项（每手时间、首选计算量、只分析黑棋或白棋、波动过大时加强分析），以及把胜率和问题手写进评论。
- 独立的闪电分析引擎，以及精析专用的规则。

## 验收标准

1. 范围：`Range` 的各种端点（省略、越界、起大于止）、`AllBranches` 的节点顺序和数量、`force` 重算已有的和保存的结果，各有测试（fake-katago）。旧调用方不传新字段时，行为和现在一样。
2. 精析设置的默认值、读写、失败时保留旧值，有测试。
3. 批量：用 fake-katago 和临时目录测试：
   - 三盘依次分析并保存，输出文件名和内容（含 LZ）正确，原文件不变；
   - 同名时不覆盖；
   - 中间一盘打不开或保存失败时，记为失败并继续；
   - 暂停、继续、停止、再开始；
   - 运行中加入、删除、上移、下移、优先、清空；
   - 运行中打开或编辑棋谱返回 `BatchAnalysisRunning`；
   - 引擎失败时整个批量停下。
   测试不依赖真实时间：时间戳的时钟可以注入。
4. `scripts/verify.sh --e2e` 通过；新依赖每个写一行理由；`docs/ARCHITECTURE.md` 更新。
