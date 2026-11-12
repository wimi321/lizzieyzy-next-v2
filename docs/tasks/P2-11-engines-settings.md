---
id: P2-11
title: 引擎和设置（一）后端：多个引擎、选主引擎、引擎规则、综合设置里的引擎项、导入 Java 版配置
depends_on: [P2-09]
paths:
  - crates/app-core/**
  - crates/engine-session/**
  - crates/analysis/**
  - crates/katago-protocol/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
  - fixtures/**
  - docs/ARCHITECTURE.md
---

## 目标

P2 第 4 批「引擎和设置」的前一半。Java 用户手里往往配着好几个引擎（不同权重、不同机器），换着用；新版要能同样管理，并且第一次打开时把 Java 版的配置搬过来，不用重配。Java 参考在 `../lizzieyzy-next`，只学行为：`gui/Menu.java` 里「选择主引擎」「引擎规则」、`gui/ConfigDialog2.java` 的引擎与分析、高级选项、棋谱加载各页，`Config.java` 里引擎列表和这些设置的读写。

## 要做

1. **多个引擎。** 设置里从一个 `engine` 变成引擎列表加「当前主引擎」：
   - 增、改、删、调整顺序；每个引擎有名字、程序、权重、配置、额外参数（沿用现有 `EngineProfile`）；
   - 选主引擎：关掉当前的，启动选中的，分析照常继续；启动失败时回到原来的引擎并报原因（和现在试用新配置的做法一致）；
   - 重启当前引擎、关闭当前引擎（关闭后不分析，状态栏能看出来）；
   - 旧设置文件只有 `engine` 时迁移成只有一项的列表，设置版本照现有规则处理。
2. **引擎规则。** Java「设置 → 引擎规则」的胜负判断规则（中国、日本、韩国、AGA、Tromp-Taylor 等，照 Java 的选项）：发给 KataGo 的规则跟随这个设置；Java 里「跟随棋谱」之类的选项如果有，照它。保存在设置里。
3. **综合设置里的引擎项。** 照 Java 的含义和默认值，保存在设置里并生效：
   - 读取棋谱贴目（关掉时用设置里的默认贴目）；
   - 自动检测引擎是否存活（引擎没有响应时按现有的重启流程处理）；
   - 本地分析刷新间隔（发给 KataGo 的 `reportDuringSearchEvery`）。
4. **导入 Java 版配置。** 新命令：给定 Java 版的目录（或自动在常见位置找），读出引擎列表、当前主引擎、引擎规则、以及 P2-06 已有对应项的显示设置，合进新版设置；路径不存在的引擎照样导入但标出来。先返回预览（导入什么、哪些有问题），再由第二个命令确认写入。不改 Java 版的任何文件。
5. 新字段、新命令照约定：生成的 TS 里可选，`e2e/mock-backend.ts` 补默认值，命令在 mock 里是脚本化的 `command`。

## 不做

- 下载权重和提速组件、路径诊断：P2-12。
- 界面。

## 验收标准

1. 引擎列表的增删改排序、选主引擎（成功、失败回退）、重启、关闭，各有测试（`tools/fake-katago`）。
2. 旧设置迁移、引擎规则和三个综合设置项的默认值与生效，有测试；默认值逐项对照 Java，写进汇报。
3. Java 配置导入：`fixtures/` 下放一份 Java 版配置样本（照 `Config.java` 的格式手写，注明来源），预览和确认写入各有测试；Java 目录只读。
4. `docs/ARCHITECTURE.md` 更新。
