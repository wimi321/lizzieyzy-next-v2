---
id: P1-32
title: 死活题
depends_on: [P1-27]
paths:
  - crates/go-core/**
  - crates/document/**
  - crates/app-core/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/**
  - apps/desktop/e2e/**
  - fixtures/tsumego/**
  - docs/ARCHITECTURE.md
---

## 目标

维护者 2026-10-31：「死活棋的功能也是需要」。

照 LizzieYzy Next 的「分析 → 死活题（Shift+E）」来做：

- 把当前局面当作一道死活题，在棋盘其余地方自动补上围墙和劫材，让 KataGo 只算这块棋的死活；
- 用完可以退出，回到原来的局面。

分工：后端由 Codex 做。死活题设置窗口和菜单接线由 Claude 在同一分支上做。Codex 在前端只改 `src/api/` 和 mock-backend。

## 要做

1. **行为照 Java 版。** 参照以下代码和对应的测试：
   - `rules/Tsumego.java`；
   - `gui/TsumeGoFrame.java`；
   - `Board` 里与死活题有关的部分，例如 `isTusmegoMode`、`tsumegoNode`、`saveTsumegoStatus`。
2. **围墙算法。** 放在 go-core 里，纯函数、同步。
   - 输入：棋盘和选项。
   - 输出：要加上的黑子和白子，以及 Java 版输出的其他内容。
   - 选项照 Java：
     - 先手方：继承、黑先、白先；
     - 进攻方：自动检测、黑、白；
     - 劫材：双方、进攻方、防守方、无；
     - 围墙距离。
3. **棋谱和分析。** app-core 新增命令：
   - 在当前节点上生成死活题局面。落到棋谱树的方式照 Java 版，生成后照常分析；
   - 退出死活题，以及再次执行，都照 Java 的规则回到原局面；
   - 同时补上 Tauri 命令、绑定和 mock-backend。
4. **记住选项。** 选项存进设置，下次打开沿用。Java 版存的是 `tsume-go-wall-distance` 和 `tsume-go-ko-threat`。

## 不做

- 屏幕抓取死活题（`captureTsumeGo`），以后另开卡。

## 验收标准

1. 对照测试：取若干局面，覆盖角、边、中腹，黑攻、白攻，以及不同的劫材和围墙距离，结果放在 `fixtures/tsumego/`。Rust 的输出要与 Java 完全一致。
   - 期望值能用本机运行 Java 版代码生成，就这样生成。
   - 不能的话，从 Java 测试里摘取样例，并在汇报里说明。
2. 单元测试：自动检测进攻方、劫材、围墙距离，以及先手方的各种选项。
3. 集成测试：生成死活题后，棋谱树和分析状态符合 Java 的行为；退出后回到原局面。
4. `scripts/verify.sh` 通过，绑定一致性测试通过。
