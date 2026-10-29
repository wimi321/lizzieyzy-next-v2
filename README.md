# LizzieYzy Next 2（开发中）

LizzieYzy Next 的重写版：Tauri 2 + Rust 内核 + React/TypeScript 界面，基于 KataGo 的围棋复盘工具。
稳定版仍是 [lizzieyzy-next](https://github.com/wimi321/lizzieyzy-next)。

- 架构：[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- 路线图：[docs/ROADMAP.md](docs/ROADMAP.md)
- 开发规则与验收：[AGENTS.md](AGENTS.md)、[docs/ACCEPTANCE.md](docs/ACCEPTANCE.md)

许可证：GPL-3.0。

## 本地开发

先安装 [Tauri 2 系统依赖](https://v2.tauri.app/start/prerequisites/)、Rustup、当前 LTS
版本的 Node.js、Python 3 和 pnpm（版本见 `apps/desktop/package.json` 的 `packageManager`）。
Rust 工具链由 `rust-toolchain.toml` 固定为 1.96.0，并包含 rustfmt 和 clippy。
以下命令从仓库根目录运行；Windows 请使用 Git Bash，并先设置 `export PYTHON=python`。

```bash
# 必须优先使用 rustup 的 cargo，而非 Homebrew 的 cargo。
export PATH="$HOME/.cargo/bin:$PATH"
pnpm -C apps/desktop install --frozen-lockfile

# 浏览器开发 / 桌面窗口开发
pnpm -C apps/desktop dev
pnpm -C apps/desktop tauri dev
```

验证和构建：

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --workspace --locked
pnpm -C apps/desktop typecheck
pnpm -C apps/desktop lint
pnpm -C apps/desktop test
pnpm -C apps/desktop build

# 首次运行 E2E 前安装 Chromium；Linux 可加 --with-deps 安装浏览器系统依赖。
pnpm -C apps/desktop exec playwright install chromium
scripts/verify.sh --e2e
```

Playwright 使用 Vite 构建后的页面和通过 alias 替换的 mock 后端。
桌面窗口需要通过 `tauri dev` 手工验收。
