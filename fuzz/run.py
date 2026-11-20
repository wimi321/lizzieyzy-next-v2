#!/usr/bin/env python3
"""Run seeded fuzz targets and retain/minimize any failing inputs.

Generate the corpus with seed_corpus.py first. Run from any directory; all
generated corpus, artifacts and logs stay inside the ignored fuzz directories.
"""

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys


FUZZ = Path(__file__).resolve().parent
# SGF property names and syntax help every target whose input is SGF text.
DICTIONARIES = {target: "sgf.dict" for target in ("sgf_roundtrip", "document_walk", "lz")}


def logged(command, log, env, timeout=None):
    print("+ " + " ".join(map(str, command)), flush=True)
    with log.open("w") as output:
        try:
            return subprocess.run(
                command, cwd=FUZZ, env=env, stdout=output,
                stderr=subprocess.STDOUT, timeout=timeout, check=False,
            ).returncode
        except subprocess.TimeoutExpired:
            output.write("\nMinimization exceeded its wall-clock limit.\n")
            return 124


def report(target, status, log):
    contents = log.read_text(errors="replace")
    progress = re.findall(r"^#(\d+).*?\bcov: (\d+)", contents, re.MULTILINE)
    runs = re.findall(r"stat::number_of_executed_units:\s*(\d+)", contents)
    count = runs[-1] if runs else progress[-1][0] if progress else "unavailable"
    cov = progress[-1][1] if progress else "unavailable"
    print(f"{target}: exit={status} executions={count} cov={cov} log={log}", flush=True)
    if status:
        print("\n".join(contents.splitlines()[-60:]), flush=True)


def run_target(target, args, cargo, env):
    corpus = FUZZ / "corpus" / target
    if not corpus.is_dir() or not any(corpus.iterdir()):
        raise ValueError(f"{target}: no seeds; run python3 fuzz/seed_corpus.py first")
    raw = FUZZ / "artifacts" / target / "raw"
    minimized = raw.parent / "minimized"
    raw.mkdir(parents=True, exist_ok=True)
    minimized.mkdir(exist_ok=True)
    before = {p: p.stat().st_mtime_ns for p in raw.iterdir() if p.is_file()}
    log = FUZZ / "logs" / f"{target}.log"
    options = [
        f"-max_total_time={args.seconds}", f"-max_len={args.max_len}",
        "-timeout=20", "-rss_limit_mb=4096", "-print_final_stats=1",
        f"-artifact_prefix={raw}{os.sep}",
    ]
    dictionary = DICTIONARIES.get(target)
    if dictionary:
        options.append(f"-dict={FUZZ / dictionary}")
    status = logged(cargo + ["run", target, str(corpus), "--", *options], log, env)
    report(target, status, log)
    if status:
        for case in sorted(raw.iterdir()):
            if not case.is_file() or before.get(case) == case.stat().st_mtime_ns:
                continue
            result = minimized / case.name
            minimize_log = FUZZ / "logs" / f"{target}-{case.name}-tmin.log"
            minimize_status = logged(
                cargo + [
                    "tmin", target, str(case), "--",
                    f"-max_total_time={args.minimize_seconds}",
                    "-timeout=20", "-rss_limit_mb=4096",
                    f"-exact_artifact_path={result}",
                ],
                minimize_log, env, timeout=args.minimize_seconds + 60,
            )
            print(
                f"{target}: minimization exit={minimize_status}; "
                f"raw={case}; minimized={result if result.is_file() else 'not produced'}; "
                f"log={minimize_log}", flush=True,
            )
    # A crash is still a failure even when its minimization succeeds.
    return status


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("targets", nargs="*", help="Targets to run; defaults to all")
    parser.add_argument("--seconds", type=int, default=600)
    parser.add_argument("--max-len", type=int, default=65536)
    parser.add_argument("--minimize-seconds", type=int, default=60)
    args = parser.parse_args()
    if min(args.seconds, args.max_len, args.minimize_seconds) <= 0:
        parser.error("time and input length limits must be positive")
    pins = re.findall(
        r'^channel\s*=\s*"(nightly-\d{4}-\d{2}-\d{2})"\s*$',
        (FUZZ / "rust-toolchain.toml").read_text(), re.MULTILINE,
    )
    if len(pins) != 1:
        parser.error("fuzz/rust-toolchain.toml must pin one dated nightly channel")
    toolchain = pins[0]
    cargo = ["cargo", f"+{toolchain}", "fuzz"]
    env = os.environ.copy()
    env["PATH"] = str(Path.home() / ".cargo" / "bin") + os.pathsep + env.get("PATH", "")
    # cargo-fuzz 0.13.1 has no --locked flag. Validate the lock before it builds,
    # and fail if any subsequent cargo-fuzz invocation changes its bytes.
    lock = FUZZ / "Cargo.lock"
    locked_bytes = lock.read_bytes()
    metadata = json.loads(subprocess.check_output(
        ["cargo", f"+{toolchain}", "metadata", "--locked", "--format-version=1"],
        cwd=FUZZ, env=env,
    ))
    package = next(
        package for package in metadata["packages"]
        if Path(package["manifest_path"]) == FUZZ / "Cargo.toml"
    )
    targets = [target["name"] for target in package["targets"] if "bin" in target["kind"]]
    selected = args.targets or targets
    if set(selected) - set(targets):
        parser.error("unknown target; available targets: " + ", ".join(targets))
    (FUZZ / "logs").mkdir(exist_ok=True)
    failed = False
    try:
        for target in selected:
            failed = bool(run_target(target, args, cargo, env)) or failed
    finally:
        if lock.read_bytes() != locked_bytes:
            raise RuntimeError("fuzz/Cargo.lock changed; resolve and review dependencies first")
    return int(failed)


if __name__ == "__main__":
    sys.exit(main())
