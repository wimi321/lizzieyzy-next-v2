#!/usr/bin/env python3
"""Deterministic CycloneDX 1.6 inventory of the four release payloads.

Requires Python 3.12, the frozen frontend installation and the Rust toolchain.
Install the hashed SBOM-only environment with:
python -m pip install --require-hashes --only-binary :all: -r scripts/packaging/sbom-requirements.txt
No package is compiled.
Cargo resolves target/feature-specific normal edges, excluding build scripts,
proc-macro-only subtrees and dev dependencies. Hashes describe upstream source
archives, not linked/tree-shaken or publisher-signed output bytes.
"""
from __future__ import annotations

import argparse
import base64
from datetime import datetime, timezone
import json
from pathlib import Path
import re
import subprocess
from urllib.parse import quote

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib

import yaml
from jsonschema.exceptions import SchemaError, ValidationError

from release_assets import sbom_name, validate_version

ROOT = Path(__file__).resolve().parents[2]
TARGETS = {
    "windows-x64": "x86_64-pc-windows-msvc",
    "macos-arm64": "aarch64-apple-darwin",
    "macos-amd64": "x86_64-apple-darwin",
    "linux-x64": "x86_64-unknown-linux-gnu",
}
ENGINE_ASSETS = {"windows-x64": "windows-cpu", "linux-x64": "linux-cpu",
                 "macos-arm64": "macos-arm64", "macos-amd64": "macos-amd64"}
SCHEMA = ROOT / "fixtures/sbom/schema"
SHA256 = re.compile(r"[0-9a-f]{64}")


class SbomError(ValueError):
    pass


def run(arguments: list[str], root: Path) -> str:
    return subprocess.check_output(arguments, cwd=root, text=True, encoding="utf-8")


def properties(platforms, ecosystem, **extra):
    values = {"ecosystem": ecosystem, "platforms": ",".join(sorted(platforms)), **extra}
    return [{"name": "lizzieyzy:" + key, "value": str(value)}
            for key, value in sorted(values.items())]


def license_entries(value):
    if isinstance(value, dict):
        value = value.get("type")
    if not isinstance(value, str) or not value.strip():
        return []
    value = value.strip()
    # Cargo historically accepted '/' as an OR separator in license metadata.
    if re.fullmatch(r"[A-Za-z0-9.+-]+/[A-Za-z0-9.+-]+", value):
        value = value.replace("/", " OR ")
    if value.startswith("SEE LICENSE IN ") or value == "UNLICENSED":
        return [{"license": {"name": value}}]
    return [{"expression": value}]


def component(name, version, purl, platforms, ecosystem, license_value=None, **extra):
    result = {"type": "library", "bom-ref": purl, "name": name, "version": version,
              "purl": purl, "properties": properties(platforms, ecosystem, **extra)}
    licenses = license_entries(license_value)
    if licenses:
        result["licenses"] = licenses
    return result


def sha256_hash(value):
    if not isinstance(value, str) or not SHA256.fullmatch(value):
        raise SbomError("Invalid SHA-256 in dependency metadata")
    return {"alg": "SHA-256", "content": value}


