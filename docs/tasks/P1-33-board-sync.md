---
id: P1-33
title: 棋盘同步：Windows 上接入 readboard
depends_on: [P1-27]
paths:
  - crates/readboard/**
  - crates/app-core/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/**
  - apps/desktop/e2e/**
  - tools/fake-readboard/**
  - fixtures/readboard/**
  - Cargo.toml
  - scripts/architecture.json
  - docs/ARCHITECTURE.md
---

## 目标

维护者 2026-10-31：「棋盘同步功能还是需要」。

LizzieYzy Next 在 Windows 上通过 readboard（https://github.com/qiyi71w/readboard ）同步棋盘：

- readboard 识别野狐等客户端里的棋盘；
- 识别到的局面同步到主棋盘，并实时分析；
- Windows 安装包里自带 `readboard.exe`。

新版在 Windows 上接入同一个 readboard。Java 版只在 Windows 上提供这个功能（`Menu.java` 里的 `OsPlatform.isWindows()`），新版也一样。

分工：后端由 Codex 做。菜单、同步状态这些界面由 Claude 在同一分支上做。Codex 在前端只改 `src/api/` 和 mock-backend。

## 要做

1. **先读懂协议。** 行为参照 Java 版的 `analysis/ReadBoard.java` 和相关类（`ReadBoardStream`、`Sync*`）。
   - 弄清 readboard 与 Java 版之间的协议：启动参数、连接方式、消息格式。
   - 在 ARCHITECTURE.md 里写一节说明。
2. **新 crate `crates/readboard`。**
   - 纯、同步。
   - 负责协议消息的解析和生成，用录制的样例做 golden 测试。
   - 在 `scripts/architecture.json` 里登记，这张卡只允许改三处：
     - `workspace_crates` 加上它，方括号里只写它真正依赖的纯 crate；
     - `pure_crates` 加上它；
     - `app-core` 的依赖列表加上它。
3. **app-core 同步会话。** 用一个 actor（一个 tokio 任务）来做：
   - 负责启动和停止 readboard 进程；
   - 收到的棋盘快照按 Java 的规则落到棋谱上：能接上当前局面就逐手追加，接不上就重建局面；
   - 同步期间自动分析最新局面；
   - 「总是保持棋盘一致」这个选项照 Java 实现。
4. **readboard 的位置。** 放在 `<resource_dir>/readboard/readboard.exe`，由 P1-34 放进安装包。找不到时返回类型化错误。
5. **假 readboard。** 在 `tools/fake-readboard` 里写一个跨平台的假 readboard，按录制的消息回放。集成测试用它，不依赖 Windows。
6. **接口。** Tauri 命令（开始同步、停止同步）、同步状态事件、绑定和 mock-backend。

## 不做

- readboard 替用户在客户端里落子、弈客、GMA 会话、同步诊断导出、readboard 自动更新。
- macOS 和 Linux 上的棋盘识别。

## 验收标准

1. 协议的 golden 测试，以及解析的 property 测试。
2. 集成测试，用 fake-readboard：
   - 开始同步、收到快照后，棋谱逐手追加；
   - 局面跳变时重建；
   - 停止同步后，进程退出；
   - readboard 不存在时，返回错误。
3. `scripts/verify.sh` 通过。Windows CI 跑同样的测试。
