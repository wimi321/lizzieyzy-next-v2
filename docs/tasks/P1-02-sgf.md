---
id: P1-02
title: sgf：宽松解析、编码识别、序列化
depends_on: [P1-01, P0-03]
paths:
  - crates/sgf/**
  - fixtures/sgf/**
  - tools/fixture-oracle/**
  - Cargo.toml
  - docs/ARCHITECTURE.md
---

## 目标

按 ARCHITECTURE.md 4.2 实现 SGF 读写，能吃下真实世界里各种不规范的棋谱。

## 要做

- `parse(bytes) -> Result<Collection, SgfError>`，返回 `Collection { games, encoding, warnings }`。
- 编码识别顺序：BOM → 合法 UTF-8 → `CA` 属性 → GB18030 兜底。可选用 chardetng 识别 Big5 / Shift_JIS / EUC-KR，取舍在汇报里说明。
- 宽松模式：小写属性名、缺右括号、未转义的 `]` 这类能恢复的错误就恢复，并记一条 warning；不能恢复的返回带位置的错误。任何输入都不能 panic。
- 属性名恢复规则（P0-03 已确定）：
  - 全小写的属性名（如 `b[aa]`）转成大写。
  - 大小写混合的 FF3 长名（如 `AddBlack`）按 FF4 规则去掉小写字母，得到 `AB`。这一类在 `fixtures/sgf/` 里补一个语料文件，并手写期望值。新增的手写语料要在 `tools/fixture-oracle/` 的登记表里登记，保证 `generate.py --check` 继续通过。
- 属性值保留原始文本（转义已解开）；未知属性（包括 `LZ`）原样保留。
- `serialize(&Collection) -> String`：输出 UTF-8，根节点写 `CA[UTF-8]`，正确转义 `]` 和 `\`，换行统一为 `\n`。

## 不做

- 坐标、贴目、规则的语义解析（P1-03 做）。

## 验收标准

1. `fixtures/sgf/` 全部能解析；编码、节点数、注释数、未知属性名与 expected.json 一致。
2. 往返：对全部语料，parse → serialize → parse 得到相同的树（属性和值都相同）；proptest 随机生成的树也满足。
3. 随机字节输入不 panic：proptest 在 CI 里跑 1000 例，汇报里给出本地跑 10 万例的结果。
4. `LZ` 属性的值往返后逐字节一致。
5. GBK / GB18030、UTF-8 BOM、`CA` 与实际编码不符这三类棋谱，注释文本与期望一致。