def rust_components(trees: dict[str, str], lock: dict, root: Path) -> list[dict]:
    """Read Cargo's resolved normal graph, never traverse a workspace-wide graph."""
    if set(trees) != set(TARGETS):
        raise SbomError("All four release target graphs are required")
    packages = {}
    for platform, tree in trees.items():
        saw_root = False
        for line in tree.splitlines():
            line = line.removesuffix(" (*)")
            match = re.fullmatch(r"([^\s]+) v([^\s]+)(?: \((.*)\))?\t(.*)", line)
            if not match:
                raise SbomError(f"Unrecognized cargo tree entry: {line}")
            name, version, location, license_value = match.groups()
            local = location is not None
            if local:
                path = Path(location).resolve()
                if not path.is_relative_to(root.resolve()):
                    raise SbomError(f"Non-workspace Rust dependency: {name}")
                if path.is_relative_to((root / "tools").resolve()):
                    raise SbomError(f"Tool unexpectedly included in desktop runtime: {name}")
            if name == "lizzieyzy-desktop":
                saw_root = True
                continue  # The application itself is metadata.component.
            key = (name, version)
            if key in packages and packages[key]["license"] != license_value:
                raise SbomError(f"Conflicting Rust package metadata: {name}@{version}")
            item = packages.setdefault(key, {"platforms": set(), "license": license_value,
                                             "local": local})
            if item["local"] != local:
                raise SbomError(f"Ambiguous Rust package identity: {name}@{version}")
            item["platforms"].add(platform)
        if not saw_root:
            raise SbomError(f"Desktop root missing from {platform} graph")
    result = []
    for (name, version), data in sorted(packages.items()):
        records = [item for item in lock["package"]
                   if item["name"] == name and item["version"] == version
                   and ("source" not in item) == data["local"]]
        if len(records) != 1:
            raise SbomError(f"Expected one locked Rust package: {name}@{version}")
        record = records[0]
        if not data["local"] and record.get("source") != "registry+https://github.com/rust-lang/crates.io-index":
            raise SbomError(f"Unsupported Rust source: {name}@{version}")
        item = component(name, version, f"pkg:cargo/{quote(name)}@{quote(version)}",
                         data["platforms"], "cargo", data["license"],
                         **{"hash-artifact": "crate-archive"} if not data["local"] else {})
        if not data["local"]:
            item["hashes"] = [sha256_hash(record.get("checksum"))]
        result.append(item)
    return result


def integrity_hashes(integrity: str) -> list[dict]:
    algorithms = {"sha1": ("SHA-1", 20), "sha256": ("SHA-256", 32),
                  "sha384": ("SHA-384", 48), "sha512": ("SHA-512", 64)}
    result = set()
    for token in integrity.split():
        algorithm, separator, encoded = token.partition("-")
        if not separator or algorithm not in algorithms:
            raise SbomError("Unsupported pnpm integrity algorithm")
        try:
            raw = base64.b64decode(encoded, validate=True)
        except ValueError as error:
            raise SbomError("Malformed pnpm integrity") from error
        label, size = algorithms[algorithm]
        if len(raw) != size:
            raise SbomError("Invalid pnpm integrity length")
        result.add((label, raw.hex()))
    if not result:
        raise SbomError("Missing pnpm integrity")
    return [{"alg": label, "content": content} for label, content in sorted(result)]


def npm_metadata(installed: list[dict]) -> dict:
    result = {}
    pending = list(installed)
    while pending:
        item = pending.pop()
        if item.get("from") and item.get("version"):
            key = (item["from"], item["version"])
            old = result.setdefault(key, item.get("license"))
            if old != item.get("license"):
                raise SbomError(f"Conflicting npm license metadata: {key}")
        for kind in ("dependencies", "optionalDependencies"):
            pending.extend(item.get(kind, {}).values())
    return result


