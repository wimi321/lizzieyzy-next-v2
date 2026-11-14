---
id: P3-08
title: 工程线：Windows CI 上偶尔超时的 app-core 集成测试
depends_on: [P3-07]
paths:
  - crates/app-core/tests/**
  - tools/fake-katago/**
---

## 目标

最近几乎每个 PR 都要在 Windows CI 上重跑一次。失败的总是 `crates/app-core/tests/` 里的三个集成测试文件，信息都是 `<名字> operation exceeded safety deadline: Elapsed(())`：

| 文件 | 失败过的测试 | CI 运行（attempt） |
|---|---|---|
| `analysis_limit.rs` | `manual_and_automatic_whole_game_analysis_complete_despite_live_limit_pause` | 43528544819（main）、43438770987、43339049249 |
| `auto_curve.rs` | `opening_starts_one_visit_automatic_review_without_interrupting_live_analysis` | 43538799317（1）、43339058696 |
| `problem_moves.rs` | `progress_refreshes_partial_losses_and_resume_fills_the_bar`、`threshold_changes_reclassify_retained_results_and_survive_restart_without_search` | 43538799317（2） |

用 `gh run view <id> --attempt <n> --log-failed` 可以看日志。CI 在 Windows 上已经是 `RUST_TEST_THREADS=2`（PR #81）。

每个文件都用 `within()` 给每一步套上 5 秒的安全超时。失败的这几个测试都要等整盘分析或自动复盘跑完，所以某一步在 Windows 上超过了 5 秒。日志只指向 `within()` 自己那一行，看不出是哪一步。

本卡要找到原因并修好，让这些测试在慢的机器上也有足够余量，同时不降低测试覆盖的行为。

## 要做

1. **超时要指出是哪一步。** 安全超时触发时，panic 信息要带上调用处的测试文件和行号，例如用 `#[track_caller]` 的同步包装、宏，或者给每一步一个标签。三个文件都改。
2. **在本机复现并测量。** 让所有核都忙起来来模拟慢的 CI，比如同时跑几个 `yes > /dev/null`，用完要停掉。然后测量这三个文件里每个 `within()` 步骤的耗时，找出耗时随棋局手数或引擎重启次数增长的步骤，把最慢的 10 步和耗时写进汇报。
3. **修掉原因。** 可以这样做：
   - 把棋局缩短到被测行为真正需要的手数；
   - 减少计算量；
   - 行为本身不涉及重启时，不重启引擎；
   - 在 fake-katago 里去掉不必要的等待；
   - 把一个大步骤拆成几个各自有意义的等待。
4. **同样的写法也要查。** `app-core/tests/` 里其他文件中，把整盘分析或复盘放在一个 `within()` 里的步骤，同样处理。

## 不做

- 不提高 5 秒上限（`AGENTS.md`：超时只是安全网，最多 5 秒）。不加重试，不在 Windows 上 `#[ignore]` 或跳过，不用 sleep 来同步。
- 不改 `.github/workflows/`、`scripts/` 和产品代码。如果原因在产品代码里，比如整盘分析本身就有不必要的等待，就停下来写进汇报，由维护者决定是否另开一张卡。

## 验收标准

1. 安全超时触发时，信息里有调用处的文件和行号。要有一个测试或一段演示来说明，比如一个故意超时、标了 `#[should_panic(expected = …)]` 的测试。
2. 汇报里有修改前后、在本机负载下最慢的 10 步及耗时。修改后每一步都不超过 1.5 秒。
3. 在本机负载下，`cargo test -p app-core --test analysis_limit --test auto_curve --test problem_moves` 连续跑 20 次全部通过，汇报里写上用的命令。
4. 没有删掉任何断言。缩短了棋局或减少了计算量的测试要逐个列出，并说明被测的行为仍然覆盖到了。
5. `scripts/verify.sh` 通过。
6. 维护者在 PR 上让 CI 连续跑 3 次，Windows 全部通过。
