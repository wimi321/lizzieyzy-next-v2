---
id: P1-11
title: 引擎探测、首次设置后端与三平台打包
depends_on: [P1-08]
paths:
  - crates/app-core/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - resources/katago/**
  - .github/workflows/preview.yml
  - docs/ARCHITECTURE.md
---

## 目标

用户第一次打开就能接上 KataGo，三个平台都出预览安装包。首次设置的界面是 P1-14（Claude 做），这张卡只做它要用的后端和打包。

## 要做

- 引擎探测，放在 app-core 的 `engine_setup` 模块。查找以下位置：
  - PATH 里的 `katago`。
  - Homebrew：`/opt/homebrew` 和 `/usr/local` 下的 `bin/katago`，以及 `share/katago/*.bin.gz`。
  - 应用目录旁边的 `katago/`。
  - Windows 上的常见位置。
- 探测结果列出找到的程序、模型、配置。模型按推荐程度排序：较新的 kata1 网络排在 g170 这类老网络前面。排序规则写进 ARCHITECTURE.md。
- `resources/katago/analysis.cfg`：应用自带的默认分析配置。档案没有指定配置时，把它复制到设置目录后使用；已经存在的副本不覆盖。打包时作为 Tauri 资源带上。
- 先试后存：用一个档案启动引擎，等到 Ready 才保存为当前设置并切换过去。启动失败（Failed）或超时，就返回带可读原因的错误，原来的设置和正在运行的引擎都不变。
- Tauri 命令：探测、先试后存。重新生成 TS 绑定，mock-backend 跟着补上新命令。
- `.github/workflows/preview.yml`，手动触发：构建未签名的安装包（Windows nsis、macOS dmg、Linux AppImage 和 deb）并作为 artifact 上传。P1 不捆绑 KataGo。

## 验收标准

1. 探测逻辑的单元测试用临时目录模拟各种目录结构，不依赖本机的真实文件。
2. 先试后存（fake-katago）：档案正确时进入 Ready 并保存；模型不存在时返回可读的错误，设置文件和正在运行的引擎都不变。
3. preview workflow 在三个平台都产出安装包（附运行链接）。
4. `cargo test -p app-core` 和桌面 crate 的测试各连续跑 10 遍，再在 CPU 满载下连跑 10 遍，全部通过。
