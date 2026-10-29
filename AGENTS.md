# Agent guide — LizzieYzy Next 2

Rules for coding agents (Codex, Claude Code, …) in this repository. The maintainers own the plan;
agents implement one task card at a time. Read this file, `docs/ARCHITECTURE.md` and your task
card completely before writing code.

## What this is

A rewrite of LizzieYzy Next (Go/baduk review app on KataGo): Tauri 2 shell, Rust core, React +
TypeScript UI. License GPL-3.0. The Java app in `../lizzieyzy-next` is a **behaviour reference
only**: read it to learn what users expect, never port its threading, locking or global state.

## How to work

1. One task card = one branch: `task/<ID>-<slug>` from `main`. Cards live in `docs/tasks/`.
2. Change only files matched by the card's `paths:` (plus `Cargo.lock` and
   `apps/desktop/pnpm-lock.yaml`). `python3 scripts/check_scope.py docs/tasks/<card>.md main`
   must report nothing.
3. If the card is ambiguous, wrong, or needs out-of-scope changes: stop and ask in your report.
   Do not widen the scope yourself.
4. Before you finish, `scripts/verify.sh` must end with `verify: ALL PASSED`. Also run the extra
   commands the card lists.
5. Report in chat (never in a committed file) using the report format in `docs/tasks/README.md`.
6. Conventional commits (`feat(engine-session): …`), small and focused. Never commit build
   output, logs, screenshots or evidence files.

## Architecture rules (enforced by `scripts/check_budgets.py` and review)

- Dependency direction is fixed in `scripts/architecture.json`. Domain crates (`go-core`, `sgf`,
  `document`, `katago-protocol`) are pure and synchronous: no tokio, tauri, network or database.
- Every engine is owned by exactly one tokio task (actor) in `engine-session`. In
  `engine-session`, `analysis` and `app-core`: no `Mutex`, `RwLock`, atomics, `static mut`,
  `lazy_static!`, `std::thread::spawn` or `thread::sleep`. Use channels and `tokio::time`.
- KataGo is driven through the **analysis engine JSON protocol**. GTP will exist only in a later
  `gtp` backend for third-party engines and `genmove` play.
- The Rust core is the single source of truth. TypeScript renders snapshots and sends commands:
  no Go rules, SGF parsing or analysis logic in TS.
- TS types for commands and events are generated from Rust into `apps/desktop/src/api/generated/`.
  Never hand-edit them; a cargo test fails when they are stale.
- `#[tauri::command]` functions live only in `apps/desktop/src-tauri/src/commands/<domain>.rs` and
  are thin adapters over `app-core`.
- Size budgets: 800 lines per source file, 1200 per test file. Put Rust unit tests in a sibling
  `tests.rs` (`#[cfg(test)] mod tests;`), not inline.
- No `unwrap()`/`expect()` outside tests (clippy denies them). Errors are typed (`thiserror`).
- Every analysis value that reaches the UI is from **Black's perspective**.

## Testing rules

- Every behaviour change ships with a test that fails without it. Tests execute behaviour.
- Engine and analysis tests run against `tools/fake-katago`, never a real KataGo or a GPU.
- No sleeping to synchronise. Timeouts in tests are safety nets only (≤ 5 s).
- Parsers: golden fixtures in `fixtures/` plus property tests (proptest) for round trips.
- UI: Vitest for logic and components; Playwright against the Vite build with the mock backend
  (wired by a Vite alias, never imported by production code); tauri-driver E2E on Linux/Windows.

## Forbidden (lessons from the previous Tauri attempt)

- Tests or scripts that read source files to "prove" a feature exists (regex/grep gates). The only
  source scanning allowed is the maintainers' budget checker.
- Committed evidence: QA JSON, screenshots, claim ledgers, status reports. Evidence goes in the chat
  report or CI artifacts. Playwright snapshot baselines used by a running test are fine.
- Test, smoke or mock code reachable from the production bundle.
- New docs outside `docs/ARCHITECTURE.md`, `docs/ROADMAP.md`, `docs/decisions/`, `docs/tasks/` and
  `docs/inventory/`. Update `ARCHITECTURE.md` when you change an interface it describes.
- Editing the acceptance gate (`scripts/verify.sh`, `scripts/check_*.py`,
  `scripts/architecture.json`, `docs/ACCEPTANCE.md`, `AGENTS.md`) unless the card says so.
- New dependencies without a one-line reason in your report.

## Commands

- Full gate: `scripts/verify.sh` (add `--e2e` to include Playwright).
- Quick loops: `cargo test -p <crate>`, `pnpm -C apps/desktop test`, `pnpm -C apps/desktop dev`.
- Desktop app: `pnpm -C apps/desktop tauri dev`.

## Product conventions

- User-facing text uses i18n keys in `apps/desktop/src/locales/{zh-CN,en}.json`; add every new key
  to both. `apps/desktop/src/locales/legacy/` holds the Java translations as reference material.
- Never store secrets in config files. Downloads that get executed or loaded (engines, models,
  updates) are pinned by SHA-256.
- Windows that show remote web pages get no IPC permissions except explicitly listed bridges.
