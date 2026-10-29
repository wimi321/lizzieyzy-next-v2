---
id: P1-03
title: document：棋谱树、编辑命令、撤销、AnalysisKey
depends_on: [P1-02]
paths:
  - crates/document/**
  - Cargo.toml
  - docs/ARCHITECTURE.md
---

## 目标

按 ARCHITECTURE.md 4.3 实现应用里唯一的棋谱数据模型。

## 要做

- `GameDocument`：arena 树，`NodeId(u32)` 不复用，包含 `root`、`current`、`revision: u64`。
- SGF 语义：`SZ`、`KM`、`RU`（经 go-core 预设映射；不认识的规则保留原文，使用默认规则）、`HA`、`AB/AW/AE`（任意节点）、`B/W`、`PL`、`C`。
- 回放规则：
  - SGF 里记录的着手照常提子，但不判劫（`play_unchecked_ko`）。
  - 下在已有子处的节点标记为非法，它之后的局面返回带原因的错误。
  - 用户新下的棋按当前规则完整判断。
- 命令：
  - `Play(Move)`：已有相同的子节点就前进到它，否则新建变化。
  - `GoTo(NodeId)`、`Step(i32)`、`SetComment`、`DeleteNode`、`PromoteVariation`。
  - `apply(cmd) -> Result<Revision, DocError>`，支持撤销 / 重做。
- `position_at(NodeId)`：返回棋盘、轮走方、从根开始的着手、初始摆子、规则、贴目。可以缓存，但缓存失效必须正确。
- `AnalysisKey`：对 (尺寸、初始摆子、着手序列、规则、贴目) 用跨版本稳定的哈希，比如 blake3。不能用 std 的 `DefaultHasher`。
- `from_sgf` / `to_sgf`：未知属性跟着各自的节点走。

## 验收标准

1. 每个语料主线末端的盘面和提子数与 expected.json 一致。
2. 命令测试：
   - 覆盖前进已有分支和新建分支、删除、提升变化、注释、跳转。
   - proptest：随机命令序列执行后全部撤销，等于原文档；再全部重做，等于操作后的文档。
3. AnalysisKey：
   - 同一局面来自不同文档时 key 相同；贴目、规则、着手顺序不同时 key 不同。
   - 用硬编码的测试向量保证 key 不会悄悄变化。
4. `to_sgf(from_sgf(x))` 保留未知属性和 `LZ`。
