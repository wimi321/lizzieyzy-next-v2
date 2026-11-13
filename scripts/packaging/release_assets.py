#!/usr/bin/env python3
"""Collect verified installers and sign a complete four-target updater release.

Only build output is written. Run collect on each native runner after acceptance;
run manifest on the merged artifacts after native signing and notarization.
"""
from __future__ import annotations

import argparse
import base64
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
from urllib.parse import quote
import zipfile

from acceptance import AcceptanceError, digest, verify_payload

ROOT = Path(__file__).resolve().parents[2]
PLATFORMS = ("windows-x64", "macos-arm64", "macos-amd64", "linux-x64")
TARGETS = {
    "windows-x64": "windows-x86_64",
    "macos-arm64": "darwin-aarch64",
    "macos-amd64": "darwin-x86_64",
    "linux-x64": "linux-x86_64",
}
EXTENSIONS = {
    "windows-x64": ("-setup.exe", "-portable.zip"),
    "macos-arm64": (".dmg", ".app.tar.gz"),
    "macos-amd64": (".dmg", ".app.tar.gz"),
    "linux-x64": (".AppImage", ".deb"),
}
UPDATER_SUFFIX = {"windows-x64": "-setup.exe", "linux-x64": ".AppImage",
                  "macos-arm64": ".app.tar.gz", "macos-amd64": ".app.tar.gz"}
BUNDLE_TARGETS = {"windows-x86_64-nsis": ("windows-x64", "-setup.exe"),
                  "linux-x86_64-appimage": ("linux-x64", ".AppImage"),
                  "linux-x86_64-deb": ("linux-x64", ".deb"),
                  "darwin-aarch64-app": ("macos-arm64", ".app.tar.gz"),
                  "darwin-x86_64-app": ("macos-amd64", ".app.tar.gz")}
MAX_UPDATE_BYTES = 1024 * 1024 * 1024
SEMVER = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
                    r"(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
                    r"(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?")


class ReleaseError(RuntimeError):
    pass


def validate_version(version: str) -> str:
    match = SEMVER.fullmatch(version)
    if not match or any(int(part) > 65535 for part in match.groups()[:3]):
        raise ReleaseError("Version must be SemVer with Windows version components <= 65535")
    if match[4] and any(part.isdigit() and len(part) > 1 and part.startswith("0")
                        for part in match[4].split(".")):
        raise ReleaseError("Numeric SemVer prerelease identifiers cannot have leading zeroes")
    return version


def version_from_tag(tag: str) -> str:
    if not tag.startswith("v"):
        raise ReleaseError("Release tag must start with v")
    return validate_version(tag[1:])


def asset_name(version: str, platform: str, suffix: str) -> str:
    validate_version(version)
    if platform not in EXTENSIONS or suffix not in EXTENSIONS[platform]:
        raise ReleaseError("Unsupported platform or asset suffix")
    return f"LizzieYzy-Next-2_{version}_{platform}{suffix}"


def regular(path: Path) -> Path:
    if path.is_symlink() or not path.is_file() or path.stat().st_size == 0:
        raise ReleaseError(f"Expected nonempty regular file: {path}")
    return path


def one_path(directory: Path, pattern: str) -> Path:
    paths = sorted(directory.glob(pattern))
    if len(paths) != 1:
        raise ReleaseError(f"Expected exactly one {pattern} in {directory}, found {len(paths)}")
    return regular(paths[0])


def installed_payload(acceptance_dir: Path, staged: Path, platform: str) -> tuple[Path, Path]:
    result = json.loads((acceptance_dir / "result.json").read_text(encoding="utf-8"))
    if result.get("platform") != platform:
        raise ReleaseError("Acceptance platform does not match")
    payload = (acceptance_dir / "payload").resolve(strict=True)
    resources, application = Path(result["resourceRoot"]), Path(result["application"])
    for path in (resources, application):
        if path.is_symlink() or not path.resolve(strict=True).is_relative_to(payload):
            raise ReleaseError("Acceptance path escapes its payload")
    regular(application)
    verify_payload(staged, resources, platform)
    return resources, application


def accepted_installer(acceptance_dir: Path, installer: Path) -> Path:
    result = json.loads((acceptance_dir / "result.json").read_text(encoding="utf-8"))
    if (installer.stat().st_size != result.get("installerBytes")
            or digest(installer) != result.get("installerSha256")):
        raise ReleaseError("Installer changed after payload acceptance")
    return installer


