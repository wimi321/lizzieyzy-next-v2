---
id: P1-45
title: fake-katago 测试：临时目录不再重名
depends_on: [P1-04]
paths:
  - tools/fake-katago/tests/**
---

## 目标

fake-katago 的集成测试偶发失败，要根治。

原因：测试用「进程号 + 纳秒时间」给临时目录起名。同一个测试进程里两个测试并行时，可能读到同一个时间值，第二个 `create_dir` 报 `AlreadyExists`。

已经出过两次：
- 2026-11-06，PR #51 的 macOS CI：`tools/fake-katago/tests/benchmark.rs:46`；
- 2026-11-11，本地 `scripts/verify.sh`：同一处。

## 要做

1. 下面四处临时目录改成不会重名的起名方式：
   - `benchmark.rs` 的 `TempDirectory`；
   - `fault_once.rs` 的 `MarkerDirectory`；
   - `startup_gate.rs` 的 `GateDirectory`；
   - `request_log.rs` 里手写的目录。
2. 四处共用 `tests/common` 里的一个辅助类型，不要各写一份。名字里加进程内递增的序号即可；测试代码里可以用原子计数器。离开作用域时照旧删除目录。

## 不做

- 不改 fake-katago 本身的行为。
- 不加新依赖（不引入 `tempfile`）。

## 验收标准

1. `cargo test -p fake-katago` 连续跑 30 遍全部通过。汇报里给出命令和结果。
2. `scripts/verify.sh` 通过，不需要 `RUST_TEST_THREADS=1`。