def npm_components(lock: dict, installed: list[dict]) -> list[dict]:
    if str(lock.get("lockfileVersion")) != "9.0":
        raise SbomError("Expected pnpm lockfile version 9.0")
    metadata = npm_metadata(installed)
    importer = lock["importers"]["."]
    pending = []
    for kind in ("dependencies", "optionalDependencies"):
        pending.extend((name, entry["version"]) for name, entry in importer.get(kind, {}).items())
    visited, found = set(), {}
    while pending:
        name, resolved = pending.pop()
        # pnpm resolves npm aliases to real-name@version; peer suffixes identify
        # graph nodes, but every peer variant has the same registry tarball.
        base, separator, peers = resolved.partition("(")
        if "@" in base:
            name, base = base.rsplit("@", 1)
            resolved = base + separator + peers
        node = f"{name}@{resolved}"
        if node in visited:
            continue
        visited.add(node)
        version = resolved.split("(", 1)[0]
        key = (name, version)
        record = lock["packages"].get(f"{name}@{version}")
        snapshot = lock["snapshots"].get(node)
        if record is None or snapshot is None:
            raise SbomError(f"Unresolved pnpm production dependency: {node}")
        if key not in metadata:
            raise SbomError(f"Install frozen frontend dependencies before generating SBOM: {node}")
        if record.get("os") or record.get("cpu") or record.get("libc"):
            raise SbomError(f"Platform-specific npm runtime package needs native inventory: {node}")
        purl = f"pkg:npm/{quote(name, safe='/')}@{quote(version)}"
        item = component(name, version, purl, TARGETS, "npm", metadata[key],
                         **{"hash-artifact": "npm-tarball"})
        item["hashes"] = integrity_hashes(record.get("resolution", {}).get("integrity", ""))
        found[key] = item
        for kind in ("dependencies", "optionalDependencies"):
            pending.extend(snapshot.get(kind, {}).items())
    return list(found.values())


def katago_components(catalog: dict, pins: dict) -> list[dict]:
    result = []
    tag = pins.get("engineDownload", {}).get("releaseTag", catalog["engineReleaseTag"])
    for platform, key in ENGINE_ASSETS.items():
        asset = catalog["assets"][key]
        url = (f"https://github.com/{catalog['engineReleaseRepository']}/releases/download/"
               f"{quote(tag, safe='')}/{quote(asset['assetName'], safe='')}")
        item = component("KataGo", catalog["katagoVersion"],
                         f"pkg:generic/katago@{quote(catalog['katagoVersion'])}?platform={platform}",
                         [platform], "katago", asset.get("license", catalog.get("license")),
                         **{"hash-artifact": "engine-download-archive",
                            "source-executable-sha256": sha256_hash(asset["executableSha256"])["content"],
                            "source-commit": catalog["katagoSourceCommit"], "backend": asset["backend"]})
        item["type"] = "application"
        item["hashes"] = [sha256_hash(asset["sha256"])]
        item["externalReferences"] = [{"type": "distribution", "url": url}]
        result.append(item)
    model = catalog["models"][catalog["defaultModelId"]]
    # Model catalog has no SemVer: the full immutable network filename is its
    # identity/version. The release tag of the engine is not the model version.
    version = model["fileName"]
    item = component("KataGo neural network", version,
                     f"pkg:generic/katago-network@{quote(version, safe='')}", TARGETS,
                     "katago", model.get("license"), **{"hash-artifact": "compressed-network"})
    item["type"] = "machine-learning-model"
    item["hashes"] = [sha256_hash(model["sha256"])]
    item["externalReferences"] = [{"type": "distribution", "url": model["downloadUrl"]}]
    if "licenses" not in item:
        notices = [entry for entry in pins.get("licenses", []) if entry["id"] == "katago-network"]
        if notices and notices[0].get("sourceUrl"):
            item["licenses"] = [{"license": {"name": "KataGo network license",
                                               "url": notices[0]["sourceUrl"]}}]
    result.append(item)
    return result


