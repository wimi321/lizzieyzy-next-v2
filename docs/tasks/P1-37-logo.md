---
id: P1-37
title: 新 logo
depends_on: [P1-36]
paths:
  - apps/desktop/src-tauri/icons/**
  - apps/desktop/src-tauri/tauri.conf.json
  - apps/desktop/src/**
  - apps/desktop/e2e/**
  - docs/tasks/P1-37-logo.md
  - docs/ROADMAP.md
---

## 目标

维护者 2026-10-31：「把 logo 也做一下吧，现在的太丑了」。界面由 Claude 做。

## 要做

- **设计思路。** 保留 LizzieYzy Next 原图标的辨识要素（棋盘格、黑白子、表示分析的元素），但要做减法：
  - 榧木色圆角底；
  - 两横两竖的「#」形棋盘线；
  - 一白一黑两颗子落在对角的两个交叉点上；
  - 第三个交叉点放绿色的 AI 推荐标记，取代原图标的橙色弧线。
- **源文件。** 矢量源稿 `apps/desktop/src/assets/logo.svg`，同时用于「关于」窗口。
- **图标生成。** 用 `tauri icon` 从 1024 像素的渲染图生成桌面所需的各个尺寸：
  - 删除 Android、iOS 和 Windows 商店用的图标；
  - 大尺寸 PNG 重新压缩；
  - icns 照原来的条目结构重新打包，以满足单文件 1 MB 的限制。
- **配置。** `tauri.conf.json` 列出 icns、ico 和各尺寸 PNG。

## 验收标准

- macOS、Windows、Linux 三个平台的安装包都使用新图标。Preview installers 构建通过。
- 「关于」窗口显示新 logo。截图基线跟着更新。
