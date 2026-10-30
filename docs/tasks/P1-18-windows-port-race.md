---
id: P1-18
title: Windows E2E 辅助测试：端口释放竞态
depends_on: [P1-15]
paths:
  - apps/desktop/e2e/tauri/**
---

## 目标

`apps/desktop/e2e/tauri/windows-app.test.ts` 里的 "cleanup reports an occupied former CDP port without killing its new owner" 在 Windows CI 上偶发失败。PR #20 的一次运行报 `listen EADDRINUSE: address already in use 127.0.0.1:57067`，同一提交的另一次运行通过。

原因：测试让模拟应用退出，确认进程已经不在后，立刻在同一端口上监听"新占用者"。Windows 上进程退出后，监听端口要稍等一会儿才释放，两者撞上就失败。

## 要做

- "新占用者"遇到 EADDRINUSE 时有界重试，等端口真正释放后再监听，总时限不超过 5 秒，超时才判失败。不要用固定 sleep。
- 检查 `e2e/tauri/` 里其他"进程刚退出就复用它的端口"的测试，有同样问题的一起改。
- 只改测试代码。如果发现被测的辅助实现本身也有这个竞态，在汇报里说明，不要顺手改。

## 验收标准

1. 本机 `pnpm -C apps/desktop e2e:tauri:check` 连续 20 遍全部通过。
2. Windows CI 的 Desktop E2E 通过。维护者合并前在 CI 上再重跑 3 次，都要通过。
