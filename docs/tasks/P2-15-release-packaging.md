---
id: P2-15
title: 发布（二）：三平台安装包、macOS 签名与公证、更新清单的发布流程
depends_on: [P2-14]
paths:
  - .github/workflows/**
  - scripts/packaging/**
  - apps/desktop/src-tauri/tauri.conf.json
  - apps/desktop/src-tauri/Info.plist
  - apps/desktop/src-tauri/entitlements.plist
  - docs/ARCHITECTURE.md
---

## 目标

P2 公开预览版的发布通道：打一个版本标签，CI 产出三平台安装包、签好名，并生成 P2-14 检查更新用的 `latest.json`，挂到 GitHub Releases 的草稿上，由维护者确认后发布。

## 要做

1. **发布工作流**（新文件，例如 `.github/workflows/release.yml`）：由 `v*` 标签或手动触发；Windows 出 NSIS 安装器和便携版，macOS 出 DMG（Apple Silicon 与 Intel，或通用包），Linux 出 AppImage 和 deb；复用现有 `preview.yml` 和 `scripts/packaging/` 的打包步骤，不另写一套。
2. **macOS 签名与公证**：用 Developer ID 证书签名、硬化运行时、`notarytool` 公证并 staple。证书、密码、Apple ID、团队 ID 都从 secrets 读；**secrets 不存在时跳过签名和公证并在日志里明确说明**，工作流照样产出未签名的包（预览可用）。自带的 KataGo、readboard 等可执行文件一起签。
3. **Windows**：有代码签名证书的 secrets 时签名，没有时跳过并说明。
4. **更新清单**：用 updater 私钥（secret）给每个安装包签名，生成 `latest.json`（版本、发布说明、日期、各平台的 URL 和签名），作为 Release 资产上传。私钥 secret 不存在时跳过并说明。
5. 在 `docs/ARCHITECTURE.md` 写清楚维护者需要配置哪些 secrets、各自的格式，以及怎样在本地验证一个签过名的包。

## 验收标准

1. 在没有任何 secrets 的分支上手动触发：三平台都产出安装包，日志写明跳过了哪些签名步骤；`latest.json` 的生成逻辑有脚本测试（`scripts/packaging/` 下，用测试密钥）。
2. 现有的 `preview.yml` 和 CI 不受影响。
3. 打包脚本的改动有测试（沿用 `scripts/packaging/test_*.py` 的写法）。
