"""Offline behavioral fixtures, including Cargo's real target/feature resolver.

CycloneDX schemas are unmodified from specification tag 1.6:
https://github.com/CycloneDX/specification/tree/1.6/schema
The adjacent provenance.json records each download URL and content digest;
LICENSE is the upstream Apache-2.0 license. Tests never retrieve remote schemas.
"""
from __future__ import annotations

import base64
import copy
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from jsonschema import ValidationError
import yaml

import sbom

FIXTURES = Path(__file__).resolve().parents[2] / "fixtures/sbom"


def read_json(name):
    return json.loads((FIXTURES / name).read_text(encoding="utf-8"))


def platform_set(item):
    return set(next(p["value"] for p in item["properties"]
                    if p["name"] == "lizzieyzy:platforms").split(","))


class SbomTests(unittest.TestCase):
    def setUp(self):
        self.lock = yaml.safe_load((FIXTURES / "pnpm-lock.yaml").read_text(encoding="utf-8"))
        self.installed = read_json("npm-installed.json")
        self.catalog = read_json("katago-assets.json")
        self.pins = read_json("bundle-pins.json")

    def sample(self):
        return sbom.document("1.2.3", "2026-11-19T12:34:56-08:00", [],
                             sbom.npm_components(self.lock, self.installed),
                             sbom.katago_components(self.catalog, self.pins))

    def test_frontend_production_closure_peer_variants_aliases_and_licenses(self):
        items = sbom.npm_components(self.lock, self.installed)
        by_name = {item["name"]: item for item in items}
        self.assertEqual(set(by_name), {"@sample/ui", "shared", "real", "second", "optional"})
        self.assertEqual(by_name["@sample/ui"]["purl"], "pkg:npm/%40sample/ui@1.0.0")
        self.assertEqual(by_name["real"]["licenses"], [{"expression": "ISC"}])
        self.assertNotIn("licenses", by_name["optional"])
        self.assertTrue(all(platform_set(item) == set(sbom.TARGETS) for item in items))
        self.assertEqual(by_name["shared"]["hashes"], [{"alg": "SHA-256", "content": bytes(range(32)).hex()}])
        self.assertEqual(by_name["@sample/ui"]["hashes"], [{"alg": "SHA-512", "content": bytes(range(64)).hex()}])

    def test_scoped_npm_alias_with_peer_suffix(self):
        self.lock["snapshots"]["@sample/ui@1.0.0(shared@2.0.0)"]["dependencies"]["alias"] = "@scope/real@3.0.0(shared@2.0.0)"
        self.lock["packages"]["@scope/real@3.0.0"] = self.lock["packages"].pop("real@3.0.0")
        self.lock["snapshots"]["@scope/real@3.0.0(shared@2.0.0)"] = {"dependencies": {"shared": "2.0.0"}}
        self.installed[0]["dependencies"]["@sample/ui"]["dependencies"]["real"]["from"] = "@scope/real"
        item = next(item for item in sbom.npm_components(self.lock, self.installed) if item["name"] == "@scope/real")
        self.assertEqual(item["purl"], "pkg:npm/%40scope/real@3.0.0")

    def test_stale_missing_or_platform_specific_frontend_fails_closed(self):
        with self.assertRaisesRegex(sbom.SbomError, "Install frozen"):
            sbom.npm_components(self.lock, [])
        self.lock["snapshots"].pop("second@1.0.0")
        with self.assertRaisesRegex(sbom.SbomError, "Unresolved"):
            sbom.npm_components(self.lock, self.installed)
        self.lock["snapshots"]["second@1.0.0"] = {}
        self.lock["packages"]["second@1.0.0"]["os"] = ["win32"]
        with self.assertRaisesRegex(sbom.SbomError, "native inventory"):
            sbom.npm_components(self.lock, self.installed)

    def test_integrity_conversion_handles_multiple_algorithms_and_rejects_bad_data(self):
        for algorithm, length in (("sha1", 20), ("sha256", 32), ("sha384", 48), ("sha512", 64)):
            raw = bytes(range(length))
            token = algorithm + "-" + base64.b64encode(raw).decode("ascii")
            self.assertEqual(sbom.integrity_hashes(token + " " + token),
                             [{"alg": algorithm.upper().replace("SHA", "SHA-"), "content": raw.hex()}])
        for bad in ("", "sha256-!", "sha256-YQ==", "md5-AAAAAAAAAAAAAAAAAAAAAA=="):
            with self.subTest(bad=bad), self.assertRaises(sbom.SbomError):
                sbom.integrity_hashes(bad)

    def test_katago_uses_only_bundled_engines_and_default_weight_with_pinned_release(self):
        items = sbom.katago_components(self.catalog, self.pins)
        self.assertEqual(len(items), 5)
        engines = [item for item in items if item["name"] == "KataGo"]
        self.assertEqual({next(iter(platform_set(item))) for item in engines}, set(sbom.TARGETS))
        for item in engines:
            platform = next(iter(platform_set(item)))
            asset = self.catalog["assets"][sbom.ENGINE_ASSETS[platform]]
            self.assertEqual(item["hashes"], [{"alg": "SHA-256", "content": asset["sha256"]}])
            self.assertIn("/pinned-release/", item["externalReferences"][0]["url"])
            self.assertIn({"name": "lizzieyzy:source-executable-sha256", "value": asset["executableSha256"]}, item["properties"])
        network = next(item for item in items if item["type"] == "machine-learning-model")
        self.assertEqual(network["version"], "network-s123.bin.gz")
        self.assertEqual(platform_set(network), set(sbom.TARGETS))
        self.assertEqual(network["hashes"][0]["content"], "e" * 64)
        self.assertEqual(network["licenses"][0]["license"]["url"], "https://example.org/network-license")

    def test_deterministic_bytes_and_commit_timestamp(self):
        original = self.sample()
        self.assertEqual(original["metadata"]["timestamp"], "2026-11-19T20:34:56Z")
        reordered = sbom.document("1.2.3", "2026-11-19T20:34:56+00:00", [],
                                  list(reversed(sbom.npm_components(self.lock, self.installed))),
                                  list(reversed(sbom.katago_components(self.catalog, self.pins))))
        self.assertEqual(sbom.encode(original), sbom.encode(reordered))
        for timestamp in ("today", "2026-11-19T20:34:56"):
            with self.assertRaises(sbom.SbomError):
                sbom.document("1.2.3", timestamp, [], [], [])

    def test_complete_official_schema_validation_is_offline_and_rejects_invalid_boms(self):
        value = self.sample()
        with patch("socket.socket", side_effect=AssertionError("Tests must stay offline")):
            sbom.validate_schema(value)
            for mutate in (
                lambda bom: bom.update(bomFormat="wrong"),
                lambda bom: bom["components"][0].update(type="not-a-component-type"),
                lambda bom: bom["components"][0]["hashes"][0].update(content="not-hex"),
                lambda bom: bom["metadata"].update(timestamp="not-a-date"),
            ):
                invalid = copy.deepcopy(value)
                mutate(invalid)
                with self.assertRaises(ValidationError):
                    sbom.validate_schema(invalid)
            # Exercise the vendored SPDX schema reference, not just local refs.
            value["components"][0]["licenses"] = [{"license": {"id": "MIT"}}]
            sbom.validate_schema(value)
            value["components"][0]["licenses"] = [{"license": {"id": "Not-A-License"}}]
            with self.assertRaises(ValidationError):
                sbom.validate_schema(value)

    def test_current_rust_release_resolution_excludes_dev_build_and_proc_macro_subtrees(self):
        with tempfile.TemporaryDirectory(prefix="sbom-cargo-") as temporary:
            root = Path(temporary) / "workspace"
            shutil.copytree(FIXTURES / "cargo-workspace", root)
            env = dict(os.environ, CARGO_NET_OFFLINE="true")
            subprocess.run(["cargo", "generate-lockfile", "--offline"], cwd=root,
                           env=env, check=True, capture_output=True, timeout=5)

            def offline_run(arguments, directory):
                return subprocess.check_output(arguments, cwd=directory, env=env,
                                               encoding="utf-8", timeout=5)

            with patch.object(sbom, "run", side_effect=offline_run):
                trees = sbom.cargo_trees(root)
            with (root / "Cargo.lock").open("rb") as stream:
                lock = sbom.tomllib.load(stream)
            items = sbom.rust_components(trees, lock, root)
            by_name = {item["name"]: item for item in items}
            self.assertEqual(set(by_name), {"runtime", "release-only", "windows-only", "macos-only",
                                            "linux-only", "macos-arm-only"})
            self.assertEqual(platform_set(by_name["runtime"]), set(sbom.TARGETS))
            self.assertEqual(platform_set(by_name["release-only"]), set(sbom.TARGETS))
            self.assertEqual(platform_set(by_name["windows-only"]), {"windows-x64"})
            self.assertEqual(platform_set(by_name["macos-only"]), {"macos-arm64", "macos-amd64"})
            self.assertEqual(platform_set(by_name["macos-arm-only"]), {"macos-arm64"})
            self.assertEqual(platform_set(by_name["linux-only"]), {"linux-x64"})
            self.assertTrue(all("hashes" not in item for item in items))
            registry = "registry+https://github.com/rust-lang/crates.io-index"
            lock["package"].append({"name": "source-crate", "version": "1.2.3", "source": registry,
                                     "checksum": "a" * 64})
            trees["linux-x64"] += "source-crate v1.2.3\tMIT/Apache-2.0\n"
            item = next(item for item in sbom.rust_components(trees, lock, root) if item["name"] == "source-crate")
            self.assertEqual(item["purl"], "pkg:cargo/source-crate@1.2.3")
            self.assertEqual(item["hashes"], [{"alg": "SHA-256", "content": "a" * 64}])
            self.assertEqual(item["licenses"], [{"expression": "MIT OR Apache-2.0"}])
            lock["package"][-1]["source"] = "git+https://example.org/source-crate"
            with self.assertRaisesRegex(sbom.SbomError, "Unsupported Rust source"):
                sbom.rust_components(trees, lock, root)
            lock["package"].pop()
            with self.assertRaisesRegex(sbom.SbomError, "Expected one locked Rust package"):
                sbom.rust_components(trees, lock, root)
            with self.assertRaisesRegex(sbom.SbomError, "All four"):
                sbom.rust_components({}, lock, root)

    def test_unknown_schema_reference_is_rejected_without_network(self):
        with tempfile.TemporaryDirectory(prefix="sbom-schema-") as temporary:
            directory = Path(temporary)
            for name in ("bom-1.6.schema.json", "spdx.schema.json", "jsf-0.82.schema.json"):
                shutil.copyfile(FIXTURES / "schema" / name, directory / name)
            path = directory / "bom-1.6.schema.json"
            schema = json.loads(path.read_text(encoding="utf-8"))
            schema["$ref"] = "https://example.invalid/missing.schema.json"
            path.write_text(json.dumps(schema), encoding="utf-8")
            with patch("socket.socket", side_effect=AssertionError("Tests must stay offline")):
                with self.assertRaisesRegex(Exception, "Unresolvable: https://example.invalid/"):
                    sbom.validate_schema(self.sample(), directory)


if __name__ == "__main__":
    unittest.main()
