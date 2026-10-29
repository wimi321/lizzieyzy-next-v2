#!/usr/bin/env bash
# Acceptance gate for LizzieYzy Next 2. Owned by the maintainers; agents must not edit it
# (AGENTS.md). Runs every step, then prints a summary. Usage: scripts/verify.sh [--e2e]
set -u
cd "$(dirname "$0")/.." || exit 1

PY="${PYTHON:-python3}"
FAILED=""

step() {
  local name="$1"
  shift
  printf '\n==> %s\n' "$name"
  if "$@"; then
    printf 'PASS  %s\n' "$name"
  else
    printf 'FAIL  %s\n' "$name"
    FAILED="${FAILED}  - ${name}\n"
  fi
}

step "budgets" "$PY" scripts/check_budgets.py
step "cargo fmt" cargo fmt --all --check
step "cargo clippy" cargo clippy --workspace --all-targets --locked -- -D warnings
step "cargo test" cargo test --workspace --locked
step "frontend install" pnpm -C apps/desktop install --frozen-lockfile
step "frontend typecheck" pnpm -C apps/desktop run typecheck
step "frontend lint" pnpm -C apps/desktop run lint
step "frontend test" pnpm -C apps/desktop run test
step "frontend build" pnpm -C apps/desktop run build
if [ "${1:-}" = "--e2e" ]; then
  step "e2e (web, mock backend)" pnpm -C apps/desktop run e2e
fi

printf '\n'
if [ -n "$FAILED" ]; then
  printf 'verify: FAILED\n%b' "$FAILED"
  exit 1
fi
printf 'verify: ALL PASSED\n'
