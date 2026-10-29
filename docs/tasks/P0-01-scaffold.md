---
id: P0-01
title: 仓库骨架、lint 与 CI 接入验收门禁
depends_on: []
paths:
  - Cargo.toml
  - rust-toolchain.toml
  - rustfmt.toml
  - clippy.toml
  - .editorconfig
  - .gitignore
  - .gitattributes
  - README.md
  - LICENSE
  - crates/**
  - apps/desktop/**
  - tools/**
  - .github/**
---

## 目标

建出 ARCHITECTURE.md 第 2 节的目录结构，让 `scripts/verify.sh --e2e` 在本机和 CI 三平台全绿。只有骨架，没有业务逻辑。

## 要做

- Rust workspace：`crates/` 下建 go-core、sgf、document、katago-protocol、engine-session、analysis、app-core；`tools/fake-katago`（包名 `fake-katago`，main 只打印用法后退出）；`apps/desktop/src-tauri` 的包名必须是 `lizzieyzy-desktop`。crate 之间先不加依赖，后续任务按需添加，方向必须符合 `scripts/architecture.json`。
- 每个 crate 设 `[lints] workspace = true`，`lib.rs` 只有一行 crate 文档。workspace 级 lint：rust `unsafe_code = "forbid"`；clippy `unwrap_used`、`expect_used` 设为 deny。`clippy.toml` 设 `allow-unwrap-in-tests = true`、`allow-expect-in-tests = true`。
- `rust-toolchain.toml` 固定版本 `1.96.0`（本机 rustup 的 stable），带 rustfmt、clippy 组件。本机要用 rustup 的工具链：PATH 里 `~/.cargo/bin` 必须排在 `/opt/homebrew/bin` 前面（Homebrew 的 cargo 没有可用的 clippy）。
- `apps/desktop`：Tauri 2 + React + TypeScript（strict）+ Vite，包管理用 pnpm。
  - package.json 脚本：`dev`、`build`、`typecheck`、`lint`（ESLint flat config）、`test`（`vitest run`）、`e2e`（Playwright，跑 Vite 构建 + mock 后端，先放一个打开首页的用例）、`tauri`。
  - 应用标识 `io.github.wimi321.lizzieyzy.next2`，窗口标题 `LizzieYzy Next 2 (dev)`，首页只显示标题。
- mock 后端按 ARCHITECTURE.md 第 6 节接入：真实的 `src/api/backend.ts` 和 `e2e/mock-backend.ts` 接口相同，Playwright 模式下用 Vite alias 替换，生产代码不 import mock。
- `.gitignore`：至少包含 `target/`、`node_modules/`、`dist/`、Playwright 输出目录。
- `.gitattributes`：`* text=auto eol=lf`；`*.bat`、`*.cmd` 用 crlf；`fixtures/** -text`（GBK 等原始字节必须原样保存）。
- CI `.github/workflows/ci.yml`：
  - ubuntu-latest、macos-latest、windows-latest 三个平台跑 `scripts/verify.sh`；Windows 用 bash，并设 `PYTHON=python`。
  - ubuntu 额外跑 `scripts/verify.sh --e2e`。
  - 安装 Tauri 在 Linux 上需要的系统库，缓存 cargo 和 pnpm，Node 用当前 LTS。
- `LICENSE`：从 `../lizzieyzy-next/LICENSE.txt` 复制（GPL-3.0 全文）。
- `README.md`：在现有内容后面补上本地开发命令。

## 不做

- 任何业务逻辑、额外文档、发布打包（P1-11 做）。

## 验收标准

1. 本机 `scripts/verify.sh --e2e` 以 `verify: ALL PASSED` 结束。
2. 推到 GitHub 后 CI 三平台全绿。仓库还没建时，这条在建仓后补验，汇报里注明。
3. `pnpm -C apps/desktop tauri dev` 能打开窗口（手工）。
4. `python3 scripts/check_budgets.py` 输出 `budgets: ok`，workspace 里的 crate 和 `scripts/architecture.json` 完全一致。
