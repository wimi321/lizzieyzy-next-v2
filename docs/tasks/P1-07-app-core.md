---
id: P1-07
title: app-core：无界面的应用内核
depends_on: [P1-06]
paths:
  - crates/app-core/**
  - tools/fake-katago/**
  - Cargo.toml
  - docs/ARCHITECTURE.md
---

## 目标

按 ARCHITECTURE.md 4.7 做出不依赖 Tauri 的应用内核，主流程在 Rust 测试里就能完整跑通。

## 要做

- `AppCore` actor 持有 `GameDocument`、协调器句柄和设置，命令串行处理。命令包括：打开（字节或路径）、保存、落子、Step / GoTo、暂停 / 恢复分析、配置引擎档案。
- 事件：`DocumentChanged { revision, view }`、`AnalysisUpdated(AnalysisView)`、`EngineStatusChanged`。
- 视图 DTO：`BoardView`、`TreeView`（主线加当前节点的变化数）、`AnalysisView`，数值用黑方视角。
- DTO 派生类型生成用的 trait：specta 或 ts-rs 二选一，汇报里说明理由；P1-08 沿用这个选择。
- 设置：JSON 格式，带版本号。缺字段用默认值，多余字段忽略。设置目录由调用方传入，为便携版和 E2E 留口子。
- 引擎档案：程序、模型、配置路径、额外参数。
- 启动 KataGo 时总是追加 `-override-config reportAnalysisWinratesAs=BLACK`。用户可能选自己的配置文件，而 KataGo 没写这一项时默认按轮走方视角报数。fake-katago 要接受 `-override-config`，并加一个参数模拟"配置里没写这一项"的引擎（按轮走方视角报数），用来测这一条。
- 协调器的引擎指纹由引擎档案生成：程序、模型、配置文件的路径、大小和修改时间，加上额外参数。档案变了就换一个新的协调器。

## 验收标准

1. 无界面集成测试（用 fake-katago）：打开语料 → 收到 DocumentChanged → 收到根节点的 AnalysisUpdated → 连发 20 次 Step(+1) → 最后一次之后的所有 AnalysisUpdated 都属于最新的 revision。
2. 保存后再打开，树完全相同。
3. 设置能保存和读取；测试里构造的旧版本设置文件也能读。
4. 引擎配错（模型不存在）时，发出带可读原因的 EngineStatusChanged(Failed)，导航照常工作。
5. fake-katago 模拟按轮走方视角报数的引擎时，经 app-core 启动后，白方轮走局面的 AnalysisView 数值仍然是黑方视角。
6. `cargo test -p app-core` 连续跑 10 遍，再在 CPU 满载下（按核数启动 `yes > /dev/null`）连跑 10 遍，全部通过。
