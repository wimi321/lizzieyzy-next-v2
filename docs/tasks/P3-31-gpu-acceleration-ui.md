---
id: P3-31
title: Windows 显卡一键提速的界面：显卡提速页、开启和退回、首次提示
depends_on: [P3-30]
paths:
  - apps/desktop/src/**
  - apps/desktop/e2e/**
---

## 目标

P3 第 2 批的界面，用 P3-30 的命令和事件。Java 参考：`gui/KataGoAutoSetupDialog.java` 4109–4348、`gui/TensorRtAccelerationView.java`（文案和推荐规则的说法）。

## 要做

1. **引擎工具 → 显卡提速**（原「英伟达显卡提速组件」页，标签改名）
   - 顶部：探测到的显卡（NVIDIA 带驱动版本）、当前引擎的后端（CPU / CUDA / TensorRT / OpenCL / 外部引擎）。
   - 选项：每种后端一行，单选，显示「推荐」、要下载的大小（已安装显示「已安装」）、不支持的置灰并说明原因、`DriverMayBeTooOld` 时显示驱动可能太旧但可以试。默认选中推荐项；当前在用的后端标「正在使用」。
   - 「开启提速」：调用 `enable_acceleration`。
     - 下载中照现有的进度条（字节、暂停、取消）。
     - 启动中显示「正在启动 …」和已用时间。
     - 调优中显示「第一次使用 OpenCL 要先调优，可能需要几分钟，请不要关闭软件」和已用时间。
   - 成功：「已改用 X」，给一个「测一下速度」按钮，跳到性能优化页。
   - 失败：「没有成功，已退回原来的引擎」，加上错误说明；KataGo 的最后几行放在可以展开的区域里，可以复制。
   - 当前是显卡引擎时，显示「改回 CPU」（`disable_acceleration`）。
   - 非 Windows 或没有可用项时，照现有的说明文字，不显示按钮。
2. **首次提示**：Windows 上、当前是 CPU 引擎、方案里有推荐项、`acceleration_hint_dismissed` 为 false 时，启动后在主窗口显示一次不挡操作的提示：「检测到 X 显卡，开启显卡提速后分析会快很多」。按钮：「去开启」（打开显卡提速页）、「不再提示」（`dismiss_acceleration_hint`）。关掉提示也算不再提示。
3. 中文、英文文案；其他语言用英文占位，照现有的做法。

## 不做

- 后端的改动（P3-30）。
- DirectML、OpenVINO、ROCm。

## 验收标准

- Vitest：
  - 各种方案的显示：NVIDIA 推荐 CUDA、TensorRT 可选、旧驱动、只有其他显卡、什么都没有；
  - 开启的各阶段和成功、失败（含展开的 KataGo 输出）；
  - 改回 CPU；
  - 首次提示的出现条件、「去开启」「不再提示」。
- Playwright：开启成功（下载 → 启动 → 调优 → 成功）、失败退回各一条，失败那条带截图基线。
- `scripts/verify.sh --e2e` 通过；`check_budgets.py` 通过。
- 真机：P3-30 的真机测试在 Windows 显卡机器上通过以后，由坐在那台机器前的维护者用正式构建点一遍：从 CPU 引擎开启 CUDA、换 TensorRT、换 OpenCL、改回 CPU，各截一张窗口图。远程验收的一方不操作那台机器的鼠标和键盘。
