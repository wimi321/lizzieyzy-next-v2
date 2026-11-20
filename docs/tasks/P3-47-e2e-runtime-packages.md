---
id: P3-47
title: 工程线：Linux 桌面端 E2E 不再现场从 Ubuntu 镜像下载运行库
depends_on: [P3-46]
paths:
  - .github/workflows/ci.yml
  - apps/desktop/e2e/tauri/**
  - docs/ARCHITECTURE.md
---

## 目标

Linux 桌面端 E2E（`ci.yml` 的 `e2e` 任务）开始时用 apt 装 `libwebkit2gtk-4.1-0`、`webkit2gtk-driver`、`xvfb` 等运行库。2026-11-19，runner 用的 Ubuntu 镜像（`us-east-2.ec2.archive.ubuntu.com`）非常慢，这个任务在 7 个 PR 上失败，E2E 一步都还没开始：
- 前几次是超出 5 分钟上限；
- #146 把 apt 的时间排除在 300 秒预算之外、任务上限放宽到 10 分钟以后，#148 上仍然有一次 apt 下载 25.5 MB 的 libwebkit2gtk 用了 4 分半还没下完，任务在 10 分钟时被取消（run 44662747349，job 136058138683）。

同一时间，Verify 任务在 Linux 上装 `libwebkit2gtk-4.1-dev` 等依赖也要下载这些包，它没有这么紧的时间限制，能装完。

## 要做

1. 运行库的 `.deb` 只在 Verify 任务里下载一次，放进已有的 `tauri-e2e-Linux` artifact，E2E 任务从 artifact 安装，不再访问 Ubuntu 镜像。
   - 在 Verify 任务装开发依赖之前，用 `apt-get install --download-only` 把 E2E 需要的运行库（现在 E2E 任务 apt 装的那几个包）连同全部依赖下载到单独的目录。下载要在干净的镜像状态下进行，这样依赖是完整的。可以用 `-o Dir::Cache::archives=…`，也可以用别的确定办法。
   - E2E 任务用 `sudo apt-get install --no-install-recommends -y <目录>/*.deb` 装，不运行 `apt-get update`，不访问网络。装完检查 `webkit2gtk-driver` 和 `xvfb-run` 都在。
   - Ubuntu 镜像是同一个 runner 映像（`ubuntu-24.04`），两边的已安装包一致。不一致导致安装失败时，任务要报出清楚的错误，不要退回去在线下载。
2. 检查 artifact 的大小（现在只有几个二进制），加上 .deb 以后在汇报里写出来。上传和下载都在 GitHub 内部，应该比 Ubuntu 镜像快得多。
3. #146 加的 apt 计时保留，E2E 的 300 秒预算照旧扣除安装时间；任务上限改回较小的值（比如 6 分钟），汇报里说明取了多少、为什么。
4. Windows 的 E2E 不变。
5. ARCHITECTURE 测试部分补一句：Linux 桌面端 E2E 的运行库由 Verify 任务下载，随 artifact 传过去，E2E 任务不访问 Ubuntu 镜像。

## 不做

- 改 Verify 任务原有的开发依赖安装（只在它之前多加一步下载）。
- 换 apt 镜像源、加第三方缓存 action。

## 验收标准

- PR 上 Linux 桌面端 E2E 通过，日志里 E2E 任务没有访问 `archive.ubuntu.com`，安装步骤在几十秒内完成（汇报里写实际秒数）。
- Verify 任务照常通过。
- `check_budgets.py` 通过。本卡只改工作流和文档，`scripts/verify.sh` 只需要确认没有受影响。
