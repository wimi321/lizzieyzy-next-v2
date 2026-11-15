---
id: P3-34
title: 工程线：窗口放回屏幕在最小尺寸大于工作区时前后不一致
depends_on: [P3-29]
paths:
  - crates/app-core/src/windows.rs
  - crates/app-core/src/windows/**
  - crates/app-core/proptest-regressions/windows/**
  - docs/ARCHITECTURE.md
---

## 目标

P3-27 的 `fit_window_bounds` 在最小尺寸大于工作区时，结果取决于输入，同一个窗口调整两次结果不同。CI 的属性测试 `windows::tests::fitted_bounds_are_inside_one_work_area` 偶尔因此失败（2026-11-15，一个只改文档的 PR 上），失败的例子：

- 工作区 `(6775, -442, 303×6273)`，另一个 `(50, -2299, 7670×2368)`，最小尺寸 `(304, 1)`，窗口 `(12331, -5127, 1×1)`；
- 第一次得到宽 303（先抬到最小值 304，再缩进工作区）；
- 对结果再调整一次得到宽 304（303 不超出工作区，不缩，只抬到最小值）。

## 要做

1. 照 ARCHITECTURE 已经写下的「工作区小于最小尺寸时，以完整可见优先」实现：每个方向上，尺寸 = min(max(原尺寸, 最小值), 工作区尺寸)，再平移进工作区。系统窗口的最小尺寸由 Tauri 另外保证。ARCHITECTURE 的说明不需要改；如果实现里还有别的地方和它不一致，一并改正并写进汇报。
2. 上面的例子写成普通的单元测试：两次结果相同，宽 303，在工作区内。
3. 把 CI 给出的种子加进 `crates/app-core/proptest-regressions/windows/tests.txt`：`cc 23ca8ce1da399cc1c3fa633281ac6ff56816059d6df24651234ee067d781b955`。
4. 属性测试不放宽：结果总在某个工作区内，且再调整一次不变。

## 验收标准

- 新的单元测试在改之前失败、改之后通过。
- `cargo test -p app-core windows::` 连续跑 20 次都通过（汇报里写命令和结果）。
- `scripts/verify.sh` 通过；`check_budgets.py` 通过。
