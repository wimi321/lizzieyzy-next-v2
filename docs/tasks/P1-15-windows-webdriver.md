---
id: P1-15
title: Windows 原生 E2E 与 WebView2 启动排查
depends_on: [P1-12]
paths:
  - apps/desktop/src-tauri/**
  - apps/desktop/e2e/**
  - .github/workflows/**
  - docs/ARCHITECTURE.md
---

## 目标

让 P1-12 的原生 E2E 在 Windows 上也跑通，并确认 Windows 上 WebView2 能正常初始化，用户打开应用不会看到空白窗口。

## 已知情况

来自 PR #18 的诊断（run 41008601246）：

- Edge、WebView2 Runtime、msedgedriver 版本一致，都是 155.0.4283.62。
- msedgedriver 以 `browserName: webview2` 启动应用。`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` 里含 `--remote-debugging-port=0`，但没有 `--user-data-dir`。等满 60 秒后报 `DevToolsActivePort file doesn't exist`，然后杀掉应用。
- 启动参数已经改成 `--config-dir=…`、`--open=…`，应用不会再因为参数错误退出。
- 同一套测试在 Ubuntu（WebKitWebDriver）上能通过。

## 要做

- 先确认 WebView2 有没有初始化：不经过 WebDriver，直接启动应用，然后检查以下几项，并截屏上传：
  - `%LOCALAPPDATA%\io.github.wimi321.lizzieyzy.next2\EBWebView` 是否创建；
  - 里面有没有 `DevToolsActivePort`；
  - 进程是否存活。
- 按结果修复：
  - 如果是应用设置的用户数据目录或浏览器参数让 msedgedriver 找不到调试端口，就让 WebDriver 场景下的 WebView2 用一个 msedgedriver 能找到的位置；正常使用不受影响。
  - 如果 WebView2 根本没初始化，这是产品缺陷，优先修复。
- 恢复 `ci.yml` 中 e2e 矩阵的 `windows-latest`。

## 验收标准

1. CI 的 Desktop E2E 在 ubuntu 和 windows 上都通过，总时长在 5 分钟以内。
2. 汇报里说明根因和依据。
