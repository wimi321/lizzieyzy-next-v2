---
id: P3-12
title: AI 陪练（HumanSL）的后端：人类棋风模型、像人一样落子、着手评价、提示和赛后总结
depends_on: [P3-10]
paths:
  - crates/katago-protocol/**
  - crates/engine-session/**
  - crates/analysis/**
  - crates/app-core/**
  - tools/fake-katago/**
  - resources/katago/katago-assets.json
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
  - docs/ARCHITECTURE.md
---

## 目标

P3 第 5 批：AI 陪练。用 KataGo 的人类棋风模型（HumanSL），让 AI 像某个段位的人一样下，并在对局中评价用户的着手。Java 参考在 `../lizzieyzy-next`，只学行为：

- `gui/NewHumanSlGameDialog.java`、`gui/CoachSetupForm.java`、`gui/HumanSlGameController.java`、`gui/CoachPanel.java`、`gui/CoachPanelState.java`、`gui/CoachAssist.java`、`gui/MoveReviewer.java`；
- `analysis/HumanSlAnalysisRunner.java`、`analysis/HumanPolicySampler.java`、`analysis/HumanLikeMoveSelector.java`；
- `training/` 下的 `HumanSlTrainingConfig`、`OpponentPreset`、`HumanThinkTime`、`AiResignation`、`RankAwareWinrate`、`PositionReview`、`MoveVerdict`、`MoveFeedback`、`GameResult`；
- `util/HumanSlModelDownload.java`，以及 `util/KataGoAutoSetupHelper.java` 里有关人类棋风模型的部分。

陪练是 P3-09 对局会话的一种：AI 一方是 HumanSL 玩家，再加上评价、提示和总结。

**和 Java 不同的地方（维护者决定）**

- Java 为陪练另起一个 KataGo 进程，并暂停主引擎来腾出显卡。新版只有一个引擎所有者：主分析引擎在启动参数里加上 `-human-model`，陪练的查询带 `humanSLProfile`，普通分析的查询不带，所以普通分析的结果不变。
- 以下 Java 问题不照搬：
  - AI 选到不合法的点时会变成 pass；
  - 19 路以外的让子不摆子；
  - 「结束对局」不确认、不记结果；
  - 只按大小（> 1 MB）就接受导入的模型。

## 要做

1. **人类棋风模型。**
   - **目录条目**：`resources/katago/katago-assets.json` 加一个 HumanSL 模型条目，`minimumKataGoVersion` 是 1.15.0，写明这是人类棋风模型，不能当主模型选：
     - 文件名 `b18c384nbt-humanv0.bin.gz`；
     - 大小 99,066,230 字节；
     - SHA-256 `637746e44f0efe00ad1245a50aa9bbf0716efe364c43965ead97bd6835d84ab5`；
     - 下载地址 `https://media.katagotraining.org/uploaded/networks/models_extra/b18c384nbt-humanv0.bin.gz`。
   - **下载**：复用 P2-12 的权重下载，包括断点续传、暂停和取消、校验大小和 SHA-256、进度事件，下载源的规则也与之相同。存放位置与其他模型相同。
   - **导入本地文件**：只接受 SHA-256 一致的文件。
   - **新命令 `humansl_model_status()`**：返回缺失、下载中（已下 / 总共）、就绪（路径）或损坏。
2. **加载到主引擎。**
   - `EngineProfile` 新增可选的 `human_model`。设置了它，引擎启动时就加 `-human-model <路径>`；它也计入引擎指纹。
   - 开始陪练时，如果主引擎还没加载人类模型，就设置它并重启主引擎，重启进度用现有的引擎状态事件报告。以后一直加载，用户可以在引擎设置里去掉。
   - 引擎版本低于 1.15.0，或主引擎是 TensorRT 版（Java 为此换用 CUDA 版，新版先不支持），返回类型化错误，不重启。
   - `katago-protocol` 解析以下字段，都是可选的：顶层的 `humanPolicy`、候选点的 `humanPrior`、根节点的 `humanWinrate` / `humanScoreMean` / `humanScoreStdev`。查一下自带的 `analysis.cfg`，确认它没有设置 `humanSLProfile`。
3. **陪练设置 `CoachSetup`。** 开始成功后保存，下次回显。
   - 对手棋力：20 级到 1 级、1 段到 9 段，共 29 档，默认 3 段。对应 `rank_20k` … `rank_9d`。
   - 训练方式：
     - 边下边提醒（默认）：不显示实时分析，有着手评价和提示；
     - 实战后复盘：不显示实时分析，也不评价；
     - 边下边分析：照常实时分析，不评价。
   - 执黑、执白、随机（默认随机）。
   - AI 思考节奏：自然（默认）、快、慢。
   - 让子 0–9：让子时用户执黑，用 document 的固定让子，9/13/19 路都摆。贴目在让子时为 0.5，否则 7.5。
   - 从当前局面开始：默认关，不保存，只在棋盘上有棋子时可用；规则同 P3-09 的续弈。
4. **AI 怎么落子。** 照 Java，随机数可以注入种子：
   - **查询设置**：
     - 每个 HumanSL 查询都带 `includePolicy` 和 `overrideSettings` 里的 `humanSLProfile`、`ignorePreRootHistory=false`、`rootNumSymmetriesToSample=1`；
     - 还有 `humanSLRootExploreProbWeightless`：7 段以上为 0.8，其他为 0.5；
     - 每手最多 10 秒。
   - **5 段及以下**：
     - 用 1 次计算的查询取得 `humanPolicy`，直接按它抽样。
     - 温度为 0.70 + 0.15 × 0.5^(手数/80 × 19/√面积)；只对概率低于 1% 的点降温。
     - 抽到被占或不合法的点，就去掉它再抽，不变成 pass。
   - **6–9 段**：
     - 候选是人类概率最高的点：最多 12 个，每个至少是最高者的 1%，累计到 95% 为止，再加上 KataGo 的首选和 policy 的最高点。
     - 用 `allowMoves` 搜索候选（初始 64 次计算），权重为 p^(1/T) × exp(−损失/最大损失)。T 是：60 手前 1.25，160 手前 1.05，之后 0.90。
     - 损失和最大损失按 Java 的段位分档。在时间之内最多加深 4 轮、4096 次计算：7–9 段总是加深，6 段只在结果不稳时加深。
   - **虚手和认输**：
     - 下满面积的 40% 以后，或对手刚虚手时，做一次不带 `humanSLProfile` 的 40 次计算的查询；
     - KataGo 的首选是 pass，或对手刚虚手且虚手最多亏 1 目时，AI 虚手；
     - 人类 policy 一半以上在 pass 上、而 KataGo 不选 pass 时，落 KataGo 的首选；
     - AI 认输：至少下满 40% 面积。级位和 1 段：连续 20 手胜率 ≤ 0.5% 且落后 ≥ 40 目；9 段：连续 10 手胜率 ≤ 1% 且落后 ≥ 20 目；其间按段位插值。
   - **思考节奏**：AI 的着法不早于节奏延时出现，用 tokio 的时间。
     - 基础延时：前 10% 的手数 1.0 秒，之后 1.6 秒；
     - 乘以 (0.6 + 2.2 × (1 − 首选占比)^1.5)，再乘一个 0.75–1.35 的随机系数；
     - 限制范围：自然 0.4–8 秒；快的系数为 0.4，范围 0.25–1.8 秒；慢的系数为 2，范围 0.8–14 秒。
5. **着手评价**：只在「边下边提醒」时做。
   - **分析**：在用户思考时，用 HumanSL 分析当前局面，400 次计算或 1.5 秒，结果缓存。
   - **下完后的目差**：用户着手的候选点计算量 ≥ max(16, 首选的 1/20) 时，取它自己的目差；否则对下完后的局面做 200 次计算或 0.8 秒的查询，取 −rootLead。
   - **段位胜率**：Φ(目差 / max(1, `humanScoreStdev`))；没有 `humanScoreStdev` 时标准差用 12。
   - **等级**：按段位胜率的下降，取整到百分之一：
     - ≥ 5% 为缓手，≥ 10% 为问题手，≥ 20% 为恶手；
     - 下之前段位胜率 < 5% 或 > 95% 时，再按亏的目数：≥ 10 目为问题手，≥ 20 目为恶手，只升不降；
     - 妙手：是 KataGo 的首选，这个段位不到 10% 的人会下，而他们常下的那手亏 ≥ 5%。
   - **新事件 `CoachMoveRated`**：手数、等级、胜率变化、亏的目数、这个段位下这手的人的比例。
   - **恶手等待**：用户下出恶手时，AI 的应手最多等评价 4 秒，然后先不落，等用户 `coach_decision(..)` 选择：
     - `Retry`：退回这手，计入一次悔棋；
     - `ShowBest`：返回 KataGo 的推荐点，以及这个段位下这手的人的比例；
     - `Continue`：AI 落下它已经想好的应手。
6. **提示 `coach_hint()`。** 只在边下边提醒、轮到用户时可用。
   - 第一次给出 KataGo 首选所在的区域（棋盘分 9 块），第二次给出准确的点，以及这个段位下这手的人的比例。分析还没好时，返回「正在读局面」的状态。
   - 每个局面最多计一次提示。
7. **结束和总结。**
   - 数子、认输和保存沿用 P3-09。`stop_game` 也写明原因（「中途结束」）。
   - **新事件 `CoachSummary`**：结果、总手数、恶手数、提示次数、悔棋次数，以及亏得最多的 3 手（节点 id、手数、等级、亏损）。
   - `coach_review()`：结束陪练，在最终局面恢复正常的实时分析。
8. **fake-katago。** 加载了 `-human-model` 参数时，按 `humanSLProfile` 给出确定的 `humanPolicy`、`humanPrior` 和 `humanScoreStdev`，支持 `allowMoves`，可以按脚本指定。
9. 新字段和新命令照约定：生成的 TS 里可选，mock 补默认值，mock 里是脚本化的 `command`。

## 不做

- 棋力评估用的 HumanSL：另排。
- TensorRT 版加载人类模型：等第 2 批在真机上验证。
- 界面：P3-13。

## 验收标准

1. 模型：目录条目、下载校验（大小或 SHA 不符时删掉重下）、导入时 SHA 不符被拒绝、四种状态，各有测试。
2. 加载：设置 `human_model` 后启动参数带上 `-human-model`；引擎指纹随之改变；版本过低和 TensorRT 时的错误；普通分析的查询不带 `humanSLProfile`。
3. 落子（用 fake-katago 和固定种子）：
   - 5 段以下按温度抽样，不合法的点重抽；
   - 6–9 段的候选池和权重；
   - 虚手和认输规则；
   - 节奏延时的范围，用 tokio 的暂停时间。
4. 评价：
   - 段位胜率的公式和没有标准差时的退路；
   - 四种等级和「只升不降」；
   - 妙手；
   - 恶手等待的三种选择，以及 Retry 计入悔棋。
5. 提示的两步和计数；总结的各项和最差 3 手；`coach_review` 恢复实时分析。
6. 三种训练方式下，实时分析是否运行与设置一致。
7. `scripts/verify.sh --e2e` 通过；新依赖每个写一行理由；`docs/ARCHITECTURE.md` 更新。
