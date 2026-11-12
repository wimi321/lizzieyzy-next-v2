---
id: P2-09
title: 文件和缓存（二）后端：分析结果缓存、清除分析信息和缓存
depends_on: [P2-08]
paths:
  - crates/analysis/**
  - crates/app-core/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
  - docs/ARCHITECTURE.md
  - scripts/architecture.json
---

## 目标

P2 第 3 批的后一半。分析过的局面不用再算：重新打开同一盘棋、翻回分析过的手，直接显示上次的结果，引擎接着往上算。Java 的「Lizzie 缓存」是内存里的，新版存到本地数据库，重启后还在。Java 参考：`rules/Board.java` 里 `clearAllLizzieBestmoves`、`clearThisLizzieBestmoves`、`clearAllLizzieCache`、`clearThisLizzieCache` 一带，以及「启用 Lizzie 缓存」（`ConfigDialog2.java` 高级选项）。

## 要做

1. **缓存。** 按局面保存分析结果（候选点、胜率、目差、计算量、所有权），键至少包括局面（含轮谁下、贴目、规则）和引擎（程序、权重、影响结果的配置）。同一个键已有结果时，先显示缓存的，引擎新的结果计算量更高时替换并写回。换了权重就不用旧结果。
2. **存储。** 本地 SQLite，放在应用数据目录；大小有上限（默认值你定，写进汇报），超过时先删最久没用的。数据库打不开或损坏时程序照常运行，只是不缓存，并记一条警告；不要因为缓存失败影响分析。按 AGENTS.md，数据库访问不能用锁，放在一个 actor 里。
3. **开关和清除。**
   - 设置里加「启用缓存」，默认开；关掉后不读不写。
   - 「清除分析信息（此手 / 全局）」：清掉棋谱里这一手或全部手存的分析结果（包括 P2-08 读进来的 LZ 数据），可以撤销，跟其他编辑一样。
   - 「清除缓存（此手 / 全局）」：清掉数据库里这一局面或全部的缓存。
4. 新命令、新字段照约定；mock 里命令是脚本化的 `command`。

## 不做

- 界面：菜单项和设置项由界面卡接。

## 验收标准

1. 缓存命中、更高计算量替换、换权重不命中、关掉开关不读不写、上限淘汰，各有测试（用 `tools/fake-katago`）。
2. 数据库损坏或无法写入时分析照常，有测试。
3. 清除分析信息可撤销；清除缓存只动数据库；各有测试。
4. 如果新增 crate 或依赖方向变化，更新 `scripts/architecture.json`（本卡允许），并在汇报里说明；新依赖写一行理由。`docs/ARCHITECTURE.md` 更新。
