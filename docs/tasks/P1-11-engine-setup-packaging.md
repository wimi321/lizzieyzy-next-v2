---
id: P1-11
title: 最小引擎设置与三平台打包
depends_on: [P1-08]
paths:
  - apps/desktop/src/settings/**
  - apps/desktop/src/locales/*.json
  - apps/desktop/src/App.tsx
  - apps/desktop/src-tauri/**
  - apps/desktop/e2e/**
  - crates/app-core/src/engine_setup/**
  - crates/app-core/src/lib.rs
  - resources/katago/**
  - .github/workflows/**
---

## 目标

用户第一次打开就能接上 KataGo；三个平台都出预览安装包。

## 要做

- 引擎探测，放在 app-core 的 `engine_setup` 模块，查找以下位置：
  - PATH 里的 `katago`。
  - Homebrew：`/opt/homebrew` 和 `/usr/local` 下的 `bin/katago` 与 `share/katago/*.bin.gz`。
  - 应用目录旁边的 `katago/`。
  - Windows 上的常见位置。
- 首次设置界面：
  - 显示探测结果，允许手动选择程序、模型、配置。
  - 配置默认把 `resources/katago/analysis.cfg` 复制到应用配置目录后使用。
  - 先把引擎启动到 Ready，再保存设置。
- `.github/workflows/preview.yml`（手动触发）：构建未签名的安装包，作为 artifact 上传，包括 Windows nsis、macOS dmg、Linux AppImage / deb。P1 不捆绑 KataGo。

## 验收标准

1. 探测逻辑的单元测试用临时目录模拟各种目录结构，不依赖本机的真实文件。
2. 在本机（Homebrew KataGo）首次启动时，能自动探测到引擎并进入 Ready（Claude 验收时实测）。
3. preview workflow 在三个平台都产出安装包（附运行链接）。
4. macOS 安装包能走通 ACCEPTANCE.md 中 P1 手工冒烟的第 1–6 步。
