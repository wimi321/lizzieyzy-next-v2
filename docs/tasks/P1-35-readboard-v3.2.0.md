---
id: P1-35
title: readboard 升级到 v3.2.0
depends_on: [P1-34]
paths:
  - resources/katago/bundle-pins.json
  - resources/licenses/**
  - scripts/packaging/**
  - docs/tasks/P1-35-readboard-v3.2.0.md
  - docs/ROADMAP.md
  - docs/ARCHITECTURE.md
---

## 目标

维护者 2026-10-31：「那就直接用 readboard 的最新版本吧，作者本来也同意用」。

Windows 安装包改为打包 readboard 最新正式版 v3.2.0（2026-10-13 发布）。P1-33 的协议实现参照的是 readboard 提交 a292eea，只比 v3.2.0 多 3 个提交；P1-34 原先打包的 v3.0.6 落后 118 个提交，升级后两边对齐。readboard 作者同意随新版分发。

## 要做

- `bundle-pins.json` 的 readboard 改为 v3.2.0：
  - 资产 `readboard-webview2-v3.2.0.zip`；
  - 大小 126,901,426 字节；
  - SHA-256 `6abb1c27…`，与 GitHub 记录的值一致；
  - 源码提交 `b82039d`。
  - 归档的顶层目录仍是 `readboard/`。
- v3.2.0 自己的清单已经是 `asInvoker`，`requireAdministrator` 只出现在注释里。所以「管理员权限改为普通权限」这步补丁改成由钉选信息决定，对 v3.2.0 关闭，exe 原样打包。
- 来源说明改为「经作者同意随 LizzieYzy Next 2 分发」。

## 验收标准

1. 打包测试通过，并覆盖「关闭补丁时 exe 原样打包」。
2. 在本分支上跑 Preview installers，Windows 构建通过，按清单核对后 `readboard/readboard.exe` 来自 v3.2.0。
3. `scripts/verify.sh` 通过（CI）。
