---
id: P3-09
title: 人机对局的后端：对局会话、AI 用时与认输、读秒、终局数子、人机续弈、让 AI 落子
depends_on: [P3-08]
paths:
  - crates/document/**
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

P3 第 5 批的第一张卡：和 KataGo 下棋。Java 参考在 `../lizzieyzy-next`，只学行为：

- `gui/NewGameDialog.java`（人机对局）、`gui/NewAnaGameDialog.java`（人机对局，分析模式）；
- `LizzieFrame.startNewGame`、`stopAiPlayingAndPolicy`，以及人机续弈和让AI落子最佳一手（`LizzieFrame.java` 13283–13406、9690 附近）；
- `gui/SetAiTimes.java`；`Leelaz.notifyAutoPlay` 里的认输和随机开局；`Config` 里对应的默认值。

**和 Java 不同的地方（维护者决定）**

- 新版的 KataGo 一直以分析引擎（JSON 协议）运行，所以和 KataGo 下棋也走这个已经在运行的引擎：AI 的一手来自一次有时间或计算量上限的查询。不另起 GTP 进程，显卡上也不加载第二份模型。
- Java 的「Genmove 模式」和「分析模式」在新版合成一种。GTP 引擎（第三方引擎）由 P3-10 加进同一套对局会话，所以玩家类型要设计成可扩展的枚举。
- Java 的以下问题不照搬：
  - 对局结束时没有结果：没有终局数子，AI 认输时有时只写结果、有时只结束对局；
  - 续弈会丢掉分支和注释；
  - 「修改 AI 用时」会把 AI 的时钟重置为满时；
  - 随机开局的筛选写错了。

## 要做

1. **对局设置 `GameSetup`。** 设置保存在 settings 里，下次打开时回显，人机续弈也用它。默认值照 Java。
   - **黑方和白方**：各是 `Human` 或 `Ai(AiPlayer)`。本卡只有 `AiPlayer::KataGo(KataGoPlay)` 一种，P3-10 再加 GTP 引擎。
   - **开局方式**：
     - 新棋盘：沿用 `NewGameOptions`，即尺寸、贴目、规则、让子（固定让子 2–9，9/13/19 路都按 document 已有的星位摆）和对局者名字；
     - 当前局面续弈：从当前节点接着下。当前节点已有后续时，新的着法放进一个新分支。原有的树、分支和注释都保留。
   - **`KataGoPlay`**：
     - 每手的上限：`seconds`（默认 2 秒）、`visits`、`pure_network`（只用 1 次计算），至少设一项；
     - 认输规则 `ResignRule { after_moves: 150, consecutive: 2, below_winrate: 0.05 }`，可以关掉；
     - 随机开局 `RandomOpening { within_moves: 20, max_winrate_drop: 0.005, min_visit_share: 0.05 }`，默认关：在前 N 手内，从胜率不低于首选减 Δ、计算量不低于首选的给定比例的候选点里随机选一个。
   - **人的用时 `HumanClock`**：默认关；打开后默认保留时间 20 分钟、读秒 30 秒 × 3 次。保留时间和读秒不能都为 0。
   - **`ponder`**（对局时后台计算，默认开）：轮到人时，实时分析是否继续。
   - **`auto_save`**（默认开）：对局结束后自动保存。保存目录是新设置项 `games_directory`，默认是「文稿」下的 `LizzieYzy Next 2/MyGames`。文件名是 `yyyy-MM-dd-HHmmss_<黑方>_vs_<白方>.sgf`，名字里的非法字符要替换掉。
2. **对局会话**，由 app actor 持有，同一时刻最多一局。
   - **新命令**：
     - `start_game(GameSetup)`、`pause_game()`、`resume_game()`、`stop_game()`；
     - `resign_game()`：人认输；
     - `take_back()`：悔棋；
     - `continue_game(ai_color: Option<Color>)`：人机续弈，见第 4 条；
     - `set_ai_player(color, AiPlayer)`：修改 AI 用时，见第 6 条。
   - **新事件 `GameUpdated(GameView)`**，`GameView` 包括：
     - 阶段：进行中、暂停、已结束；
     - 双方玩家、轮到谁、AI 是否在思考；
     - 人的时钟：剩余保留时间、剩余读秒次数、本次读秒剩余秒数；
     - 手数、对局起点和最新一手的节点；
     - 结果和结束原因。
   - **AI 的回合**：
     - 对对局的最新局面发一个查询，上限用 `maxVisits` 和 `overrideSettings.maxTime`（纯网络时用 1 次计算）；
     - 查询的中间结果照常作为这个局面的实时分析发布；
     - 查询结束就落子：落首选点，或按随机开局规则选点；首选是 pass 就虚手；
     - 同一时刻对这个局面只有一个活跃查询，规则和现有实时分析一致；
     - 落子前先按认输规则判断。
   - **人的回合**：
     - 人用现有的 `play` 在对局的最新节点落子。轮到 AI 时，或不在最新节点时，`play` 返回类型化错误；
     - `ponder` 关时，人的回合不分析。
   - **对局进行中**：
     - 可以浏览别的节点，AI 照样只对对局的最新局面思考；
     - 会改变对局这条线的操作返回类型化错误「对局进行中」，包括删除、提升为主分支、打开或新建棋谱、编辑命令和撤销；
     - 暂停或结束后恢复正常。
   - **悔棋**：去掉人的上一手和 AI 对它的应手；AI 还没应时，只去掉人的那一手。不能退到对局起点之前。被悔掉的节点从树里删掉，并取消 AI 正在进行的思考。
   - **暂停**停止 AI 的思考和人的时钟；**继续**时如果轮到 AI，就重新开始它的回合。
   - **人的时钟**只在人的回合走。保留时间用完后进入读秒；每下一手，本次读秒重新计时；一次读秒用完就扣一次，次数用完即超时。
3. **结束和结果。** 结果写进 SGF 的 `RE`，`GameView` 里另带结束原因。
   - **认输**：人认输或 AI 按规则认输，记 `B+R` / `W+R`。
   - **超时**：记 `B+T` / `W+T`。
   - **双方连续虚手**：用 KataGo 数子，即对最终局面做一次 200 次计算的查询，取 `scoreLead`。结果的小数部分对齐贴目的小数部分：贴目 7.5 时总是半目，贴目为整数时可以和棋（`0`）。查询失败或超时，就结束对局但不写结果，原因写「未数子」。这是 KataGo 的估计，不是人工数子，写进 `ARCHITECTURE.md`。
   - **`stop_game()`**：结束对局，不写结果。
   - **自动保存**：上面每种结束方式之后都保存。保存失败用类型化错误报告，不影响对局已经结束。
4. **人机续弈 `continue_game(ai_color)`。** 不打开对话框，用保存的设置从当前节点续弈。`ai_color` 为空时，AI 执轮到走的一方（Java 的 Enter）。
5. **让 AI 落子最佳一手 `play_best_move(pv: Option<PvPick>)`。** 这不是对局：
   - 不带参数时，落当前分析的首选点；
   - 带 `PvPick { candidate, moves }` 时，按这个候选点的变化落前 `moves` 手。
   - 整串着法是一次撤销。没有分析结果时返回类型化错误。
6. **修改 AI 用时 `set_ai_player(color, AiPlayer)`。** 对局中也能改，从 AI 的下一手起生效，不重置任何时钟；不在对局中时只改保存的设置。
7. **fake-katago。** 按测试需要支持：
   - `overrideSettings.maxTime`，用模拟时间；
   - 指定首选为 pass；
   - 按脚本给出逐手的胜率和目差，用来测认输、数子和随机开局。
8. 新字段和新命令照约定：生成的 TS 里可选，mock 补默认值，mock 里是脚本化的 `command`。

## 不做

- GTP 引擎、引擎对局和 GTP 控制台：P3-10。HumanSL 陪练：P3-12。界面：P3-11。
- 自由让子：分析引擎没有 `place_free_handicap`，P3-10 只给 GTP 引擎做。
- Java 的「高级时间设置」（用户自己写 GTP 时间命令）和 KataGo 的加秒制、包干制：这些都是 GTP 时间命令。KataGo 在新版用每手的时间或计算量上限，P3-10 给 GTP 引擎做时间命令。
- 读秒语音、对弈模式里隐藏胜率：由界面决定。

## 验收标准

1. 设置：默认值、校验（AI 至少一项上限、时钟不能全为 0）、保存和回显，各有测试。
2. 对局（用 fake-katago，时间用 tokio 的暂停时间，不 sleep）：
   - 新棋盘和续弈都能开始；续弈时原有分支和注释都在；
   - AI 按秒数、计算量和纯网络三种上限落子；
   - 随机开局只在给定范围内选点。随机数可以注入种子，测试用固定种子；
   - 首选为 pass 时 AI 虚手；双方虚手后按贴目数子，包括和棋，以及查询失败时的「未数子」；
   - AI 认输、人认输、人超时（保留时间、读秒次数各自扣减）、`stop_game` 的结果和原因；
   - 轮到 AI 时 `play` 被拒绝；对局中删除等操作被拒绝；浏览别的节点不影响 AI；
   - 悔棋的两种情况，以及不能退到起点之前；
   - 暂停时时钟和思考都停，继续时恢复；
   - 修改 AI 用时从下一手生效；
   - 自动保存的文件名、`RE` 和保存失败的报告。
3. 人机续弈的 AI 执哪一方；让 AI 落子最佳一手的首选和整串变化（一次撤销），以及没有分析时报错。
4. `scripts/verify.sh --e2e` 通过；新依赖每个写一行理由；`docs/ARCHITECTURE.md` 更新。
