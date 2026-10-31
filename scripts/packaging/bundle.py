#!/usr/bin/env python3
"""Prepare pinned, verified installer resources; no downloads during ordinary builds."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import stat
import subprocess
import tempfile
import time
import zipfile

ROOT = Path(__file__).resolve().parents[2]
CATALOG = ROOT / "resources/katago/katago-assets.json"
PINS = ROOT / "resources/katago/bundle-pins.json"
PLATFORMS = {
    "windows-x64": "windows-cpu", "linux-x64": "linux-cpu",
    "macos-arm64": "macos-arm64", "macos-amd64": "macos-amd64",
}


class BundleError(RuntimeError):
    pass


def digest(path):
    sha = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            sha.update(block)
    return sha.hexdigest()


def check_file(path, spec):
    if path.is_symlink() or not path.is_file():
        raise BundleError(f"Not a regular file: {path}")
    if path.stat().st_size != spec["sizeBytes"] or digest(path) != spec["sha256"]:
        raise BundleError(f"SHA-256/size mismatch: {path}")


def fetch(spec, cache):
    """Recheck cache hits; a corrupted cache or download fails closed."""
    if not spec["url"].startswith("https://"):
        raise BundleError("Downloads must use HTTPS")
    sha = spec["sha256"]
    if len(sha) != 64 or any(c not in "0123456789abcdef" for c in sha):
        raise BundleError("Invalid SHA-256 pin")
    cache.mkdir(parents=True, exist_ok=True)
    destination = cache / sha
    if destination.exists() or destination.is_symlink():
        check_file(destination, spec)
        return destination
    with tempfile.TemporaryDirectory(prefix="download-", dir=cache) as temporary:
        partial = Path(temporary) / "payload"
        subprocess.run([
            "curl", "--fail", "--location", "--silent", "--show-error",
            "--proto", "=https", "--proto-redir", "=https", "--retry", "3",
            "--retry-delay", "2", "--connect-timeout", "30", "--max-time", "900",
            "--max-filesize", str(spec["sizeBytes"]), "--output", str(partial), spec["url"],
        ], check=True)
        check_file(partial, spec)
        partial.replace(destination)
    return destination


def extract(archive, destination):
    """Extract data only: reject links, traversal, duplicates and zip bombs."""
    seen = set()
    total = 0
    with zipfile.ZipFile(archive) as zipped:
        for member in zipped.infolist():
            name = PurePosixPath(member.filename)
            mode = member.external_attr >> 16
            # zipfile turns "\\" into "/" on Windows; the raw name keeps what the archive says.
            if (not name.parts or name.is_absolute() or ".." in name.parts
                    or "\\" in member.orig_filename or ":" in member.orig_filename
                    or stat.S_ISLNK(mode)
                    or (stat.S_IFMT(mode) not in (0, stat.S_IFREG, stat.S_IFDIR))):
                raise BundleError(f"Unsafe archive member: {member.filename}")
            key = str(name).casefold()
            if key in seen:
                raise BundleError(f"Duplicate archive member: {member.filename}")
            seen.add(key)
            total += member.file_size
            if total > 2 * 1024**3:
                raise BundleError("Archive exceeds 2 GiB unpacked limit")
            target = destination.joinpath(*name.parts)
            if member.is_dir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                with zipped.open(member) as source, target.open("xb") as output:
                    shutil.copyfileobj(source, output)
                target.chmod(0o755 if mode & 0o111 else 0o644)


def engine_root(directory, executable):
    # Only files count: the engine archives carry licenses/KataGo/, which a case-insensitive
    # file system (macOS) also returns for the name "katago".
    candidates = [path for path in directory.rglob(executable) if path.is_file()]
    if len(candidates) != 1:
        raise BundleError(f"Expected exactly one {executable}, found {len(candidates)}")
    return candidates[0].parent


def patch_readboard(path, spec):
    original_sha = digest(path)
    if spec.get("executableSha256") and original_sha != spec["executableSha256"]:
        raise BundleError("Unexpected original readboard executable")
    original = b'level="requireAdministrator"'
    replacement = b'level="asInvoker"' + b" " * (len(original) - len(b'level="asInvoker"'))
    data = path.read_bytes()
    count = data.count(original)
    if count < 1:
        raise BundleError("Missing readboard elevation manifest")
    path.write_bytes(data.replace(original, replacement))
    packaged_sha = digest(path)
    if spec.get("packagedExecutableSha256") and packaged_sha != spec["packagedExecutableSha256"]:
        raise BundleError("Unexpected patched readboard executable")
    return {"originalSha256": original_sha, "packagedSha256": packaged_sha, "replacements": count}


def macho(path):
    with path.open("rb") as stream:
        return stream.read(4) in (
            b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe", b"\xfe\xed\xfa\xcf",
            b"\xfe\xed\xfa\xce", b"\xca\xfe\xba\xbe", b"\xbe\xba\xfe\xca",
        )


def sign_macos(directory):
    if os.uname().sysname != "Darwin":
        raise BundleError("macOS resources must be prepared and signed on macOS")
    binaries = [p for p in directory.rglob("*") if p.is_file() and macho(p)]
    executable = directory / "katago"
    for binary in sorted(binaries, key=lambda p: p == executable):
        subprocess.run(["codesign", "--force", "--sign", "-", str(binary)], check=True)
        subprocess.run(["codesign", "--verify", "--strict", str(binary)], check=True)
        linked = subprocess.check_output(["otool", "-L", str(binary)], text=True)
        for line in linked.splitlines()[1:]:
            reference = line.strip().split(" (", 1)[0]
            if reference.startswith(("/System/Library/", "/usr/lib/")):
                continue
            if reference.startswith("@executable_path/"):
                resolved = directory / reference.removeprefix("@executable_path/")
            elif reference.startswith("@loader_path/"):
                resolved = binary.parent / reference.removeprefix("@loader_path/")
            else:
                raise BundleError(f"Non-portable Mach-O dependency: {reference}")
            if not resolved.is_file() or not resolved.resolve().is_relative_to(directory.resolve()):
                raise BundleError(f"Missing bundled Mach-O dependency: {reference}")


def inventory(directory):
    return {
        path.relative_to(directory).as_posix(): {
            "sizeBytes": path.stat().st_size, "sha256": digest(path),
            "executable": bool(path.stat().st_mode & 0o111),
        }
        for path in sorted(directory.rglob("*")) if path.is_file()
    }


def prepare(platform, cache, output, catalog=None, pins=None):
    started = time.monotonic()
    catalog = catalog if catalog is not None else json.loads(CATALOG.read_text())
    pins = pins if pins is not None else json.loads(PINS.read_text())
    asset = catalog["assets"][PLATFORMS[platform]]
    # The catalog may name a release that is not published yet; the pins can name a published
    # one carrying the same files. The SHA-256 check is unchanged either way.
    tag = pins.get("engineDownload", {}).get("releaseTag", catalog["engineReleaseTag"])
    engine = dict(asset, url=(f"https://github.com/{catalog['engineReleaseRepository']}/releases/"
                              f"download/{tag}/{asset['assetName']}"))
    model = catalog["models"][catalog["defaultModelId"]]
    model = dict(model, url=model["downloadUrl"])
    downloads = []
    transformations = {}

    def download(spec):
        before = time.monotonic()
        cached = (cache / spec["sha256"]).is_file()
        result = fetch(spec, cache)
        downloads.append({key: spec[key] for key in ("url", "sha256", "sizeBytes")})
        print(f"{'cache' if cached else 'download'}: {spec['url']} "
              f"{spec['sizeBytes']} bytes, SHA-256 {spec['sha256']}, "
              f"{time.monotonic() - before:.2f}s", flush=True)
        return result

    if output.is_symlink() or (output.exists() and not (output / "bundle-manifest.json").is_file()):
        raise BundleError(f"Refusing to replace unmanaged output: {output}")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="bundle-", dir=output.parent) as temporary:
        temporary = Path(temporary)
        staged = temporary / "resources"
        staged.mkdir()
        unpacked = temporary / "engine"
        extract(download(engine), unpacked)
        executable_name = "katago.exe" if platform == "windows-x64" else "katago"
        source = engine_root(unpacked, executable_name)
        if digest(source / executable_name) != asset["executableSha256"]:
            raise BundleError("KataGo executable SHA-256 mismatch")
        destination = staged / "engines/katago" / platform
        shutil.copytree(source, destination)
        (destination / executable_name).chmod(0o755)
        if platform.startswith("macos-"):
            sign_macos(destination)
        weights = staged / "weights"
        weights.mkdir()
        shutil.copyfile(download(model), weights / "default.bin.gz")
        if platform == "windows-x64":
            readboard = pins["readboard"]
            extracted = temporary / "readboard"
            extract(download(readboard), extracted)
            source = engine_root(extracted, "readboard.exe")
            shutil.copytree(source, staged / "readboard")
            # Releases before v3.1 asked for administrator rights; later ones run as invoker.
            if readboard.get("elevationPatch", True):
                transformations["readboard/readboard.exe"] = patch_readboard(
                    staged / "readboard/readboard.exe", readboard)
        for license_spec in pins["licenses"]:
            if platform not in license_spec.get("platforms", PLATFORMS):
                continue
            license_path = staged / license_spec["destination"]
            license_path.parent.mkdir(parents=True, exist_ok=True)
            if "source" in license_spec:
                local_license = ROOT / license_spec["source"]
                check_file(local_license, license_spec)
                shutil.copyfile(local_license, license_path)
            else:
                shutil.copyfile(download(license_spec), license_path)
        files = inventory(staged)
        receipt = {"schemaVersion": 1, "platform": platform, "downloads": downloads,
                   "transformations": transformations, "files": files}
        (staged / "bundle-manifest.json").write_text(json.dumps(receipt, indent=2) + "\n")
        previous = temporary / "previous"
        if output.exists():
            output.replace(previous)
        try:
            staged.replace(output)
        except OSError:
            if previous.exists():
                previous.replace(output)
            raise
    size = sum(item["sizeBytes"] for item in files.values())
    print(f"bundle: {platform}, {len(files)} files, {size} bytes unpacked, "
          f"{time.monotonic() - started:.2f}s; {output}", flush=True)
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["prepare"])
    parser.add_argument("--platform", choices=PLATFORMS, required=True)
    parser.add_argument("--cache", type=Path, default=ROOT / "target/packaging-cache")
    parser.add_argument("--output", type=Path, default=ROOT / "target/bundled-resources")
    args = parser.parse_args()
    try:
        prepare(args.platform, args.cache.resolve(), args.output.absolute())
    except (BundleError, OSError, subprocess.CalledProcessError, zipfile.BadZipFile) as error:
        parser.exit(1, f"bundle: FAILED: {error}\n")


if __name__ == "__main__":
    main()