def windows_portable(resources: Path, application: Path, output: Path) -> None:
    if application.parent != resources or application.name != "lizzieyzy-desktop.exe":
        raise ReleaseError("Windows executable must be beside its installed resources")
    with zipfile.ZipFile(output, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
        for path in sorted(resources.rglob("*")):
            if path.is_symlink() or not path.resolve().is_relative_to(resources.resolve()):
                raise ReleaseError(f"Portable payload contains a link: {path}")
            if path.is_dir():
                continue
            if not path.is_file():
                raise ReleaseError(f"Portable payload contains a special file: {path}")
            relative = path.relative_to(resources)
            # The NSIS uninstaller belongs to the installed product, not the ZIP.
            if relative.as_posix().lower() == "uninstall.exe":
                continue
            archive.write(path, "LizzieYzy-Next-2/" + relative.as_posix())
    with zipfile.ZipFile(output) as archive:
        if archive.testzip() is not None:
            raise ReleaseError("Portable ZIP integrity check failed")


def macos_update_archive(application: Path, output: Path) -> None:
    app = application.parent.parent.parent
    if app.suffix != ".app" or application.parent.name != "MacOS":
        raise ReleaseError("Expected a macOS app bundle executable")
    for path in app.rglob("*"):
        if not path.resolve(strict=True).is_relative_to(app.resolve()):
            raise ReleaseError(f"App bundle link escapes the app: {path}")
        if not (path.is_dir() or path.is_file() or path.is_symlink()):
            raise ReleaseError(f"App bundle contains a special file: {path}")
    # Tauri's macOS installer expects a gzip tar with the .app as its top entry.
    # Preserve modes and internal framework symlinks; never dereference them.
    with tarfile.open(output, "x:gz", dereference=False) as archive:
        archive.add(app, arcname=app.name)


def collect(platform: str, version: str, bundle_dir: Path, staged: Path,
            output: Path, acceptance_dir: Path | None = None) -> list[Path]:
    validate_version(version)
    if platform not in PLATFORMS:
        raise ReleaseError("Unsupported platform")
    output.mkdir(parents=True, exist_ok=True)
    destinations = [output / asset_name(version, platform, suffix) for suffix in EXTENSIONS[platform]]
    if any(path.exists() or path.is_symlink() for path in destinations):
        raise ReleaseError("Refusing to overwrite collected release assets")
    if platform == "linux-x64":
        sources = [one_path(bundle_dir / "appimage", "*.AppImage"), one_path(bundle_dir / "deb", "*.deb")]
        for source, destination in zip(sources, destinations):
            shutil.copy2(source, destination)
    else:
        if acceptance_dir is None:
            raise ReleaseError("Windows/macOS collection requires --acceptance-dir")
        resources, application = installed_payload(acceptance_dir, staged, platform)
        if platform == "windows-x64":
            shutil.copy2(accepted_installer(acceptance_dir, one_path(bundle_dir / "nsis", "*.exe")), destinations[0])
            windows_portable(resources, application, destinations[1])
        else:
            shutil.copy2(accepted_installer(acceptance_dir, one_path(bundle_dir / "dmg", "*.dmg")), destinations[0])
            macos_update_archive(application, destinations[1])
    return destinations


def asset_inventory(assets: Path, version: str) -> list[Path]:
    expected = {asset_name(version, platform, suffix)
                for platform in PLATFORMS for suffix in EXTENSIONS[platform]}
    found = {path.name for path in assets.iterdir()}
    if found != expected:
        raise ReleaseError(f"Release asset inventory mismatch; missing={sorted(expected - found)}, "
                           f"unexpected={sorted(found - expected)}")
    paths = [regular(assets / name) for name in sorted(expected)]
    update_assets = {(platform, UPDATER_SUFFIX[platform]) for platform in PLATFORMS}
    update_assets.update(BUNDLE_TARGETS.values())
    for platform, suffix in update_assets:
        path = assets / asset_name(version, platform, suffix)
        if path.stat().st_size > MAX_UPDATE_BYTES:
            raise ReleaseError(f"Updater package exceeds P2-14's 1 GiB limit: {path.name}")
    return paths


def signature_version(signature: str) -> str:
    try:
        lines = base64.b64decode(signature, validate=True).decode("utf-8").splitlines()
        if len(lines) != 4 or not lines[2].startswith("trusted comment: "):
            raise ValueError("structure")
        raw = base64.b64decode(lines[1], validate=True)
        global_sig = base64.b64decode(lines[3], validate=True)
        if len(raw) != 74 or raw[:2] != b"ED" or len(global_sig) != 64:
            raise ValueError("algorithm")
        versions = [part[8:] for part in lines[2][17:].split("\t") if part.startswith("version:")]
        if len(versions) != 1:
            raise ValueError("version")
        return validate_version(versions[0])
    except (ValueError, UnicodeError) as error:
        raise ReleaseError("Invalid version-bound Tauri signature") from error


def tauri_command() -> list[str]:
    node = shutil.which("node")
    cli = ROOT / "apps/desktop/node_modules/@tauri-apps/cli/tauri.js"
    if not node or not cli.is_file():
        raise ReleaseError("Install the locked frontend dependencies and Node before signing")
    return [node, str(cli)]


def verify_signature(asset: Path, signature: Path, version: str, public_key: str) -> None:
    env = dict(os.environ, LIZZIE_UPDATER_PUBKEY=public_key)
    result = subprocess.run([tauri_command()[0], str(Path(__file__).with_name("verify_release_signature.mjs")),
                             str(asset), str(signature), version], env=env,
                            capture_output=True, text=True, timeout=300)
    if result.returncode:
        raise ReleaseError(f"Public-key signature verification failed: {asset.name}")


def sign_asset(asset: Path, version: str, env: dict[str, str]) -> str:
    # Credentials remain environment-only. Capture CLI output so even an upstream
    # diagnostic cannot accidentally print key material into workflow logs.
    result = subprocess.run([*tauri_command(), "signer", "sign", "--app-version", version, str(asset.resolve())],
                            env=env, capture_output=True, text=True, timeout=300)
    if result.returncode:
        raise ReleaseError(f"Tauri signing failed for {asset.name}; check the signing key/password")
    signature_file = asset.with_name(asset.name + ".sig")
    signature = signature_file.read_text(encoding="utf-8").strip()
    if signature_version(signature) != version:
        raise ReleaseError("Signer did not bind the requested version")
    if env.get("LIZZIE_UPDATER_PUBKEY", "").strip():
        verify_signature(asset, signature_file, version, env["LIZZIE_UPDATER_PUBKEY"])
    return signature


def write_checksums(assets: Path, paths: list[Path]) -> None:
    (assets / "SHA256SUMS").write_text("".join(f"{digest(path)}  {path.name}\n" for path in sorted(paths)),
                                     encoding="utf-8")


def manifest(assets: Path, version: str, tag: str, repository: str, notes: str,
             pub_date: str | None = None, env: dict[str, str] | None = None) -> dict | None:
    if version_from_tag(tag) != validate_version(version):
        raise ReleaseError("Manifest tag and version disagree")
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]*/[A-Za-z0-9][A-Za-z0-9_.-]*", repository):
        raise ReleaseError("Repository must be owner/name")
    pub_date = pub_date or datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")
    try:
        date = datetime.fromisoformat(pub_date.replace("Z", "+00:00"))
    except ValueError as error:
        raise ReleaseError("Publication date must be RFC3339") from error
    if date.tzinfo is None:
        raise ReleaseError("Publication date must include its timezone")
    paths = asset_inventory(assets, version)
    env = dict(os.environ if env is None else env)
    if not env.get("TAURI_SIGNING_PRIVATE_KEY", "").strip() or not env.get("TAURI_SIGNING_PRIVATE_KEY_PASSWORD", ""):
        print("更新签名私钥或密码 secret 缺失：跳过更新签名和 latest.json。")
        write_checksums(assets, paths)
        return None
    if not env.get("LIZZIE_UPDATER_PUBKEY", "").strip():
        print("LIZZIE_UPDATER_PUBKEY 缺失：本次构建的程序不会检查更新；继续生成签名供既有客户端使用。")
    signatures = {path.name: sign_asset(path, version, env) for path in paths}
    platforms = {}
    targets = {TARGETS[platform]: (platform, UPDATER_SUFFIX[platform]) for platform in PLATFORMS}
    targets.update(BUNDLE_TARGETS)
    for target, (platform, suffix) in targets.items():
        name = asset_name(version, platform, suffix)
        platforms[target] = {
            "url": f"https://github.com/{repository}/releases/download/{quote(tag, safe='')}/{quote(name, safe='')}",
            "signature": signatures[name], "sha256": digest(assets / name),
        }
    result = {"version": version, "notes": notes, "pub_date": pub_date, "platforms": platforms}
    latest = assets / "latest.json"
    latest.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    write_checksums(assets, [*paths, latest, *(path.with_name(path.name + ".sig") for path in paths)])
    print(f"release manifest: ok; {len(paths)} signed assets, {len(platforms)} updater targets")
    return result


