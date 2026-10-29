---
id: P1-01
title: go-core：棋盘、规则、哈希、坐标
depends_on: [P0-01]
paths:
  - crates/go-core/**
  - Cargo.toml
  - docs/ARCHITECTURE.md
---

## 目标

按 ARCHITECTURE.md 4.1 实现纯同步的围棋规则核心。

## 要做

- 类型：`Color`、`Point`、`Move`、`BoardSize`（2..=25，可以不是正方形）、`Rules`（简单劫 / 位置超级劫 / 情境超级劫；是否允许自杀；数子或数目）、`Position`、`PlayOutcome`（含提子列表）、`IllegalMove`（被占、自杀、劫、越界，带原因）。
- `Position::play` 和 `Position::play_unchecked_ko`：后者供 SGF 回放用，照常提子但不判劫（见 P1-03）。
- Zobrist 哈希：用固定种子生成，覆盖盘面和轮走方；超级劫用历史哈希集合判断。
- 坐标：`Point ⇄ GTP（"Q16"，跳过 I）⇄ SGF（"pd"）`；pass 在 GTP 里是 `pass`，在 SGF 里是空值，19 路以内也接受 `tt`。
- 规则预设：chinese、japanese、korean、aga、tromp-taylor、new-zealand，每个名称映射到 `Rules` 和 KataGo 规则串。

## 不做

- 终局数子和形势判断（交给 KataGo 的 ownership）；SGF 解析。

## 验收标准

1. 单元测试覆盖：单块和多块提子、倒扑、按规则允许 / 禁止自杀、简单劫、三劫循环在超级劫下被禁止、pass、边角气数、非正方形棋盘、越界。
2. proptest：随机合法着手序列下，任何时刻棋盘上都没有零气的块；增量哈希和从零重算的哈希一致；落子数减提子数等于盘上子数。
3. 坐标往返测试，覆盖跳过 I、9/13/19 路和 pass。
4. 汇报 release 模式下回放 1 万手随机合法着手的耗时。用 `#[ignore]` 测试或 bench，不在 CI 里断言。
5. 不依赖 tokio、tauri 等（预算检查会自动拦）。
