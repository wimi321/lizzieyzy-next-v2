---
id: P3-22
title: 读取剪贴板里的链接，给「打开在线链接」预填
depends_on: [P3-19]
paths:
  - crates/app-core/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
---

## 目标

Java 版打开「打开在线链接」时，剪贴板里如果是链接，就预先填进输入框（`gui/OnlineDialog.java` 404 行附近）。新版的对话框（P3-19 的 `OpenUrlDialog`）已经能接收预填值，还缺一个读剪贴板的后端命令。界面在 P3-20 接上。

## 要做

1. **`clipboard_link() -> Option<String>`**（app-core）：
   - 用已有的 `Clipboard` 抽象读文本，和 `paste_sgf` 走同一个入口。
   - 去掉两端的空白和成对的引号。
   - 只有是一个 http / https 链接时才返回：有 host、没有 user-info、不超过 4 KiB、中间没有空白或换行。否则返回 `None`。
   - 剪贴板不可用或没有文本时也返回 `None`，不报错。
   - 剪贴板内容不写进日志。
2. **Tauri 命令**，只授权本地主窗口；生成的 TS；`apps/desktop/src/api/` 里的封装和测试。
3. **mock**：做成默认查询（返回 `null`），这样不读剪贴板的现有测试不用改。

## 不做

- 界面（P3-20）。
- 不把任意剪贴板文本交给网页：只返回通过上面检查的链接。

## 验收标准

1. 测试：http 和 https 链接会返回（带两端空白和引号的也会）；普通文字、`ftp://`、带 user-info、超过 4 KiB、多行、剪贴板不可用、没有文本，都返回 `None`。
2. `scripts/verify.sh --e2e` 通过。
