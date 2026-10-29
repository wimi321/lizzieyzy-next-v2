---
id: P1-08
title: Tauri 层：命令、类型生成、分析流
depends_on: [P1-07]
paths:
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - Cargo.toml
  - docs/ARCHITECTURE.md
---

## 目标

把 app-core 接到 Tauri，前端拿到类型安全的命令和事件流。

## 要做

- 命令放在 `src-tauri/src/commands/{document,analysis,engine,settings}.rs`，每个命令只做参数转换并调用 AppCore。
- 启动时在应用配置目录创建 AppCore。支持两个启动参数：`--config-dir <路径>`（便携版和 E2E 用）；把棋谱路径作为启动参数直接打开。
- 文档变化、分析、引擎状态这三类事件流用 `tauri::ipc::Channel` 推送。
- 绑定生成到 `apps/desktop/src/api/generated/`。写一个 cargo test：把绑定生成到临时位置，再和已提交的文件比对，不一致就失败。
- `src/api/backend.ts` 对生成的绑定做薄封装（订阅和取消订阅）；`e2e/mock-backend.ts` 实现同样的接口。
- 权限：主窗口只开放本应用的命令和打开 / 保存对话框（dialog 插件），不开放 shell、fs 这类宽泛权限。

## 验收标准

1. 绑定一致性测试通过；汇报里演示：改一个 DTO 字段却不重新生成时，测试会失败。
2. 用 Tauri 的 mock runtime 写测试：调用打开命令后，Channel 上依次收到 DocumentChanged 和 AnalysisUpdated（用 fake-katago）。
3. 每个命令文件不超过 15 个命令，每个命令体不超过约 30 行（审查）。
4. capability 文件只列必要的权限（审查）。
