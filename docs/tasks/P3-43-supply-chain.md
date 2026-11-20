---
id: P3-43
title: 工程线：供应链检查——cargo-deny、pnpm audit、发布附 SBOM、第三方 action 固定到 commit
depends_on: [P3-42]
paths:
  - deny.toml
  - .github/workflows/**
  - scripts/packaging/sbom.py
  - scripts/packaging/test_sbom.py
  - scripts/packaging/release_assets.py
  - scripts/packaging/test_release_assets.py
  - scripts/packaging/release_workflow.py
  - scripts/packaging/test_release_workflow.py
  - fixtures/sbom/**
  - apps/desktop/package.json
  - Cargo.toml
  - crates/*/Cargo.toml
  - tools/*/Cargo.toml
  - apps/desktop/src-tauri/Cargo.toml
  - docs/ARCHITECTURE.md
---

## 目标

现在 CI 不检查依赖的已知漏洞、许可证和来源，发布也不附带依赖清单。补上：每个 PR 和每周检查 Rust 与前端依赖；正式发布附一份 CycloneDX SBOM；第三方 GitHub Action 固定到 commit。

## 要做

1. **cargo-deny**：根目录加 `deny.toml`，检查整个工作区（`cargo deny check`，四项都查）。
   - licenses：本项目是 `GPL-3.0-only`，只允许和它兼容的许可证。先列出依赖树里实际出现的全部许可证，再写允许列表。拿不准是否兼容的（比如 OpenSSL 许可证、`Unicode-*`、`CDLA-*`），不要直接放行：在汇报里列出 crate、版本和许可证，由维护者决定。缺许可证元数据的 crate 用 `clarify` 写明，并固定许可证文件的哈希。工作区自己的 crate 和工具缺 `license` 字段的，补上 `license.workspace = true`。
   - advisories：已知漏洞（vulnerability）拒绝；yanked 拒绝；unmaintained 也检查。暂时无法升级的（比如 Tauri 在 Linux 上依赖的 gtk-rs 系列）写进 `ignore`，每条带原因。全部忽略的条目在汇报里列出。
   - bans：非 path 依赖不能用通配版本；同一 crate 的多个版本只警告，不失败。
   - sources：只允许 crates.io，未知的 registry 和 git 来源拒绝。
2. **pnpm audit**：`pnpm -C apps/desktop audit --audit-level high`，包括开发依赖（构建工具在带发布密钥的 CI 里运行）。暂时无法修的写进 `package.json` 里 pnpm 的审计忽略配置，汇报里写每条的原因。能通过升级解决的直接升级（只改锁文件或补丁版本）。
3. **CI**：新增 `.github/workflows/supply-chain.yml`。
   - 触发：pull_request、推到 main、每周一次（schedule）、workflow_dispatch。
   - 两个任务，在 Linux 上跑：cargo-deny 和 pnpm audit，失败就让检查失败。
   - cargo-deny 用固定版本（`cargo install --locked` 指定版本，或者固定到 commit 的官方 action，二选一）。
   - 不放进 `scripts/verify.sh`：漏洞库要联网，本地 verify 保持离线可用。
4. **SBOM**：正式发布附一份 CycloneDX（1.5 或 1.6）JSON。
   - 生成脚本 `scripts/packaging/sbom.py`，在 `release.yml` 的 draft 任务里生成，作为发布资产上传，并且列进 `SHA256SUMS`。文件名照现有资产的命名方式，比如 `LizzieYzy-Next-<版本>-sbom.cdx.json`。
   - 内容：
     - 四个发布平台（Windows x64、macOS arm64、macOS x64、Linux x64）实际编译进桌面程序的 Rust crate，不含开发依赖和 `tools/`，每个组件标出属于哪些平台；
     - 打包进程序的前端生产依赖；
     - 打包的 KataGo 引擎和权重，取自 `resources/katago/` 里的固定清单，带 SHA-256。
   - 每个组件写名称、版本、purl（`pkg:cargo`、`pkg:npm`；KataGo 用 `pkg:generic` 或 `pkg:github`）、许可证（元数据里有的话），以及已知的哈希：`Cargo.lock` 的 checksum 是 `.crate` 文件的 SHA-256；`pnpm-lock.yaml` 的 integrity 换算成 CycloneDX 的哈希格式。
   - 输出要确定：同样的锁文件和版本号生成同样的字节。时间戳用构建提交的时间，不用当前时间。
   - 脚本的单元测试用 `fixtures/sbom/` 里的小样本离线运行，并加入现有的「Test packaging helpers without network」步骤（这一步运行 `scripts/packaging` 下的全部 `test_*.py`，新测试文件会自动包括进去）。
   - `release_workflow.py draft` 检查资产列表时要接受这个文件。
5. **第三方 action 固定到 commit**：所有不属于 GitHub 官方（`actions/*`）的 action（`Swatinem/rust-cache`、`pnpm/action-setup`，以及这张卡新加的）都改成完整的 commit SHA，后面注释版本号。`actions/*` 保持主版本标签。
6. ARCHITECTURE 的 CI 和发布部分各加几句：供应链检查有哪些、什么时候跑、忽略条目放在哪里；SBOM 包含什么。

## 不做

- Tauri 权限和 CSP 的复查（维护者另做）。
- Dependabot 或自动升级依赖。
- 前端依赖的许可证检查（SBOM 里列出许可证，但不做门槛）。
- 给 preview 构建（非正式发布）附 SBOM。

## 验收标准

- `cargo deny check` 在本机通过（汇报里附输出的摘要）；`pnpm -C apps/desktop audit --audit-level high` 通过。
- `python3 -m unittest discover -s scripts/packaging -p 'test_*.py'` 通过，包括 SBOM 的测试：组件、平台标记、哈希换算，以及输出的确定性。
- 本机用当前的锁文件生成一份 SBOM，汇报里写组件数量（Rust、npm、KataGo 各多少），并用 CycloneDX 的 JSON Schema 校验通过（schema 文件可以在测试时从 fixtures 读，不在测试里联网）。
- `scripts/verify.sh` 通过；`check_budgets.py` 通过。