def main() -> None:
    # GitHub's Windows runner can redirect Python through a legacy code page.
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    version = commands.add_parser("version")
    version.add_argument("--tag", required=True)
    gather = commands.add_parser("collect")
    gather.add_argument("--platform", required=True, choices=PLATFORMS)
    gather.add_argument("--version", required=True)
    gather.add_argument("--bundle-dir", type=Path, default=Path("target/release/bundle"))
    gather.add_argument("--staged", type=Path, default=Path("target/bundled-resources"))
    gather.add_argument("--acceptance-dir", type=Path)
    gather.add_argument("--output", type=Path, default=Path("target/release-assets"))
    signing = commands.add_parser("manifest")
    signing.add_argument("--assets", type=Path, required=True)
    signing.add_argument("--version", required=True)
    signing.add_argument("--tag", required=True)
    signing.add_argument("--repository", required=True)
    signing.add_argument("--notes-file", type=Path, required=True)
    signing.add_argument("--pub-date")
    args = parser.parse_args()
    try:
        if args.command == "version":
            print(version_from_tag(args.tag))
        elif args.command == "collect":
            for path in collect(args.platform, args.version, args.bundle_dir, args.staged,
                                args.output, args.acceptance_dir):
                print(f"release asset: {path.name} ({path.stat().st_size} bytes)")
        else:
            manifest(args.assets, args.version, args.tag, args.repository,
                     args.notes_file.read_text(encoding="utf-8"), args.pub_date)
    except (ReleaseError, AcceptanceError, OSError, ValueError, subprocess.SubprocessError) as error:
        parser.exit(1, f"release assets: FAILED: {error}\n")


if __name__ == "__main__":
    main()
