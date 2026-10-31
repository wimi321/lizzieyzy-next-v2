---
id: P1-34
title: 安装包自带 KataGo、B11 模型和 readboard
depends_on: [P1-28, P1-33]
paths:
  - .github/workflows/**
  - scripts/packaging/**
  - apps/desktop/src-tauri/tauri.conf.json
  - apps/desktop/src-tauri/tauri.*.conf.json
  - resources/**
  - docs/ARCHITECTURE.md
---

## 目标

和 LizzieYzy Next 一样，安装包里自带 KataGo 和默认模型 B11，新用户装好就能直接分析。Windows 包另外带上 readboard。

## 要做

1. **构建时下载固定版本的组件。** 打包流程包括 Preview installers 和以后的正式发布。所有下载都按 SHA-256 校验，不一致就让构建失败。
   - KataGo：
     - 来源与 Java 版相同，即 `resources/katago/katago-assets.json` 里的 `engineReleaseRepository`、`engineReleaseTag` 和各平台资产；
     - 每个平台选 Java 版主安装包用的那一个。
   - B11 模型：用 `katago-assets.json` 里的 `downloadUrl` 和 `sha256`。
   - readboard.exe：用 Java 版 Windows 包里的那个版本，来源和哈希照 Java 仓库的打包脚本。
2. **放进资源目录。**
   - 位置：`engines/katago/<平台>/…`、`weights/default.bin.gz`、`readboard/…`，与 P1-28 和 P1-33 查找的位置一致。
   - 下载结果在 CI 里缓存。
3. **能在各平台运行。**
   - macOS：自带的 katago 和动态库在 ad-hoc 签名后仍能运行。
   - Windows：带上 KataGo 需要的运行库。
4. **许可证。** KataGo、模型和 readboard 的许可证文件一起放进安装包。

## 验收标准

1. Preview installers 在三个平台上都构建通过。解开安装包后，上面这些文件都在，哈希一致。
2. 在干净的环境里安装（没有 Java 版，也没有 Homebrew 的 KataGo），第一次启动时不弹设置窗口，直接用 B11 开始分析。
   - macOS 由维护者手动验证；
   - Windows 和 Linux 在 CI 冒烟测试里检查自动配置的结果。
