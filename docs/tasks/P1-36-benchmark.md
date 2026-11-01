---
id: P1-36
title: KataGo 性能优化（用户手动测速）
depends_on: [P1-29]
paths:
  - crates/app-core/**
  - crates/katago-protocol/**
  - crates/engine-session/**
  - tools/fake-katago/**
  - fixtures/katago/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/**
  - apps/desktop/e2e/**
  - docs/ARCHITECTURE.md
---

## 目标

维护者 2026-10-31：「一键测速 还是留个用户自己点击吧，安装好直接测速，用户不能立刻用，有点影响用户体验」。

照 LizzieYzy Next「设置 → 引擎一键设置 → 性能优化」里的「官方快速优化」来做：

- 用户点「开始优化」后，用当前引擎的程序和模型跑一次 KataGo 官方 benchmark，让它自动搜索线程数；
- 只采用它推荐的搜索线程数；
- 界面显示「实际搜索速度」和「NN 计算速度」。

**不自动运行。** 首次启动照常直接分析。

分工：后端由 Codex 做。性能优化窗口和菜单接线由 Claude 在同一分支上做。Codex 在前端只改 `src/api/` 和 mock-backend。

## 要做

1. **行为参照 Java 版：**
   - `gui/KataGoAutoSetupDialog.java` 的性能优化页；
   - `util/katago/tuning/` 下的类；
   - `KataGoRuntimeHelper` 里与 benchmark 有关的部分；
   - 以及它们的测试。

   官方快速优化用什么参数（visits、局面数等），照 Java。
2. **一次优化的流程。** app-core 新增「开始、取消、读取状态和上次结果」三个命令，加一个进度事件：
   - 开始时暂停当前所有分析，包括实时分析和整盘分析。结束或取消后，恢复原来的分析状态。
   - 单独起一个 `katago benchmark` 进程，不经过分析会话，也不是 GTP。配置文件的给法照 Java（GTP 风格配置或 override），写进汇报。
   - 解析输出：每个线程数的进度（第几个局面 / 共几个）、visits/s、nnEvals/s，以及最后推荐的线程数。
   - 用维护者 Mac 上录到的真实输出（B11、Metal）做 golden 测试，放进 `fixtures/katago/`。
3. **保存。** 结果按引擎指纹存进设置，包括：
   - 推荐线程数；
   - 实际搜索速度（visits/s）；
   - NN 计算速度（nnEvals/s）；
   - 测速时间；
   - 模型规范名。
4. **生效与恢复。**
   - 推荐线程数对应到分析引擎的哪个参数（例如 `numSearchThreadsPerAnalysisThread`），映射要说明理由。用 override 重启分析引擎生效。
   - 提供「恢复默认」，清除优化结果。
5. **fake-katago。** 支持 `benchmark` 子命令，能回放录制的输出、控制时长。集成测试用它。
6. **接口。** Tauri 命令、进度事件、ts-rs 绑定和 mock-backend。

## 不做

- 实验性深度调优（GPU/ANE 拓扑、批量大小）。
- TensorRT、OpenCL 组件。
- 首次启动自动测速。

## 验收标准

1. 解析的 golden 测试和 property 测试。
2. 集成测试（fake-katago）覆盖以下流程：
   - 开始 → 进度 → 完成并保存结果 → 引擎按新的线程数重启 → 分析恢复；
   - 取消时，benchmark 进程退出，分析恢复，设置不变；
   - 失败时返回原因。
3. `scripts/verify.sh` 通过，绑定一致性测试通过。
