#!/usr/bin/env python3
"""Release metadata, temporary version override, and draft-only GitHub upload."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
from urllib.parse import quote

from release_assets import ReleaseError, validate_version, version_from_tag


def metadata(env: dict[str, str], config: Path) -> tuple[str, str]:
    if env.get("GITHUB_REF_TYPE") == "tag":
        tag = env["GITHUB_REF_NAME"]
    elif env.get("RELEASE_TAG", "").strip():
        tag = env["RELEASE_TAG"].strip()
    else:
        base = validate_version(json.loads(config.read_text(encoding="utf-8"))["version"])
        # An untagged manual branch run cannot accidentally become a stable release.
        base = base.split("+", 1)[0].split("-", 1)[0]
        run_id, attempt = env.get("GITHUB_RUN_ID", ""), env.get("GITHUB_RUN_ATTEMPT", "")
        if not re.fullmatch(r"[1-9][0-9]*", run_id) or not re.fullmatch(r"[1-9][0-9]*", attempt):
            raise ReleaseError("Manual previews require a positive run ID and attempt")
        tag = f"v{base}-preview.{run_id}.{attempt}"
    return tag, version_from_tag(tag)


def configure(version: str, public_key: str, output: Path) -> None:
    validate_version(version)
    output.parent.mkdir(parents=True, exist_ok=True)
    # Signing is performed after all native signing/stapling, in the aggregate job.
    output.write_text(json.dumps({"version": version, "bundle": {"createUpdaterArtifacts": False}},
                                 indent=2) + "\n", encoding="utf-8")
    if not public_key.strip():
        print("LIZZIE_UPDATER_PUBKEY 缺失：本次构建的程序不会检查更新。")
    else:
        print("更新公钥将通过构建时 LIZZIE_UPDATER_PUBKEY 嵌入程序。")


def gh(arguments: list[str]) -> subprocess.CompletedProcess:
    return subprocess.run(["gh", *arguments], text=True, capture_output=True, check=False)


def require_success(result: subprocess.CompletedProcess) -> None:
    if result.returncode:
        raise ReleaseError(result.stderr.strip() or "GitHub command failed")


def draft(repository: str, tag: str, commit: str, assets: Path, notes: Path, run=gh) -> None:
    version = version_from_tag(tag)
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
        raise ReleaseError("Expected owner/repository")
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ReleaseError("Release target must be an exact commit SHA")
    files = sorted(assets.iterdir())
    if not files or any(not p.is_file() or p.is_symlink() for p in files):
        raise ReleaseError("Release assets must be regular files")
    if not (assets / "SHA256SUMS").is_file():
        raise ReleaseError("Release asset checksums are missing")
    # Refuse to attach newly built bytes to a tag that names another commit.
    ref = run(["api", f"repos/{repository}/git/ref/tags/{quote(tag, safe='')}",
               "--jq", '.object | [.type,.sha] | @tsv'])
    if ref.returncode == 0:
        kind, referenced_sha = ref.stdout.strip().split("\t")
        if kind == "tag":
            resolved = run(["api", f"repos/{repository}/commits/{quote(tag, safe='')}", "--jq", ".sha"])
            require_success(resolved)
            referenced_sha = resolved.stdout.strip()
        elif kind != "commit":
            raise ReleaseError("Release tag does not resolve to a commit")
        if referenced_sha != commit:
            raise ReleaseError("Release tag belongs to a different commit")
    elif "HTTP 404" not in ref.stderr:
        require_success(ref)
    common = ["--repo", repository]
    view = run(["release", "view", tag, *common, "--json", "isDraft,targetCommitish,assets"])
    if view.returncode == 0:
        existing = json.loads(view.stdout)
        if not existing["isDraft"]:
            raise ReleaseError("Refusing to modify a published release")
        if existing["targetCommitish"] != commit:
            raise ReleaseError("Existing draft does not name the exact build commit")
        unexpected = {item["name"] for item in existing["assets"]} - {p.name for p in files}
        if unexpected:
            raise ReleaseError("Existing draft has stale assets; maintainer must review them before retrying")
        require_success(run(["release", "edit", tag, *common, "--notes-file", str(notes)]))
        require_success(run(["release", "upload", tag, *common, "--clobber", *map(str, files)]))
    elif "HTTP 404" in view.stderr or "release not found" in view.stderr.lower():
        arguments = ["release", "create", tag, *common, "--draft", "--target", commit,
                     "--title", f"LizzieYzy Next 2 {version}", "--notes-file", str(notes)]
        if "-" in version.split("+", 1)[0]:
            arguments.append("--prerelease")
        require_success(run([*arguments, *map(str, files)]))
    else:
        require_success(view)
    print(f"Draft ready: https://github.com/{repository}/releases/tag/{tag}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    meta = commands.add_parser("metadata")
    meta.add_argument("--config", type=Path, default=Path("apps/desktop/src-tauri/tauri.conf.json"))
    config = commands.add_parser("configure")
    config.add_argument("--output", type=Path, required=True)
    publish = commands.add_parser("draft")
    publish.add_argument("--assets", type=Path, required=True)
    publish.add_argument("--notes", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == "metadata":
            tag, version = metadata(dict(os.environ), args.config)
            with Path(os.environ["GITHUB_OUTPUT"]).open("a", encoding="utf-8") as output:
                output.write(f"tag={tag}\nversion={version}\n")
            print(f"Release candidate: {tag}")
        elif args.command == "configure":
            configure(os.environ["RELEASE_VERSION"], os.environ.get("LIZZIE_UPDATER_PUBKEY", ""), args.output)
        else:
            draft(os.environ["GITHUB_REPOSITORY"], os.environ["RELEASE_TAG"],
                  os.environ["GITHUB_SHA"], args.assets, args.notes)
    except (ReleaseError, OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        parser.exit(1, f"release: FAILED: {error}\n")


if __name__ == "__main__":
    main()
