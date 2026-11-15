---
id: P3-30
title: Windows 显卡一键提速的后端：从自带的 CPU 引擎一键换到 CUDA / TensorRT / OpenCL，失败自动退回，真机测试
depends_on: [P3-29]
paths:
  - crates/app-core/**
  - apps/desktop/src-tauri/**
  - apps/desktop/src/api/**
  - apps/desktop/e2e/mock-backend.ts
  - apps/desktop/e2e/fixtures.ts
  - resources/katago/katago-assets.json
  - tools/fake-katago/**
  - docs/ARCHITECTURE.md
---

## 目标

P3 第 2 批「Windows 显卡提速的真机验收」的后端。界面在 P3-31。

现状的问题（2026-11-15 在真机上核对代码）：Windows 安装包只带 CPU（Eigen）引擎；「英伟达显卡提速组件」的安装走 `repair_engine(InstallRuntime)`，它要求当前主引擎已经是清单里的 NVIDIA `katago.exe`（按 SHA-256 识别），对自带的 CPU 引擎直接报错「selected engine does not require a managed NVIDIA runtime」。也就是说，新装的用户点「安装」会失败，界面上也没有别的入口换到显卡引擎。Java 版靠分开的 Windows 安装包解决（NVIDIA 包自带 CUDA，OpenCL 包给 AMD / Intel / 老 NVIDIA 卡），六成下载是显卡包。

另一个问题：应用内下载引擎包的地址用清单的 `engineReleaseTag`（`next-2026-10-28.1`），这个发布还是草稿，公开访问是 404，所以正式版里「安装提速组件」的下载必然失败。打包脚本用 `resources/katago/bundle-pins.json` 里已发布的 `next-2026-10-24.2` 绕过了它（同样的文件、同样的 SHA-256），应用内的下载没有。

本卡让默认安装的用户一键换到显卡引擎，并加一个真机测试，由维护者在真实的 Windows 显卡机器上跑。

Java 参考只学行为（`../lizzieyzy-next`，路径相对 `src/main/java/featurecat/lizzie/`）：

- 探测和推荐：`util/NvidiaGpuDetector.java` 131–178（nvidia-smi 查询和重试）、370–404（推荐规则、驱动版本）；
- TensorRT 的安装和启用：`util/KataGoRuntimeHelper.java` 2364–2736；`gui/KataGoAutoSetupDialog.java` 4144–4348；
- OpenCL 首次调优的等待：`Leelaz.java` 480–482、9347–9350、23935–23963（检测调优输出、延长启动时限）。

**和 Java 不同的地方**（决定见 ROADMAP 第 2 批）：

- 一个安装包加一键下载，不分包。
- 提供 CUDA、TensorRT、OpenCL 三种。DirectML、OpenVINO、ROCm 是清单里的 experimental 级别，不做。
- 新引擎启动失败（包括超时）时自动退回原来的主引擎；Java 要用户手动切回。

## 要做

1. **清单**（`resources/katago/katago-assets.json` 和 `assets/catalog.rs`）
   - `engineReleaseTag` 改成已发布的 `next-2026-10-24.2`，应用内下载和打包用同一个发布。文件和 SHA-256 不变，打包的固定版本不用动。
   - 加载 `windows-opencl`（stable 级别）作为可下载的引擎资产，和 `windows-nvidia`、`windows-tensorrt` 一样按 SHA-256 固定。
   - 加一个运行配置 `opencl`：引擎资产 `windows-opencl`，`requiredFiles` 按压缩包里实际的文件写（先下载核对一次）；需要 MSVC 运行库时带上 `runtime-msvc-runtime`。
   - 现有的 `cuda12.8-cudnn9`、`cuda12.8-cudnn9-tensorrt` 不变。
2. **提速方案**：`acceleration_plan()` 扩展为（字段名可调整，写进 ARCHITECTURE）：
   - `gpus`：探测到的显卡名字。
     - NVIDIA 照现有的 nvidia-smi 查询，加上 `driver_version`。
     - 其他显卡用固定、只读的系统查询（显示适配器列表），5 秒超时，失败时为空，不报错。不要拼接用户输入，也不要经过 shell 解析参数。
     - 远程桌面、虚拟显示器一类的适配器不算（例如名字里带 Virtual、Remote、Basic Display）。
   - `current`：当前主引擎的后端，按可执行文件的 SHA-256 对清单识别（`eigen`、`cuda`、`tensorrt`、`opencl`），认不出时为 `external`。
   - `options`：每种可用后端一项 `{ backend, profile_id, installed, download_bytes, recommended, supported, note }`。
     - 有 NVIDIA：CUDA 推荐；TensorRT 照现有的算力规则（≥ 7.5 支持，7.5..8.8 推荐，未知时允许手动）；OpenCL 支持但不推荐。
     - 没有 NVIDIA、有其他显卡：OpenCL 推荐。
     - 什么显卡都没探测到：OpenCL 支持但不推荐（虚拟机、核显驱动不全的机器上可能启动失败，失败会退回）。
     - `download_bytes` 是还没装的部分的大小。
     - 驱动低于 570.65（Java 的线）时 CUDA / TensorRT 的 `note` 标 `DriverMayBeTooOld`，仍然允许试，试的结果以启动为准。
   - 非 Windows x64 照旧返回不需要。
3. **一键开启**：新命令 `enable_acceleration(backend)`（app-core 方法加 Tauri 命令）。
   1. 用现有的资产 actor 下载这个后端的整组文件；进度、暂停、取消照旧走 `AssetDownloadStatus`。已经装好的跳过。
   2. 新的主引擎配置 = 当前主引擎的模型和分析配置 + 托管的 `katago.exe`。名字按后端起（例如「KataGo CUDA」），只改这一项，不新增引擎条目。
   3. 用现有的试启动（trial）机制启动，时限按后端：
      - CUDA：180 秒；
      - TensorRT：600 秒，第一次要构建计划缓存；
      - OpenCL：600 秒，KataGo 在调优（输出里出现调优的进度行）时每次延长 120 秒，最长 30 分钟。
   4. 启动过程发事件 `AccelerationProgress { backend, phase }`，`phase` 是 `Downloading`、`Starting`、`Tuning`、`Ready`、`Failed`。
   5. 成功：新配置成为主引擎并保存。
   6. 失败（下载出错、进程退出、超时、取消）：
      - 原来的主引擎配置不变，并重新启动；
      - 返回类型化的错误，带上 KataGo 输出的最后 20 行，例如驱动太旧、显存不足；
      - 已下载的文件保留，下次不用重下。
   7. 同时只允许一个开启过程；正在测速或另一个试启动时拒绝（类型化错误）。
4. **改回 CPU**：`disable_acceleration()` 等于现有的 `UseBundledProgram` 修复（模型和配置不变），给界面一个不需要先诊断的入口。
5. **现有的 `InstallRuntime` 修复**（主引擎已经是托管的显卡引擎、只是缺运行库）保持可用。
6. **一次性的提示**：设置里加 `acceleration_hint_dismissed`（默认 false，旧设置文件缺字段时为 false），命令 `dismiss_acceleration_hint()`。界面在 Windows 上、当前是 CPU 引擎、方案里有推荐项时提示一次（P3-31）。
7. **fake-katago**：加选项，模拟：
   - 启动时打印后端行；
   - 若干行调优进度后才就绪；
   - 启动失败并打印错误。
   测试只用 fake-katago 和本地 HTTP 服务器上的假清单，不连网。
8. **真机测试**：`crates/app-core/tests/acceleration_live.rs`，`#[ignore]`，维护者手动运行，不进 verify。
   - 环境变量：
     - `LIZZIE_LIVE_ACCEL=cuda|tensorrt|opencl`；
     - `LIZZIE_LIVE_MODEL`：模型文件；
     - `LIZZIE_LIVE_CPU_ENGINE`：自带的 CPU 引擎；
     - `LIZZIE_LIVE_DATA`：可选的数据目录；给了就复用已经下载的文件，否则用临时目录。
   - 步骤：
     1. 以 CPU 引擎和模型建好主引擎；
     2. 打印 `acceleration_plan()`；
     3. `enable_acceleration`，打印每个阶段和用时；
     4. 用新引擎分析空棋盘到 1000 访问，打印每秒访问数、KataGo 报告的后端行；
     5. `disable_acceleration`，确认回到 CPU 引擎，同样分析一次，打印每秒访问数做对比。
   - 断言：
     - 第 3 步成功；
     - 第 4 步的后端行和所选后端一致；
     - 第 5 步的主引擎是 CPU 引擎。
   - 卡里不要求在 CI 跑；汇报里写清楚命令。

## 不做

- 界面（P3-31）。
- DirectML、OpenVINO、ROCm；macOS、Linux 的显卡后端。
- 下载镜像。真机上 NVIDIA 的 .com 和 .cn 速度相同，以后有需要再加。
- 安装包的改动（Windows 仍然只带 CPU 引擎）。

## 验收标准

- `select` 的纯函数测试覆盖：NVIDIA（各算力、未知算力、旧驱动）、只有其他显卡、什么都没有、非 Windows。
- 用 fake-katago 和本地假清单：
  - 从 CPU 引擎一键开启成功，主引擎换成托管的 `katago.exe`，模型和配置不变，设置已保存；
  - 启动失败和超时都退回原来的主引擎，错误带 KataGo 的最后几行，已下载的文件还在；
  - 调优行让时限延长，不调优时按原时限超时（用 `tokio::time::pause`，不真的等）；
  - 开启过程中再开启、开启时测速，都被拒绝；
  - `disable_acceleration` 回到自带引擎。
- `acceleration_hint_dismissed`：旧设置文件读成 false，`dismiss_acceleration_hint` 之后保存为 true。
- 新命令只授权给主窗口；生成的 TS 类型和 mock 同步。
- `cargo test -p app-core --test acceleration_live -- --ignored` 在没有设环境变量时立即跳过并说明原因，不失败。
- `scripts/verify.sh` 通过；`check_budgets.py` 通过。
