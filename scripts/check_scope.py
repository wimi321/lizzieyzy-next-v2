#!/usr/bin/env python3
"""List files changed on this branch that fall outside a task card's `paths:` scope.

Part of the acceptance gate owned by the maintainers (AGENTS.md); agents must not edit it.
Usage: scripts/check_scope.py docs/tasks/<card>.md [base-ref, default main]
"""
from __future__ import annotations

import fnmatch
import subprocess
import sys
from pathlib import Path

ALWAYS_ALLOWED = ["Cargo.lock", "apps/desktop/pnpm-lock.yaml"]


def card_globs(card: Path) -> list[str]:
    lines = card.read_text(encoding="utf-8").splitlines()
    if not lines or lines[0].strip() != "---":
        sys.exit(f"{card}: missing front matter")
    globs: list[str] = []
    in_paths = False
    for line in lines[1:]:
        stripped = line.strip()
        if stripped == "---":
            break
        if line.startswith("paths:"):
            in_paths = True
        elif in_paths and stripped.startswith("- "):
            globs.append(stripped[2:].strip().strip("\"'"))
        elif in_paths and stripped:
            in_paths = False
    if not globs:
        sys.exit(f"{card}: no paths listed in front matter")
    return globs


def git(*args: str) -> list[str]:
    out = subprocess.run(["git", *args], check=True, capture_output=True, text=True).stdout
    return [line for line in out.splitlines() if line]


def changed_files(base: str) -> list[str]:
    files = set(git("diff", "--name-only", f"{base}...HEAD"))
    files.update(git("diff", "--name-only", "HEAD"))
    files.update(git("ls-files", "--others", "--exclude-standard"))
    return sorted(files)


def main() -> int:
    if len(sys.argv) not in (2, 3):
        sys.exit(__doc__)
    card = Path(sys.argv[1])
    base = sys.argv[2] if len(sys.argv) == 3 else "main"
    globs = card_globs(card) + ALWAYS_ALLOWED
    outside = [f for f in changed_files(base) if not any(fnmatch.fnmatch(f, g) for g in globs)]
    for path in outside:
        print(f"  outside scope: {path}")
    print(f"scope: {len(outside)} file(s) outside {card.name}" if outside else "scope: ok")
    return 1 if outside else 0


if __name__ == "__main__":
    sys.exit(main())
