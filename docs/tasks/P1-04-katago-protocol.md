---
id: P1-04
title: katago-protocol 与 fake-katago
depends_on: [P0-03]
paths:
  - crates/katago-protocol/**
  - tools/fake-katago/**
  - Cargo.toml
  - docs/ARCHITECTURE.md
---

## 目标

协议类型以真实录制为准；再做一个可控的假 KataGo，让后面所有引擎相关的测试都不需要 GPU。

## 要做

- katago-protocol：
  - 按 ARCHITECTURE.md 4.4 定义请求、动作、响应、错误行和警告行。
  - `parse_line` 永远不 panic；不认识的字段直接忽略。
  - 与录制不一致的地方以录制为准，并更新 ARCHITECTURE.md。
- fake-katago 的基本行为：
  - 命令行形状与 `katago analysis -config … -model …` 相同。
  - 支持 `query_version`、分析请求、`reportDuringSearchEvery`、`analyzeTurns`、`priority`、`terminate`、`terminate_all`，非法着手时输出错误行。
  - 结果是确定性的：候选点由着手序列和手数的哈希生成。visits 按 `--visits-per-sec` 增长，时间按 `--time-scale` 缩放。
  - `terminate` 等行为模仿 P0-03 录制里观察到的真实表现。
- fake-katago 的故障注入参数：
  - `--startup-delay-ms`：延迟启动。
  - `--fail-startup`：在 stderr 打印"模型找不到"，然后以退出码 1 退出。
  - `--crash-after-queries N`、`--crash-after-ms N`：到时崩溃。
  - `--hang-after-queries N`：之后不再输出任何内容。
  - `--garbage-every N`：每隔 N 行插入一行坏数据。
  - `--threads N`：并发数，配合 priority 使用。
- 提供库函数 `fake_katago::binary_path()`：必要时执行 `cargo build -p fake-katago`，返回可执行文件路径，供其他 crate 的测试使用。

## 验收标准

1. `fixtures/katago/` 里每一行 out 都能解析；每个场景里，序列化出来的请求与录制的 in 行语义一致。
2. fake-katago 的输出全部能被 katago-protocol 解析。
3. 每个故障注入参数都有一个真正启动进程的集成测试。
4. 对照测试：对每个录制场景，fake 输出的形状（字段集合、中间结果到最终结果的顺序、terminate 的表现）与真实录制一致。
