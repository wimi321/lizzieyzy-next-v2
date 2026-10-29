---
id: P1-06
title: analysis：分析协调器
depends_on: [P1-03, P1-05]
paths:
  - crates/analysis/**
  - Cargo.toml
  - docs/ARCHITECTURE.md
---

## 目标

按 ARCHITECTURE.md 4.6 实现"当前局面 → 实时分析"的协调，从结构上杜绝旧局面的分析串到新局面。

## 要做

- actor 接收 `Focus { revision, node_id, position, key }`、`SetPondering(bool)`、`SetLiveSettings`，输出 `AnalysisSnapshot`。
- 焦点变化时：terminate 旧请求 → 先推缓存 → 发新请求。只转发 (revision, node_id) 等于当前焦点的结果。
- 由 `GamePosition` 构造 KataGo 请求：着手、初始摆子、规则串、贴目、尺寸、轮走方。
- 节流：每个焦点最多 20 次/秒。
- 内存 LRU 缓存：key 是 AnalysisKey 加引擎指纹，保留 visits 最多的结果。

## 验收标准

1. 连续 50 次快速切换焦点后，所有输出都属于最后一个焦点。用 proptest 在 fake-katago 随机延迟下验证：永远不会输出过期焦点的结果。
2. 回到看过的局面时，先立即收到缓存结果，再收到新的实时结果；更深的结果会替换缓存。
3. fake 每秒推 100 次时，输出不超过 20 次/秒。
4. 暂停分析会 terminate 当前请求；恢复后发新请求。
5. 分析途中引擎重启，当前焦点的分析会自动恢复。
