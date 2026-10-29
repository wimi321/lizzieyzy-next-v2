---
id: P0-02
title: Java 版功能清单
depends_on: []
paths:
  - docs/inventory/**
---

## 目标

给维护者一份完整的功能清单，用来决定哪些进 MVP、哪些放 P3、哪些砍掉。只读 Java 仓库，不写代码。

## 要做

- 只读 `../lizzieyzy-next`，重点看：
  - `search/FunctionCatalog.java`：离线搜索用的功能元数据，最全。
  - `gui/Menu.java` 的菜单、`gui/BottomToolbar.java`。
  - 所有 `gui/*Dialog.java`、`*Frame.java`、`*Panel.java`。
  - `l10n/DisplayStrings_zh_CN.properties`（中文名称）、README、ROADMAP。
- 写 `docs/inventory/FEATURES.md`，按领域分组：复盘与分析、引擎与设置、对局与训练、在线与同步、工具、系统与发布。每行的列：功能（中文名）｜用户入口｜Java 主要源码｜外部依赖（引擎 / 网络 / 平台）｜建议层级（MVP / P2 / P3 / 候选砍掉）｜理由（一句话）。
- 文末加"下载数据"：用 `gh api` 取最近 3 个正式版（非 prerelease）每个安装包的下载数，按变体汇总成表。

## 不做

- 不改 Java 仓库，不提交脚本。

## 验收标准

1. FunctionCatalog 的每个条目、Menu 的每个顶层菜单项都能在表里找到；汇报里给出条目总数、已覆盖数和核对方法。
2. 每一行都有建议层级和理由；MVP 范围和 ROADMAP 的 P2 描述一致，不一致的在汇报里说明。
3. 维护者审阅后，把"建议层级"列改成"层级"（由维护者完成，不是 Codex）。
