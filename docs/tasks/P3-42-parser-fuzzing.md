---
id: P3-42
title: 工程线：解析器的模糊测试——cargo-fuzz、PR 冒烟、每周长跑
depends_on: [P3-41]
paths:
  - fuzz/**
  - fixtures/fuzz/**
  - .github/workflows/fuzz.yml
  - Cargo.toml
  - crates/sgf/src/**
  - crates/document/src/**
  - crates/katago-protocol/src/**
  - crates/gtp/src/**
  - crates/fox-kifu/src/**
  - crates/readboard/src/**
  - crates/app-core/src/web_board/**
  - docs/ARCHITECTURE.md
---

## 目标

外部输入的解析器现在有 proptest 的「任意字节不 panic」测试（`sgf`、GIB、KataGo 输出行、测速输出），每次 1000 个随机输入，没有覆盖率引导，也没有覆盖打开棋谱之后的处理。加上覆盖率引导的模糊测试（cargo-fuzz / libFuzzer）：PR 上短跑，每周长跑，发现的问题修掉，并留下回归测试。

## 要做

1. 在仓库根目录建 `fuzz/`，是一个独立的 cargo-fuzz 项目：
   - 有自己的 `[workspace]`，主工作区和 `scripts/verify.sh` 不编译它。根 `Cargo.toml` 需要时加 `exclude`。
   - nightly 工具链固定到一个具体日期（比如 `nightly-2026-11-01`），只用于模糊测试；`cargo-fuzz` 固定版本，用 `--locked` 安装。
   - 有自己的 `Cargo.lock`，提交进仓库。
2. 每个目标只调用生产代码的入口，不复制解析逻辑。入口是私有的，可以在该 crate 里开一个最小的公开入口（比如 `#[doc(hidden)]`），汇报里列出来。目标至少包括：

   | 输入来源 | 目标做什么 |
   |---|---|
   | 用户打开的 SGF | `sgf::parse`；成功时序列化再解析，必须成功，而且第二次序列化和第一次相同（一轮之后稳定）。确实有合理的例外（比如编码转换）时，收窄性质并在汇报里说明 |
   | 用户打开的 GIB | `sgf::parse_gib` |
   | 打开棋谱之后 | SGF → 每盘 `Document::from_sgf` → 照 app-core 打开棋谱的路径走一遍每个节点（落子、摆子、规则、贴目）。不 panic |
   | 棋谱里的 LZ 分析 | `document::lz::parse`，棋盘大小和轮走方也由输入决定 |
   | KataGo 的输出 | `katago_protocol::parse_line` |
   | GTP 引擎的输出 | 回复解析、`genmove` 解析、`kata-analyze` 行解析 |
   | 野狐和腾讯服务器的回复 | `fox-kifu` 的 JSON 解析和棋谱整理 |
   | readboard 的输出 | `readboard` 的协议行解析 |
   | 网页旁观里局域网浏览器发来的消息 | `web_board` 解析 WebSocket 文本消息的那一步 |

3. 种子语料在运行时从 `fixtures/` 里的真实样本生成（脚本或 `fuzz/` 里的小程序都可以），不提交生成的语料。可以加 libFuzzer 字典（比如 SGF 的属性名）。
4. CI 新增 `.github/workflows/fuzz.yml`，只在 Linux 上跑：
   - pull_request 和推到 main：只在上表涉及的 crate 或 `fuzz/` 有改动时运行。编译全部目标，每个目标从种子语料跑 30 秒。
   - 每周一次（schedule）和 workflow_dispatch：每个目标跑 15 分钟，目标之间并行（matrix）。
   - 发现崩溃时任务失败，崩溃输入和最小化后的输入作为 artifact 上传。
   - 第三方 action 固定到 commit SHA，注释写版本号。
5. 本机每个目标至少跑 10 分钟。汇报里列出每个目标的执行次数和覆盖率（`cov`）。
6. 发现的每个问题：
   - 在对应 crate 里修好；
   - 最小化后的输入放进 `fixtures/fuzz/<目标>/`；
   - 在该 crate 的普通测试里加一个回归测试读这个文件，这样不用 nightly 的 `cargo test` 也一直覆盖它。
   - 修复改变了行为（比如以前接受、现在拒绝的输入）时，汇报里逐条说明。
7. ARCHITECTURE 的测试部分加一段：模糊测试的目标列表、怎么在本机运行、CI 什么时候跑、崩溃怎么变成回归测试。

## 不做

- 远程算力、智子和 AI 解说的网络回复（以后另开卡）。
- 引擎进程输出的传输层（P3-41 已处理）。
- 把模糊测试放进 `scripts/verify.sh`。

## 验收标准

- `cd fuzz && cargo +<固定的 nightly> fuzz build` 成功，每个目标都能从种子语料运行。
- 每个修复都有回归测试，改之前失败、改之后通过。
- `scripts/verify.sh` 通过；`check_budgets.py` 通过。
- 汇报里有每个目标的本机运行结果，以及 `fuzz.yml` 用 `act` 或语法检查验证过的说明（不能在本机跑 GitHub Actions 时，留给维护者在 PR 上看）。
