---
id: P3-10
title: GTP 后端与引擎对局：gtp 协议、GTP 引擎、引擎比赛、GTP 控制台
depends_on: [P3-09]
paths:
  - Cargo.toml
  - crates/gtp/**
  - crates/engine-session/**
  - crates/analysis/**
  - crates/app-core/**
  - tools/fake-gtp/**
  - tools/fake-katago/**
  - scripts/architecture.json
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
  - docs/ARCHITECTURE.md
---

## 目标

P3 第 5 批：第三方 GTP 引擎和引擎对局。Java 参考在 `../lizzieyzy-next`，只学行为：

- `gui/NewEngineGameDialog.java`、`gui/EnginePkConfig.java`、`enginegame/`（`EngineGameModule`、`EngineGameBatchState`、`ParticipantBinding`）；
- `gui/Manual.java`（干预引擎对局）、`analysis/EngineGameParameterSession.java`（对局中改 WRN / PDA）；
- `gui/GtpConsolePane.java`；`DesktopTimeControl` 和 `Leelaz.sendAiTime` 里的 GTP 时间命令。

`ARCHITECTURE.md` 已经定了：KataGo 只走分析引擎，GTP 后端只用于第三方引擎（以及用户自己按 GTP 方式配置的 KataGo）。本卡在 P3-09 的对局会话里加上 GTP 玩家，加上引擎比赛和 GTP 控制台。

## 要做

1. **`crates/gtp`**：新的纯同步 crate，和 `katago-protocol` 同级。`scripts/architecture.json` 里登记它，依赖只有 `go-core`。
   - 命令编码：可选的数字 id，坐标由 go-core 转换。
   - 回复解析：`=` 和 `?`、带 id、多行回复以空行结束。坏的回复返回类型化错误，不 panic。
   - `genmove` 的结果：落点、pass 或 resign，不区分大小写。
   - 标准命令：`protocol_version`、`name`、`version`、`known_command`、`list_commands`、`boardsize`、`clear_board`、`komi`、`fixed_handicap`、`place_free_handicap`、`set_free_handicap`、`play`、`genmove`、`undo`、`time_settings`、`time_left`、`final_score`、`quit`。引擎在 `list_commands` 里列出 `kata-set-rules` 和 `kata-time_settings` 时才用它们。
   - golden fixture 加 proptest（编码和解析往返）。
2. **GTP 会话**：在 `engine-session` 里，复用现有的本地进程传输层，一个 actor 独占一个进程。
   - GTP 是一问一答：同一时刻只有一条命令在途，其余排队，队列有上限。
   - 启动握手：发 `protocol_version`、`name`、`version`、`list_commands`，都有超时。
   - 对局中引擎退出或超时，不自动重启，以类型化错误结束这局。
   - 收发记录：发出的行、收到的行和 stderr，存进一个环形缓冲区，每个会话最近 2000 行，供 GTP 控制台读取。
3. **GTP 引擎列表**：存在 settings 里，和现有的 KataGo 引擎列表分开。
   - 每项 `GtpEngineEntry { id, name, program, args, working_dir, init_commands }`。`init_commands` 是每次启动后依次发的命令，任何一条出错就算启动失败。
   - 新命令：增、改、删，以及 `test_gtp_engine(id)`。测试就是启动、取回 name / version / list_commands，然后 quit。
4. **GTP 玩家**：P3-09 的 `AiPlayer` 新增 `Gtp { engine_id, time: GtpTime }`。
   - **`GtpTime`**：
     - `PerMove(seconds)`：发 `time_settings 0 N 1`；
     - `Byoyomi { main, period, stones }`：发 `time_settings`，每次 genmove 前用会话记的时钟发 `time_left`；
     - `Fischer { main, increment }` 和 `Absolute { main }`：只在引擎支持 `kata-time_settings` 时可用，否则返回类型化错误；
     - `EngineDefault`：不发任何时间命令；
     - `Custom(lines)`：原样发送用户写的命令（Java 的「高级时间设置」）。
   - **开局同步**：
     1. `boardsize`、`clear_board`、`komi`；
     2. 规则：引擎支持时用 `kata-set-rules`；
     3. 让子：固定让子用 `set_free_handicap` 发 document 摆好的点，保证各个引擎一致。只有 GTP 玩家可以选自由让子：发 `place_free_handicap N`，按引擎回的点摆子；
     4. 续弈时逐手 `play` 重放。
   - **每一手**：先 `play` 对手的着法，再 `genmove` 自己的颜色。引擎回 resign 就记认输；引擎回的着法不合法，就以类型化错误结束对局，不变成 pass。
   - **悔棋**：发 `undo`。引擎不支持或出错时，`clear_board` 后重放来重新同步。
   - **双方虚手后数子**：引擎支持时用 `final_score`；否则如果主引擎是 KataGo，就用 P3-09 的数子；都不行就不写结果，原因写「未数子」。
5. **引擎比赛 `start_match(MatchSetup)`**：新的比赛会话，和人机对局互斥。
   - **两方 A 和 B**：各是现有的 KataGo 引擎条目，或 GTP 引擎条目。
   - **每方设置**：
     - KataGo：每手上限（P3-09 的 `KataGoPlay`）、认输规则（默认至少 0 手、连续 2 手、胜率 < 10%）、开局广度 WRN 和激进度 PDA（用 `overrideSettings` 的 `wideRootNoise` / `playoutDoublingAdvantage`）；
     - GTP：`GtpTime`，认输由引擎自己决定。
   - **全局设置**：
     - 盘数（默认 1）、交换黑白（默认开）、贴目、让子、规则；
     - 最大手数：默认关，关时为 宽×高×2；
     - 开局 SGF 列表：按顺序或随机取，每盘从开局主线的末尾开始；
     - 随机开局：沿用 P3-09 的规则；
     - 从当前局面开始；
     - 自动保存：默认开。
   - **引擎进程**：
     - 某一方用当前主引擎，就复用主会话；其他 KataGo 条目为这场比赛另起分析会话，比赛结束后关掉；
     - 两方是同一个条目时，只起一个会话，两方的设置各自用 `overrideSettings`；
     - 比赛期间，主棋盘的实时分析停止。
   - **对局**：每盘都是主棋盘上的一盘新棋谱，用户可以看着下。
   - **每盘的结束**：认输、双方虚手（按第 4 条数子）、达到最大手数（不计分，原因「超手数」）、某一方出错（不计分，原因写明）。
   - **保存**：保存到 `<games_directory>/EngineGames/<A>_VS_<B>_<yyyyMMddHHmmss>/`，文件名是 `<序号>_<黑方>_vs_<白方>_<结果>.sgf`。
   - **新事件 `MatchUpdated(MatchView)`**：
     - 盘数进度；
     - 每方的总胜、执黑胜、执白胜；
     - 不计分的盘数，按原因分开；
     - 按开局 SGF 分开的统计；
     - 当前一盘的序号；
     - A 相对 B 的 Elo 估计和 2σ 区间：p = (胜 + 0.5 × 和) / 已计分盘数，σ = √(p(1−p)/n)，Elo = −400·log10(1/p − 1)；p 为 0 或 1 时没有估计。
   - **比赛控制**：
     - `pause_match` / `resume_match`；
     - `stop_match`：当前这盘不保存，已完成的统计保留；
     - `set_match_games(n)`：n 不能小于已下完的盘数加 1，否则在当前这盘结束后结束比赛；
     - `match_play_now()`：让轮到的 KataGo 方立即落子，GTP 方不支持；
     - `match_resign(color)`：手动判某方认输；
     - `set_match_side_params(side, wrn, pda)`：对局中改 KataGo 方的 WRN / PDA。
6. **GTP 控制台**：
   - `gtp_console(engine_id)` 返回该引擎会话的收发记录，`GtpConsoleLine` 事件推送新的行。
   - `gtp_console_send(engine_id, text)`：引擎没运行就先启动，返回引擎的回复。这个引擎正在对局或比赛中时，会改变对局状态的命令返回类型化错误，不发给引擎：`play`、`genmove`、`undo`、`clear_board`、`boardsize`、`komi`、`fixed_handicap`、`place_free_handicap`、`set_free_handicap`、`loadsgf`、`quit`。其他命令照发。
   - KataGo 分析引擎的 JSON 收发不进 GTP 控制台。
   - 收发记录总是记着；Java 的「控制台隐藏时仍记录 GTP」不需要了。
7. **`tools/fake-gtp`**：测试用的 GTP 引擎二进制，像 `tools/fake-readboard` 一样独立。可以用参数或脚本指定：
   - 落子序列、pass、resign、不合法的着法；
   - `final_score` 的回复；
   - 是否支持 `undo` 和 `kata-*` 命令；
   - 启动失败和中途退出。
8. 新字段和新命令照约定：生成的 TS 里可选，mock 补默认值，mock 里是脚本化的 `command`。

## 不做

- Zen 的 `zengtp-config` 棋力设置、`lz-genmove_analyze` / `kata-genmove_analyze`：新版只用标准 `genmove`，以及 KataGo 的时间和规则命令。
- Java 的 Zen、KataGoCustom、Leela 0.11、golaxy、gogui 特殊分支；保存胜率图 PNG；比赛结束后写 txt 报告，统计由界面显示。
- 远程引擎（WebSocket / SSH）：第 6 批。
- 界面：P3-11。

## 验收标准

1. `gtp` crate：编码、解析（单行、多行、带 id、错误、坏回复）、genmove 三种结果，有 golden 和 proptest。
2. GTP 会话（用 fake-gtp）：握手、命令排队、`init_commands` 出错算启动失败、超时、对局中退出、收发记录的上限。
3. GTP 玩家（用 fake-gtp）：
   - 开局同步，包括固定让子、自由让子和续弈重放；
   - 五种时间设置发出的命令，以及 `time_left`；
   - resign、不合法着法、悔棋（包括不支持 undo 时重新同步）；
   - `final_score` 数子，以及没有 `final_score` 时的退路。
4. 引擎比赛（KataGo 方用 fake-katago，GTP 方用 fake-gtp）：
   - 交换黑白和统计；开局 SGF 按顺序和随机；
   - 超手数、双方虚手、出错时不计分；
   - 同一条目只起一个会话，比赛结束后关掉另起的会话；
   - Elo 和区间的计算，包括 p 为 0 或 1 的情况；
   - 暂停、停止（当前这盘不保存）、改盘数、立即落子、手动认输、改 WRN / PDA；
   - 保存的目录和文件名。
5. GTP 控制台：记录和推送；没运行的引擎先启动；对局中会改变状态的命令被拒绝。
6. `scripts/verify.sh --e2e` 通过；新依赖每个写一行理由；`docs/ARCHITECTURE.md` 更新，包括第 2、3 节的 crate 和依赖方向。