def document(version, timestamp, rust, npm, katago):
    validate_version(version)
    try:
        date = datetime.fromisoformat(timestamp.replace("Z", "+00:00"))
    except ValueError as error:
        raise SbomError("Commit timestamp must be RFC3339") from error
    if date.tzinfo is None:
        raise SbomError("Commit timestamp must include a timezone")
    components = sorted([*rust, *npm, *katago], key=lambda item: item["bom-ref"])
    if len({item["bom-ref"] for item in components}) != len(components):
        raise SbomError("Duplicate SBOM component identity")
    return {"$schema": "http://cyclonedx.org/schema/bom-1.6.schema.json",
            "bomFormat": "CycloneDX", "specVersion": "1.6", "version": 1,
            "metadata": {"timestamp": date.astimezone(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z"),
                         "component": {"type": "application", "name": "LizzieYzy Next 2",
                                       "version": version, "purl": f"pkg:generic/lizzieyzy-next-2@{quote(version)}",
                                       "licenses": [{"expression": "GPL-3.0-only"}]},
                         "properties": [{"name": "lizzieyzy:inventory-scope", "value":
                             "Release normal Rust dependency graphs and frontend production dependency closure; "
                             "excludes dev/build/proc-macro-only tooling. Archive hashes precede linking, "
                             "frontend tree shaking and publisher signing; not a binary symbol inventory."}]},
            "components": components}


def encode(value: dict) -> bytes:
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode("utf-8")


def validate_schema(value: dict, directory: Path = SCHEMA) -> None:
    """Only vendored references are resolvable; an unknown URI fails closed."""
    from jsonschema import Draft7Validator, FormatChecker
    from referencing import Registry, Resource
    from referencing.exceptions import NoSuchResource

    def no_network(uri):
        raise NoSuchResource(ref=uri)

    registry = Registry(retrieve=no_network)
    schema = None
    for name in ("bom-1.6.schema.json", "spdx.schema.json", "jsf-0.82.schema.json"):
        data = json.loads((directory / name).read_text(encoding="utf-8"))
        resource = Resource.from_contents(data)
        registry = registry.with_resource("http://cyclonedx.org/schema/" + name, resource)
        if data.get("$id"):
            registry = registry.with_resource(data["$id"], resource)
        if name == "bom-1.6.schema.json":
            schema = data
    Draft7Validator(schema, registry=registry, format_checker=FormatChecker()).validate(value)


def cargo_trees(root: Path) -> dict[str, str]:
    return {platform: run(["cargo", "tree", "--locked", "--color", "never", "-p", "lizzieyzy-desktop",
                            "--target", target, "--no-default-features", "--features", "custom-protocol",
                            "--edges", "normal,no-proc-macro", "--prefix", "none", "--format", "{p}\t{l}"], root)
             for platform, target in TARGETS.items()}


def generate(root: Path, version: str, timestamp: str | None = None) -> dict:
    trees = cargo_trees(root)
    with (root / "Cargo.lock").open("rb") as stream:
        rust = rust_components(trees, tomllib.load(stream), root)
    frontend = root / "apps/desktop"
    installed = json.loads(run(["pnpm", "-C", str(frontend), "list", "--prod", "--depth", "Infinity",
                                "--json", "--long"], root))
    npm = npm_components(yaml.safe_load((frontend / "pnpm-lock.yaml").read_text(encoding="utf-8")), installed)
    directory = root / "resources/katago"
    katago = katago_components(json.loads((directory / "katago-assets.json").read_text(encoding="utf-8")),
                              json.loads((directory / "bundle-pins.json").read_text(encoding="utf-8")))
    timestamp = timestamp or run(["git", "show", "-s", "--format=%cI", "HEAD"], root).strip()
    result = document(version, timestamp, rust, npm, katago)
    validate_schema(result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True)
    parser.add_argument("--output", type=Path, default=ROOT / "target/release-assets")
    args = parser.parse_args()
    try:
        result = generate(ROOT, args.version)
        args.output.mkdir(parents=True, exist_ok=True)
        output = args.output / sbom_name(args.version)
        with output.open("xb") as stream:
            stream.write(encode(result))
        counts = {kind: sum(any(p["name"] == "lizzieyzy:ecosystem" and p["value"] == kind
                                for p in item["properties"]) for item in result["components"])
                  for kind in ("cargo", "npm", "katago")}
        print(f"sbom: schema ok; {counts}; {output}")
    except (OSError, ValueError, SchemaError, ValidationError, subprocess.SubprocessError) as error:
        parser.exit(1, f"sbom: FAILED: {error}\n")


if __name__ == "__main__":
    main()
