#!/usr/bin/env python3
"""Architecture and size budgets for LizzieYzy Next 2.

Part of the acceptance gate owned by the maintainers (AGENTS.md); agents must not edit it.
Configuration lives in scripts/architecture.json. Exits with 1 when any budget is violated.
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CONFIG = json.loads((ROOT / "scripts" / "architecture.json").read_text(encoding="utf-8"))
COMMENT_PREFIXES = ("//", "#", "*", "/*")
TEST_SUFFIXES = ("_tests.rs", ".test.ts", ".test.tsx", ".spec.ts", ".spec.tsx")


def git_files() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard"],
        cwd=ROOT, check=True, capture_output=True, text=True,
    ).stdout
    return sorted({line for line in out.splitlines() if line and (ROOT / line).is_file()})


def is_test_file(path: str) -> bool:
    name = path.rsplit("/", 1)[-1]
    return (
        "/tests/" in path
        or path.startswith("apps/desktop/e2e/")
        or name == "tests.rs"
        or name.endswith(TEST_SUFFIXES)
        or (name.startswith("test_") and name.endswith(".py"))
    )


def check_file_sizes(path: str, errors: list[str]) -> None:
    size = (ROOT / path).stat().st_size
    by_prefix = CONFIG["max_file_bytes_by_prefix"]
    limit = next((v for k, v in by_prefix.items() if path.startswith(k)), CONFIG["max_file_bytes"])
    if size > limit:
        errors.append(f"{path}: {size} bytes > {limit}")
    if path.startswith("docs/") and path.lower().endswith(tuple(CONFIG["forbidden_in_docs"])):
        errors.append(f"{path}: no images, PDFs or JSON evidence under docs/")


def check_source(path: str, text: str, errors: list[str]) -> None:
    test = is_test_file(path)
    lines = len(text.splitlines())
    limit = CONFIG["max_test_lines"] if test else CONFIG["max_source_lines"]
    if lines > limit:
        errors.append(f"{path}: {lines} lines > {limit}")
    if test:
        return
    for rule in CONFIG["forbidden_patterns"]:
        if not path.startswith(tuple(rule["paths"])):
            continue
        regex = re.compile(rule["pattern"])
        for number, line in enumerate(text.splitlines(), 1):
            if not line.lstrip().startswith(COMMENT_PREFIXES) and regex.search(line):
                errors.append(f"{path}:{number}: {rule['message']}")
    if path.endswith(".rs"):
        commands = len(re.findall(r"#\[tauri::command", text))
        command_dir = CONFIG["tauri_command_dir"]
        if commands and not path.startswith(command_dir):
            errors.append(f"{path}: #[tauri::command] belongs under {command_dir}")
        elif commands > CONFIG["max_tauri_commands_per_file"]:
            errors.append(f"{path}: {commands} Tauri commands; split the file by domain")


def check_files(errors: list[str]) -> None:
    roots = tuple(root.rstrip("/") + "/" for root in CONFIG["scan_roots"])
    extensions = tuple(CONFIG["source_extensions"])
    for path in git_files():
        check_file_sizes(path, errors)
        if not (path.startswith(roots) and path.endswith(extensions)):
            continue
        text = (ROOT / path).read_text(encoding="utf-8", errors="replace")
        if "/generated/" in path or "@generated" in text[:300]:
            continue
        check_source(path, text, errors)


def is_forbidden_dep(name: str, forbidden: list[str]) -> bool:
    return any(name == f or name.startswith(f + "-") for f in forbidden)


def check_crates(errors: list[str]) -> None:
    if not (ROOT / "Cargo.toml").is_file():
        errors.append("Cargo.toml missing: the workspace is not scaffolded yet (P0-01)")
        return
    proc = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
        cwd=ROOT, capture_output=True, text=True,
    )
    if proc.returncode != 0:
        detail = proc.stderr.strip().splitlines()
        errors.append("cargo metadata failed: " + (detail[-1] if detail else "no output"))
        return
    meta = json.loads(proc.stdout)
    allowed = CONFIG["workspace_crates"]
    dev_allowed = CONFIG["dev_allowed"]
    members = set(meta["workspace_members"])
    packages = [p for p in meta["packages"] if p["id"] in members]
    names = {p["name"] for p in packages}
    for name in sorted(names - set(allowed)):
        errors.append(f"crate {name}: not in scripts/architecture.json (maintainers decide its place)")
    for name in sorted(set(allowed) - names):
        errors.append(f"crate {name}: listed in scripts/architecture.json but not in the workspace")
    for package in packages:
        name = package["name"]
        for dep in package["dependencies"]:
            dep_name, is_dev = dep["name"], dep.get("kind") == "dev"
            if dep_name in names:
                ok = set(allowed.get(name, []))
                if is_dev:
                    ok |= set(dev_allowed.get(name, []))
                if dep_name not in ok:
                    errors.append(f"crate {name} -> {dep_name}: dependency direction not allowed")
            if (name in CONFIG["pure_crates"] and not is_dev
                    and is_forbidden_dep(dep_name, CONFIG["pure_forbidden_deps"])):
                errors.append(f"crate {name} -> {dep_name}: pure crates stay synchronous and I/O-free")
            if dep_name.startswith("tauri") and name not in CONFIG["tauri_allowed_crates"]:
                errors.append(f"crate {name} -> {dep_name}: only the desktop shell depends on Tauri")


def main() -> int:
    errors: list[str] = []
    check_files(errors)
    check_crates(errors)
    for error in errors:
        print(f"  {error}")
    print(f"budgets: FAILED, {len(errors)} violation(s)" if errors else "budgets: ok")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
