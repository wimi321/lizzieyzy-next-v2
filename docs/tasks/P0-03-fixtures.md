---
id: P0-03
title: SGF 语料、期望结果与 KataGo 真实录制
depends_on: [P0-01]
paths:
  - fixtures/**
  - tools/fixture-oracle/**
  - tools/record-katago/**
  - resources/katago/**
---

## 目标

给后面的解析器、规则、协议、会话测试准备标准答案。期望结果必须来自独立实现，不能来自本项目自己的代码。

## 要做

- `fixtures/sgf/`：约 40 个棋谱，至少覆盖以下类别：
  - 棋盘：基本 19 路对局；9 路、13 路；非正方形（`SZ[19:13]`）。
  - 摆子与着手：让子（`HA` + `AB`）；中途摆子（非根节点的 `AB/AW/AE`）；pass；非法着手（下在已有子处）。
  - 结构：多分支变化；一个文件含多盘棋。
  - 注释：含 `\]`、换行，以及中英日韩文字。
  - 编码：UTF-8 带 BOM；GBK / GB18030 且没有 `CA`；`CA` 与实际编码不符。
  - 来源：野狐下载的棋谱、弈客下载的棋谱、Java 版保存的带 `LZ` 分析属性的棋谱。
  - 常见畸形：小写属性名、缺右括号、FF[3] 旧写法、重复属性。
- 语料来源和隐私：
  - 只用维护者自己的对局、公有领域棋谱或自己构造的局面。
  - 真实对局要匿名化：`PB/PW` 等改成 Black/White，去掉账号信息。
- 每个棋谱配一个 `<名字>.expected.json`，字段：
  - 实际编码、棋盘尺寸、贴目、规则原文、让子；
  - 节点数、主线手数、根节点下的分支数、注释数、未知属性名列表；
  - 主线终局盘面（每行一个字符串，`.` / `X` / `O`）、双方提子数。
- `tools/fixture-oracle/`：用 Python 的 sgfmill（独立实现）生成上面的期望值；`generate.py --check` 重新计算并和已提交的文件比对。sgfmill 处理不了的文件（比如畸形输入）手写期望值，并在 json 里标 `"source": "manual"`。
- `resources/katago/analysis.cfg`：
  - 从 KataGo 1.16.5 的 `analysis_example.cfg`（Homebrew 路径 `/opt/homebrew/share/katago/configs/`）改出。
  - 显式设置 `reportAnalysisWinratesAs = BLACK`，线程数用保守默认值。
  - 每个改动的键写一行英文注释。
- `tools/record-katago/`：用本机 KataGo 和上面的 cfg 跑一组场景，写到 `fixtures/katago/<场景>.jsonl`。
  - 要求 Python 3.9 可运行，只用标准库。
  - 程序是 `/opt/homebrew/bin/katago`；模型通过参数传入，如 `/opt/homebrew/share/katago/kata1-b18c384nbt-*.bin.gz`，不提交模型。
  - 文件格式：首行记录 KataGo 版本、模型名、后端；之后每行 `{"dir":"in"|"out"|"err","t_ms":…,"line":…}`。
  - 至少录这些场景：
    - `query_version`；
    - 19 路开局，带 ownership 和 policy；
    - `reportDuringSearchEvery`：至少 3 个中间结果，再到最终结果；
    - 搜索中 `terminate`；排队中、还没开始就 `terminate`；`terminate_all`；
    - `analyzeTurns` 一次 5 手，带不同的 `priority`；
    - 9 路局面；含 pass 的局面；
    - 非法着手（应出现错误行）；未知字段（应出现警告行）。
  - HumanSL 场景只在提供 human 模型时才录，否则跳过并在汇报里说明。
  - 每个 jsonl 控制在 300KB 内：maxVisits 50–200，`analysisPVLen` 8，ownership 只在 1–2 个场景里开。

## 不做

- 不写本项目的解析器（P1-02 做）；不提交模型和二进制。

## 验收标准

1. 汇报里列出"覆盖类别 → 文件名"对照，上面每一类都有。
2. `python3 tools/fixture-oracle/generate.py --check` 通过（Claude 验收时会在干净的 venv 里装 sgfmill 复跑）。
3. `python3 tools/record-katago/record.py --katago … --model …` 在本机能完整重跑。汇报里描述两个 `terminate` 场景里 KataGo 实际返回了什么（字段、顺序），用于修订 ARCHITECTURE.md 的 4.4 和 4.5。
4. `python3 scripts/check_budgets.py` 通过（文件大小预算）；真实对局已经匿名化。
