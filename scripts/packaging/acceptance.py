#!/usr/bin/env python3
"""Inspect real installer payloads against the verified build-time receipt.

Run on the installer's own platform. Results belong in target/ or CI artifacts;
this module never rewrites, signs, or repairs the installed payload.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import stat
import subprocess
import sys
import tempfile
import time


class AcceptanceError(RuntimeError):
    pass


def digest(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def regular_inside(root: Path, relative: str) -> Path:
    parts = PurePosixPath(relative)
    if parts.is_absolute() or not parts.parts or ".." in parts.parts or "\\" in relative:
        raise AcceptanceError(f"Unsafe receipt path: {relative}")
    path = root.joinpath(*parts.parts)
    if not path.is_file() or path.is_symlink() or not path.resolve().is_relative_to(root.resolve()):
        raise AcceptanceError(f"Missing or escaping resource: {path}")
    return path


def verify_payload(staged: Path, resource_root: Path, platform: str) -> dict:
    receipt_path = staged / "bundle-manifest.json"
    receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
    if receipt.get("platform") != platform or not receipt.get("files"):
        raise AcceptanceError("Receipt platform or file inventory does not match")
    if digest(regular_inside(resource_root, "bundle-manifest.json")) != digest(receipt_path):
        raise AcceptanceError("Installer contains a different bundle receipt")
    files = receipt["files"]
    executable = f"engines/katago/{platform}/katago" + (".exe" if platform == "windows-x64" else "")
    required = {executable, "weights/default.bin.gz"}
    if platform == "windows-x64":
        required.add("readboard/readboard.exe")
    if not required <= files.keys():
        raise AcceptanceError(f"Receipt omits required payload: {sorted(required - files.keys())}")
    total = 0
    for relative, expected in files.items():
        path = regular_inside(resource_root, relative)
        if path.stat().st_size != expected["sizeBytes"] or digest(path) != expected["sha256"]:
            raise AcceptanceError(f"Installer resource hash/size mismatch: {relative}")
        if expected.get("executable") and os.name != "nt" and not path.stat().st_mode & stat.S_IXUSR:
            raise AcceptanceError(f"Installer lost executable mode: {relative}")
        total += path.stat().st_size
    return {"filesVerified": len(files), "resourceBytes": total}


def run(command: list[str], **kwargs) -> None:
    subprocess.run(command, check=True, timeout=300, **kwargs)


def extract(installer: Path, payload: Path) -> None:
    suffix = installer.suffix.lower()
    if suffix == ".exe":
        if os.name != "nt":
            raise AcceptanceError("NSIS installation requires Windows")
        # NSIS requires /D last and unquoted inside the argument. subprocess
        # quotes the single native argument correctly when the destination has spaces.
        run([str(installer), "/S", f"/D={payload}"])
    elif suffix == ".deb":
        run(["dpkg-deb", "-x", str(installer), str(payload)])
    elif suffix == ".appimage":
        installer.chmod(installer.stat().st_mode | stat.S_IXUSR)
        run([str(installer), "--appimage-extract"], cwd=payload, stdout=subprocess.DEVNULL)
    elif suffix == ".dmg":
        if sys.platform != "darwin":
            raise AcceptanceError("DMG inspection requires macOS")
        with tempfile.TemporaryDirectory(prefix="lizzie-dmg-") as mount:
            run(["hdiutil", "attach", "-readonly", "-nobrowse", "-mountpoint", mount, str(installer)])
            try:
                apps = list(Path(mount).glob("*.app"))
                if len(apps) != 1:
                    raise AcceptanceError("DMG must contain exactly one application")
                run(["ditto", str(apps[0]), str(payload / apps[0].name)])
            finally:
                run(["hdiutil", "detach", mount])
    else:
        raise AcceptanceError(f"Unsupported installer: {installer.name}")


def locate(payload: Path, platform: str) -> tuple[Path, Path]:
    receipts = []
    for candidate in payload.rglob("bundle-manifest.json"):
        if candidate.is_symlink() or not candidate.resolve().is_relative_to(payload.resolve()):
            continue
        try:
            manifest = json.loads(candidate.read_text(encoding="utf-8"))
        except (UnicodeError, json.JSONDecodeError):
            continue
        # Vendor KataGo bundles can carry their own bundle-manifest.json.
        # Only our application resource receipt has this platform/file schema.
        if (isinstance(manifest, dict) and manifest.get("schemaVersion") == 1
                and manifest.get("platform") == platform and isinstance(manifest.get("files"), dict)):
            receipts.append(candidate)
    if len(receipts) != 1:
        raise AcceptanceError(f"Expected one packaged receipt, found {len(receipts)}")
    root = receipts[0].parent
    if platform.startswith("macos-"):
        application = root.parent / "MacOS" / "lizzieyzy-desktop"
    elif platform == "windows-x64":
        application = root / "lizzieyzy-desktop.exe"
    else:
        application = root.parent.parent / "bin" / "lizzieyzy-desktop"
    if not application.is_file():
        raise AcceptanceError(f"Packaged application not found: {application}")
    return root, application


def inspect(args: argparse.Namespace) -> dict:
    started = time.monotonic()
    installer = args.installer.resolve(strict=True)
    staged = args.staged.resolve(strict=True)
    output = args.output.resolve()
    # Never recursively delete an old install or overwrite previous evidence.
    output.mkdir(parents=True, exist_ok=False)
    payload = output / "payload"
    payload.mkdir()
    extract(installer, payload)
    resource_root, application = locate(payload, args.platform)
    result = verify_payload(staged, resource_root, args.platform)
    if args.platform.startswith("macos-"):
        engine = resource_root / "engines" / "katago" / args.platform
        for binary in [engine / "katago", *engine.rglob("*.dylib")]:
            run(["codesign", "--verify", "--strict", str(binary)])
        # Only the version command; full clean-profile macOS analysis is manual acceptance.
        run([str(engine / "katago"), "version"], cwd=output)
    result.update({
        "platform": args.platform, "installer": str(installer),
        "installerBytes": installer.stat().st_size, "installerSha256": digest(installer),
        "payloadRoot": str(payload), "resourceRoot": str(resource_root),
        "application": str(application), "elapsedSeconds": round(time.monotonic() - started, 3),
    })
    (output / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    line = (f"installer: ok {installer.name}; {result['filesVerified']} resource hashes; "
            f"installer {result['installerBytes']} bytes; resources {result['resourceBytes']} bytes; "
            f"{result['elapsedSeconds']}s")
    print(line)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with Path(os.environ["GITHUB_STEP_SUMMARY"]).open("a", encoding="utf-8") as summary:
            summary.write(line + "\n\n")
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform", required=True, choices=["windows-x64", "linux-x64", "macos-arm64", "macos-amd64"])
    parser.add_argument("--staged", required=True, type=Path)
    parser.add_argument("--installer", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    try:
        inspect(args)
    except (AcceptanceError, OSError, ValueError, subprocess.SubprocessError) as error:
        parser.exit(1, f"installer: FAILED: {error}\n")


if __name__ == "__main__":
    main()
