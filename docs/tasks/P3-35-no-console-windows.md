---
id: P3-35
title: 工程线：Windows 正式版启动引擎等子进程时不弹出控制台窗口
depends_on: [P3-29]
paths:
  - crates/engine-session/src/transport/**
  - crates/engine-session/src/transport.rs
  - crates/engine-session/tests/**
  - crates/engine-session/Cargo.toml
  - docs/ARCHITECTURE.md
---

## 目标

Windows 正式版是 GUI 程序（`windows_subsystem = "windows"`），本身没有控制台。它启动的控制台程序会各自得到一个新的控制台窗口：KataGo、GTP 引擎、测速、引擎版本探测、readboard，以及 P3-30 的 `nvidia-smi` 和 PowerShell 显卡查询。用户会看到黑色的命令行窗口弹出来，关掉它还会杀掉引擎。

Java 版没有这个问题：Java 在 Windows 上启动子进程时总是带 `CREATE_NO_WINDOW`。

生产代码里所有子进程都经过 `engine_session::spawn_process`（`crates/engine-session/src/transport/spawn.rs`）。2026-11-15 核对过：其余的 `Command::new` 只在测试、工具或仅 Linux 的 `xdg-open` 里。所以只改这一个地方。

## 要做

1. `spawn_process` 在 Windows 上给命令加 `CREATE_NO_WINDOW`（`0x0800_0000`）。
   - 用 tokio `Command` 自带的 `creation_flags`。
   - 不覆盖调用方已经设置的其他标志：tokio 没有读取接口，就在文档注释里写明，这个函数是唯一设置标志的地方。
   - 其他平台不变。
   - 现有的 Linux ETXTBSY 重试不变。
2. ARCHITECTURE 里子进程的说明加一句：Windows 上所有子进程都不带控制台窗口，由 `spawn_process` 统一保证。新的启动子进程的代码必须经过它。
3. 测试（只在 Windows 上编译和运行，其他平台不编译）。
   - 子进程通过 `spawn_process` 启动，报告 `GetConsoleWindow()` 是否为空，测试断言为空。
   - 对照组：同一个子进程用普通的 `Command` 从测试进程（控制台程序）启动，报告非空。这证明测试能分辨两种情况。
   - 子进程用什么由你选：
     - 可以是 PowerShell 一行（`Add-Type` 调 `kernel32!GetConsoleWindow`）；
     - 也可以是 `tools/fake-katago` 的一个开关，但那样要扩大范围，先在汇报里说明。
   - 不加新的依赖：`windows-sys` 已经在 `Cargo.lock` 里，确实需要时可以作为 engine-session 仅 Windows 的开发依赖，汇报里写理由。

## 不做

- readboard 自己的窗口（它是 GUI 程序，本来就有窗口，`CREATE_NO_WINDOW` 不影响它）。
- macOS、Linux。

## 验收标准

- Windows 上的新测试在改之前失败、改之后通过（汇报里写怎么确认的；本机是 macOS，可以把「改之前」的验证留给 CI 的 Windows 任务和维护者的 Windows 真机，汇报里说明）。
- `scripts/verify.sh` 通过；`check_budgets.py` 通过；三个平台的 clippy 都通过。
