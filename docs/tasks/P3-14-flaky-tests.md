---
id: P3-14
title: 工程线：分析缓存写入测试和自动检查更新测试的偶发失败
depends_on: [P3-13]
paths:
  - crates/analysis/src/persistent_cache/*tests*.rs
  - crates/analysis/src/persistent_cache/tests/**
  - crates/analysis/tests/**
  - crates/app-core/src/release/tests.rs
  - crates/app-core/src/release/tests/**
---

## 目标

P3-08 修好了 app-core 集成测试在 Windows CI 上的超时，之后又发现两处偶发失败。它们和失败时所在的 PR 无关，重跑都能通过：

| 测试 | 现象 | 出现的地方 |
|---|---|---|
| `analysis` 的 `persistent_cache::write_tests::equally_deep_pending_final_preserves_latest_ownership_when_flushed` | `cache operation exceeded safety deadline: Elapsed(())`（`write_tests.rs` 的 `within()`） | Windows CI，运行 43568499629 |
| `analysis` 的 `persistent_cache::write_tests::dropping_last_handle_flushes_queued_and_pending_snapshots` | 同上 | 同上 |
| `app-core` 的 `release::tests::failed_backend_returns_none_logs_once_and_keeps_claim_across_restart` | `release/tests.rs:95`：`one warning from the automatic check: Disconnected`，也就是 `try_recv` 时警告还没到 | macOS 本机，`cargo test --workspace` 满负载时 |

用 `gh run view <id> --log-failed` 可以看 CI 日志。

本卡要找到原因并修好，让这些测试在慢的机器上也稳定，同时不降低它们覆盖的行为。做法照 P3-08。

## 要做

1. **分析缓存写入测试。**
   - 安全超时触发时，panic 信息要带上调用处的文件和行号。可以照 `crates/app-core/tests/common/mod.rs` 的 `within()`，用同步的 `#[track_caller]` 包装。
   - 在本机加负载复现（例如同时跑 12 个 `yes > /dev/null`，用完按 PID 逐个停掉），测量每个 `within()` 步骤的耗时，找出慢在哪里。例如是否在一个 5 秒期限里等了 SQLite 的多次落盘，或者等了真实时间的 1 秒写入窗口。
   - 修掉原因。可以这样做：
     - 用 tokio 的暂停时间推进写入窗口，不等真实时间；
     - 按每一步进展分段等待；
     - 减少不必要的落盘次数。
2. **自动检查更新的警告测试。**
   - 找到警告为什么还没到就被读取：比如 tracing 订阅只在当前线程生效、警告在别的线程或任务里发出，或者测试在警告发出前就去读了。
   - 改成等待警告到达，在安全期限之内（例如 `recv().await` 套 `within()`），或者让被测代码在返回前确定地发出警告。「只发一次」的断言要保留：先等到第一条，再确认在一个确定的同步点之前没有第二条。
3. 同一个文件里其他同样写法的测试一并处理。

## 不做

- 不提高 5 秒上限，不加重试，不在某个平台上 `#[ignore]` 或跳过，不用 sleep 来同步。
- 不改产品代码（`crates/*/src` 里非测试的文件）、`.github/workflows/` 和 `scripts/`。原因如果在产品代码里，就停下来写进汇报，附上证据（哪一步、多长时间、为什么）。

## 验收标准

1. 分析缓存测试的安全超时触发时，信息里有调用处的文件和行号，要有测试证明。
2. 汇报里写明两处的原因，以及修改前后在本机负载下最慢的 10 步和耗时。
3. 在本机负载下（12 个 `yes`，`RUST_TEST_THREADS=2`），下面两条命令各连续跑 20 次全部通过，汇报里写上命令：
   - `cargo test -p analysis persistent_cache`
   - `cargo test -p app-core --lib release::`
4. 没有删掉任何断言。
5. `scripts/verify.sh` 通过。
6. 维护者在 PR 上让 CI 连续跑 3 次，Windows 全部通过。
