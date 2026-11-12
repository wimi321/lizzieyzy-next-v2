---
id: P2-12
title: 引擎和设置（二）后端：官方权重下载和切换、显卡提速组件、路径诊断与修复
depends_on: [P2-11]
paths:
  - crates/app-core/**
  - crates/engine-session/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
  - resources/katago/**
  - docs/ARCHITECTURE.md
---

## 目标

P2 第 4 批的后一半：「设置 → 引擎一键设置」里的权重管理、英伟达显卡提速组件和路径诊断。下载的东西会被执行或加载，按 AGENTS.md 一律用 SHA-256 校验。Java 参考：`../lizzieyzy-next` 里引擎一键设置的权重管理、提速组件和概览页，以及共用的 `resources/katago/katago-assets.json`（新版的那份在 `resources/katago/`）。

## 要做

1. **官方权重。** 按 `katago-assets.json` 列出官方权重（名称、大小、发布日期、是否已下载）。下载：进度、取消、断点续传或失败后重来、SHA-256 校验不过就删掉并报错；放在应用数据目录。下载好的权重可以一键设为当前主引擎的权重（走 P2-11 的切换流程，失败回退）。
2. **显卡提速组件。** Windows 上有英伟达显卡时，按清单下载对应的 KataGo 运行组件（照 Java 的选择规则和清单字段），同样校验；其他平台和没有英伟达显卡时说明不需要。
3. **路径诊断与修复。** 对当前主引擎检查：程序是否存在、能否执行、版本；权重是否存在、能否读；分析配置是否存在、是否有效。每项给出结果和能做的修复（例如重新生成分析配置、改用自带的程序或权重），修复是单独的命令，执行后重新诊断。
4. 新字段、新命令照约定：生成的 TS 里可选，mock 补默认值，命令在 mock 里是脚本化的 `command`；下载进度用事件推给界面。

## 不做

- 界面。

## 验收标准

1. 权重下载：进度事件、取消、校验失败删文件、重复下载，各有测试；测试里用本地 HTTP 服务或注入的下载器，不连真网络。
2. 提速组件的选择规则有测试（有/无英伟达显卡、各平台）。
3. 诊断每一项的正常和出错情况、每个修复命令，有测试（用 `tools/fake-katago`）。
4. 新依赖照规矩写一行理由；`docs/ARCHITECTURE.md` 更新。
