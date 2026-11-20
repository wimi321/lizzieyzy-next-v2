---
id: P3-44
title: 工程线：核心性能基准——打开大棋谱、第一条分析结果、内存，进 CI 并设上限
depends_on: [P3-43]
paths:
  - crates/app-core/benches/**
  - crates/app-core/tests/performance_support.rs
  - crates/app-core/Cargo.toml
  - tools/fake-katago/**
  - .github/workflows/bench.yml
  - docs/ARCHITECTURE.md
---

## 目标

ROADMAP 工程线的性能基准分两步。这张卡做第一步：不启动界面，用 app-core 测打开大棋谱的耗时、第一条分析结果的延迟和内存占用，在 CI 上每次运行，并给每项设上限，防止数量级的退步（比如打开棋谱变成平方复杂度）。

桌面程序的冷启动和整个进程的内存，在正式版安装包上测，另开一张卡。和 Java 版的对比由维护者在同一台机器上测，不在这张卡里。

## 要做

1. 在 app-core 里加一个基准目标 `crates/app-core/benches/performance.rs`（`harness = false`，不新建 crate）。
   - 用 `cargo bench -p app-core --bench performance --locked -- <参数>` 运行。
   - 通过 app-core 的公开 API 驱动，和桌面壳调用的路径一样：临时数据目录、真实的 `AppCore`、订阅事件。不调用内部函数，也不复制逻辑。
   - 引擎用 fake-katago（`fake_katago::binary_path()`），回复不加模拟延迟。需要新开关时加在 fake-katago 里。
2. 测试棋谱在运行时用固定种子生成，不提交大文件：
   - **复盘过的对局**：19 路，400 手主线，每手都有注释和 LZ 分析数据（像 Java 版或新版保存过分析的棋谱），另有 30 个分支，每个分支 10 手。
   - **大棋谱树**：19 路，大约 20,000 个节点，大量分支（像题库或研究树）。
   - 生成的 SGF 先用 `sgf::parse` 确认能打开，基准才开始计时。
3. 场景和测量：

   | 场景 | 计时范围 |
   |---|---|
   | 打开复盘过的对局 | 调用打开文件的 API，到收到界面等待的那个文档事件 |
   | 打开大棋谱树 | 同上 |
   | 跳到最后一手 | 打开复盘过的对局以后，从跳转命令到对应的文档事件 |
   | 引擎就绪 | 用已保存的 fake-katago 引擎设置启动，到会话就绪 |
   | 第一条分析结果 | 就绪后开始分析，到当前局面的第一个分析事件 |
   | 落子后的分析 | 落一手，到新局面的第一个分析事件 |

   - 每个场景先热身 1 次，再测 7 次，记录中位数和最大值。
   - 内存：两个打开棋谱的场景和「第一条分析结果」场景，各在一个单独的子进程里运行（基准程序用参数启动自己），记录峰值常驻内存。macOS 和 Linux 用 `getrusage`，Windows 用 `GetProcessMemoryInfo`。需要新依赖时只作为 app-core 的开发依赖，并在汇报里写理由。
4. 输出和上限：
   - 结果写成 JSON（`target/bench/performance.json`），同时打印一张表。传 `--summary <文件>` 时，追加一张 Markdown 表，供 CI 写进 step summary。
   - 上限文件 `crates/app-core/benches/limits.json`，按平台（linux、macos、windows）和场景给出中位数上限和内存上限。传 `--check` 时，超过上限就以非零状态退出，并列出超出的项。
   - 初始上限：本机中位数的 5 倍、峰值内存的 2 倍，取整到好读的数。CI 的实际数据维护者在 PR 上看过以后再调。
   - 基准自己的逻辑（统计、上限比较、JSON 读写、棋谱生成器的确定性）有普通单元测试，在 `cargo test` 里运行，不计时、不依赖机器速度。`harness = false` 的基准目标不会被 `cargo test` 运行，所以把这些逻辑放在 `benches/` 下的模块里，由 `crates/app-core/tests/performance_support.rs` 用 `#[path]` 引入后测试。
5. CI 新增 `.github/workflows/bench.yml`：
   - 触发：pull_request、推到 main、workflow_dispatch。
   - 在 Linux（ubuntu-24.04）上用 release 配置运行 `--check`，结果写进 step summary，JSON 作为 artifact 上传。
   - 第三方 action 固定到 commit SHA，注释写版本号。
6. ARCHITECTURE 加一节「性能基准」：场景、怎么在本机运行、上限文件在哪里、怎么调整。

## 不做

- 桌面程序冷启动、WebView 和整个进程的内存（另开卡，在正式版安装包上测）。
- 和 Java 版的对比（维护者做）。
- 真的 KataGo 和 GPU。
- 放进 `scripts/verify.sh`。

## 验收标准

- `cargo bench -p app-core --bench performance --locked -- --check` 在本机通过，汇报里附结果表（中位数、最大值、峰值内存）。
- 基准自己的单元测试在 `cargo test -p app-core` 里通过。
- 故意让某个场景变慢（比如在本地临时加一个延迟）时 `--check` 失败，汇报里写怎么确认的（不提交这个改动）。
- `scripts/verify.sh` 通过；`check_budgets.py` 通过。
