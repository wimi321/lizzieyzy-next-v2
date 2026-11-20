---
id: P3-46
title: 工程线：测试稳定——显卡提速测试的下载服务竞态和超时、Windows 控制台探测超时
depends_on: [P3-45]
paths:
  - crates/app-core/src/acceleration/tests.rs
  - crates/app-core/src/acceleration/tests/**
  - crates/engine-session/tests/no_console_windows.rs
  - crates/engine-session/Cargo.toml
  - tools/fake-katago/**
  - docs/ARCHITECTURE.md
---

## 目标

2026-11-19 的 CI 上，有两组测试在和改动无关的 PR 上反复失败，靠重跑才通过。只改测试和测试工具，不改产品行为（除非查出来确实是产品的 bug，那样先在汇报里说明，不要自己扩大范围）。

1. **显卡提速测试（P3-30，`crates/app-core/src/acceleration/tests*`）**
   - Linux：`cancelled_download_and_dismissed_hint_are_persisted_without_changing_engine` 失败在 `tests/support.rs` 的假下载服务里：逐字节读请求头时 `socket.read_u8().await.unwrap()` 遇到 `UnexpectedEof`。取消下载时客户端可能在请求头发完之前就断开，服务端的任务随之 panic，`close()` 里的 `within(self.server).await.unwrap()` 再把它变成测试失败。
   - macOS：同一个模块里 6 个测试同时在 `tests.rs` 的 `within`（5 秒安全超时）里超时，包括 `cpu_enables_cuda_preserves_profile_identity_persists_and_disables_to_cpu`、`cancellation_does_not_cancel_an_activation_queued_immediately_after_it`、`activation_excludes_another_activation_and_benchmark_and_cancellation_rolls_back`、`existing_benchmark_and_manual_trial_reject_acceleration`。CI 日志：run 44651691691 的 macOS Verify 任务（job 136007877477），以及 run 44654237408 的 Linux Verify 任务（job 136016517108）。
2. **Windows 控制台探测（P3-35，`crates/engine-session/tests/no_console_windows.rs`）**：PowerShell 探测进程在繁忙的 Windows runner 上启动超过 5 秒，`spawn_process_suppresses_console_while_plain_spawn_has_one` 超时（2026-11-16 的 #135、2026-11-19 的 #147）。PowerShell 冷启动本来就慢，不适合放在 5 秒的安全超时里。

## 要做

1. 假下载服务：客户端在请求头发完之前断开，是这些测试本来就要覆盖的情况（取消下载），服务端要把它当作一次结束的连接，继续等下一个连接或停止，不能 panic。统计请求数的语义保持不变，或者在汇报里说明怎么变了、为什么不影响断言。
2. macOS 上的超时：先从 CI 日志和本机复现查清楚是哪一步慢（比如每个测试第一次构建或启动 fake-katago、试启动引擎、写设置文件）。
   - 修法按原因定。AGENTS 的规则不变：不用 sleep 同步，安全超时 ≤ 5 秒。
   - 和测试要测的行为无关的准备工作（比如构建 fake-katago 二进制）放到安全超时之外，像 P3-41 的 `output_robustness.rs` 那样。
   - 不要靠把安全超时调大来解决。
3. 控制台探测：换一个启动快的探测程序代替 PowerShell，比如给 fake-katago 加一个只在 Windows 上编译的开关，报告 `GetConsoleWindow()` 是否为空。
   - `fake-katago` 禁止 unsafe 时，用已经在 `Cargo.lock` 里的 `windows-sys`，照 P3-41 的做法只给这一个函数局部放开，汇报里说明。
   - 对照组照旧：普通 `Command` 启动报告非空，`spawn_process` 启动报告为空。
   - 探测程序的构建放在安全超时之外。
4. 证明修好了：
   - 下载服务：加一个确定的测试，客户端发一半请求头就断开，服务端不 panic，后续请求照常处理。改之前这个测试失败。
   - 超时：汇报里写清楚查到的原因，以及本机加压（比如 `cargo test -p app-core --lib acceleration -- --test-threads=16`，同时另开几个 CPU 满载的进程）连续运行 30 次的结果。
   - 控制台探测：本机是 macOS，Windows 的验证留给 CI，汇报里说明。
5. ARCHITECTURE 测试部分补一句：测试里的子进程和二进制构建放在安全超时之外。

## 不做

- 改显卡提速、下载或控制台相关的产品代码（查出产品 bug 时先汇报）。
- 其他偶发失败的测试（另开卡）。

## 验收标准

- 上面的确定测试改之前失败、改之后通过；加压运行 30 次没有失败。
- `scripts/verify.sh` 通过；`check_budgets.py` 通过；三个平台的 clippy 都通过（Windows 部分本机检查不了时留给 CI，汇报里说明）。
