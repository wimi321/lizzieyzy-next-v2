"""Behavior tests use tiny archives and an injected downloader, never a real engine."""
import hashlib
import json
from pathlib import Path
import stat
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import bundle


def pin(data, url="https://example.invalid/payload"):
    return {"url": url, "sha256": hashlib.sha256(data).hexdigest(), "sizeBytes": len(data)}


class BundleTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.addCleanup(self.temporary.cleanup)

    def archive(self, entries):
        archive = self.root / "archive.zip"
        with zipfile.ZipFile(archive, "w") as zipped:
            for name, data in entries:
                zipped.writestr(name, data)
        return archive

    def test_cached_payload_is_rehashed_and_corruption_fails_without_network(self):
        spec = pin(b"good")
        cache = self.root / "cache"
        cache.mkdir()
        path = cache / spec["sha256"]
        path.write_bytes(b"good")
        with patch("bundle.subprocess.run") as run:
            self.assertEqual(bundle.fetch(spec, cache), path)
            path.write_bytes(b"evil")
            with self.assertRaisesRegex(bundle.BundleError, "mismatch"):
                bundle.fetch(spec, cache)
            run.assert_not_called()

    def test_download_published_only_after_hash_and_size_match(self):
        spec = pin(b"good")
        cache = self.root / "cache"

        def downloader(command, **kwargs):
            Path(command[command.index("--output") + 1]).write_bytes(b"evil")

        with patch("bundle.subprocess.run", side_effect=downloader):
            with self.assertRaisesRegex(bundle.BundleError, "mismatch"):
                bundle.fetch(spec, cache)
        self.assertEqual(list(cache.iterdir()), [])
        with patch("bundle.subprocess.run", side_effect=lambda command, **kw:
                   Path(command[command.index("--output") + 1]).write_bytes(b"good")):
            self.assertEqual(bundle.fetch(spec, cache).read_bytes(), b"good")

    def test_failed_download_leaves_no_cache_payload(self):
        with patch("bundle.subprocess.run", side_effect=subprocess.CalledProcessError(22, "curl")):
            with self.assertRaises(subprocess.CalledProcessError):
                bundle.fetch(pin(b"good"), self.root / "cache")
        self.assertEqual(list((self.root / "cache").iterdir()), [])

    def test_zip_rejects_traversal_links_and_case_collisions(self):
        link = zipfile.ZipInfo("link")
        link.create_system = 3
        link.external_attr = (stat.S_IFLNK | 0o777) << 16
        cases = [
            [("../escaped", b"bad")], [("/absolute", b"bad")],
            [("folder\\escaped", b"bad")], [("C:/escaped", b"bad")],
            [(link, b"/etc/passwd")], [("FILE", b"a"), ("file", b"b")],
        ]
        for index, entries in enumerate(cases):
            with self.subTest(entries=entries), self.assertRaises(bundle.BundleError):
                bundle.extract(self.archive(entries), self.root / f"extract-{index}")
        self.assertFalse((self.root / "escaped").exists())

    def test_zip_keeps_nested_runtime_files_and_permissions(self):
        executable = zipfile.ZipInfo("engine/katago")
        executable.create_system = 3
        executable.external_attr = (stat.S_IFREG | 0o755) << 16
        archive = self.archive([(executable, b"engine"), ("engine/lib/runtime.dylib", b"library")])
        output = self.root / "extract"
        bundle.extract(archive, output)
        self.assertEqual(bundle.engine_root(output, "katago"), output / "engine")
        self.assertEqual((output / "engine/lib/runtime.dylib").read_bytes(), b"library")
        if bundle.os.name != "nt":
            self.assertTrue((output / "engine/katago").stat().st_mode & 0o111)

    def test_readboard_patch_checks_both_hashes(self):
        original = b'header level="requireAdministrator" tail'
        expected = b'header level="asInvoker"            tail'
        path = self.root / "readboard.exe"
        path.write_bytes(original)
        spec = {"executableSha256": pin(original)["sha256"],
                "packagedExecutableSha256": pin(expected)["sha256"]}
        bundle.patch_readboard(path, spec)
        self.assertEqual(path.read_bytes(), expected)
        with self.assertRaises(bundle.BundleError):
            bundle.patch_readboard(path, spec)

    def test_prepare_layout_receipt_and_failed_replacement_preserves_output(self):
        archive = self.archive([("payload/katago", b"engine"),
                                ("payload/licenses/KataGo.txt", b"license"),
                                ("payload/lib/runtime.so", b"runtime")])
        engine = pin(archive.read_bytes())
        engine.update(assetName="engine.zip", executableSha256=pin(b"engine")["sha256"])
        model = pin(b"model")
        model["downloadUrl"] = model["url"]
        catalog = {"assets": {"linux-cpu": engine}, "engineReleaseRepository": "owner/repo",
                   "engineReleaseTag": "fixed", "models": {"b11": model}, "defaultModelId": "b11"}
        model_path = self.root / "model"
        model_path.write_bytes(b"model")
        output = self.root / "resources"
        with patch("bundle.fetch", side_effect=[archive, model_path]):
            receipt = bundle.prepare("linux-x64", self.root / "cache", output, catalog, {"licenses": []})
        self.assertEqual((output / "weights/default.bin.gz").read_bytes(), b"model")
        self.assertEqual((output / "engines/katago/linux-x64/lib/runtime.so").read_bytes(), b"runtime")
        self.assertEqual(receipt["files"], {
                             key: value for key, value in bundle.inventory(output).items()
                             if key != "bundle-manifest.json"})
        self.assertEqual(json.loads((output / "bundle-manifest.json").read_text()), receipt)
        with patch("bundle.fetch", side_effect=bundle.BundleError("mismatch")):
            with self.assertRaises(bundle.BundleError):
                bundle.prepare("linux-x64", self.root / "cache", output, catalog, {"licenses": []})
        self.assertEqual(json.loads((output / "bundle-manifest.json").read_text()), receipt)

    def test_engine_root_ignores_directories_named_like_the_engine(self):
        root = self.root / "unpacked"
        (root / "licenses" / "katago").mkdir(parents=True)
        (root / "licenses" / "katago" / "LICENSE").write_text("license")
        (root / "katago").write_bytes(b"engine")
        self.assertEqual(bundle.engine_root(root, "katago"), root)

    def test_pins_can_download_the_engine_from_a_published_release(self):
        archive = self.archive([("payload/katago", b"engine")])
        engine = pin(archive.read_bytes())
        engine.update(assetName="engine.zip", executableSha256=pin(b"engine")["sha256"])
        model = pin(b"model")
        model["downloadUrl"] = model["url"]
        catalog = {"assets": {"linux-cpu": engine}, "engineReleaseRepository": "owner/repo",
                   "engineReleaseTag": "draft", "models": {"b11": model}, "defaultModelId": "b11"}
        model_path = self.root / "model"
        model_path.write_bytes(b"model")
        with patch("bundle.fetch", side_effect=[archive, model_path]) as fetch:
            bundle.prepare("linux-x64", self.root / "cache", self.root / "resources", catalog,
                           {"licenses": [], "engineDownload": {"releaseTag": "published"}})
        first = fetch.call_args_list[0].args[0]
        self.assertEqual(first["url"], "https://github.com/owner/repo/releases/download/published/engine.zip")
        self.assertEqual(first["sha256"], engine["sha256"])

    def test_refuses_to_replace_unmanaged_output(self):
        output = self.root / "resources"
        output.mkdir()
        (output / "user-file").write_text("keep")
        # A valid minimal catalog reaches the output ownership check without network.
        catalog = {"assets": {"linux-cpu": {"assetName": "x"}},
                   "engineReleaseRepository": "a/b", "engineReleaseTag": "fixed",
                   "models": {"b11": {"downloadUrl": "https://invalid"}}, "defaultModelId": "b11"}
        with self.assertRaisesRegex(bundle.BundleError, "unmanaged"):
            bundle.prepare("linux-x64", self.root / "cache", output, catalog, {"licenses": []})
        self.assertEqual((output / "user-file").read_text(), "keep")

    def test_windows_preparation_keeps_runtime_and_records_manifest_change(self):
        engine_zip = self.archive([("katago.exe", b"engine"),
                                   ("licenses/KataGo.txt", b"license")])
        engine_zip.rename(self.root / "engine.zip")
        engine_zip = self.root / "engine.zip"
        original = b'header level="requireAdministrator" tail'
        readboard_zip = self.archive([("readboard/readboard.exe", original),
                                      ("readboard/coreclr.dll", b"runtime"),
                                      ("readboard/LICENSE.txt", b"runtime license")])
        engine = pin(engine_zip.read_bytes())
        engine.update(assetName="engine.zip", executableSha256=pin(b"engine")["sha256"])
        model = pin(b"model")
        model["downloadUrl"] = model["url"]
        model_path = self.root / "model"
        model_path.write_bytes(b"model")
        license_path = self.root / "network-license"
        license_path.write_bytes(b"network license")
        license_spec = dict(pin(b"network license"), source=str(license_path),
                            destination="weights/LICENSE.txt")
        catalog = {"assets": {"windows-cpu": engine}, "engineReleaseRepository": "owner/repo",
                   "engineReleaseTag": "fixed", "models": {"b11": model}, "defaultModelId": "b11"}
        pins = {"readboard": pin(readboard_zip.read_bytes()), "licenses": [license_spec]}
        output = self.root / "output"
        with patch("bundle.fetch", side_effect=[engine_zip, model_path, readboard_zip]):
            receipt = bundle.prepare("windows-x64", self.root / "cache", output, catalog, pins)
        self.assertEqual((output / "readboard/coreclr.dll").read_bytes(), b"runtime")
        self.assertEqual((output / "readboard/LICENSE.txt").read_bytes(), b"runtime license")
        self.assertEqual((output / "weights/LICENSE.txt").read_bytes(), b"network license")
        transformation = receipt["transformations"]["readboard/readboard.exe"]
        self.assertEqual(transformation["originalSha256"], pin(original)["sha256"])
        self.assertEqual(transformation["packagedSha256"],
                         bundle.digest(output / "readboard/readboard.exe"))
        self.assertEqual(transformation["replacements"], 1)
        self.assertNotEqual(transformation["originalSha256"], transformation["packagedSha256"])

    def test_signing_rejects_unbundled_library_after_verifying_signature(self):
        binary = self.root / "katago"
        binary.write_bytes(b"\xcf\xfa\xed\xfecode")
        with patch("bundle.os.uname", create=True) as uname, \
                patch("bundle.subprocess.run") as run, \
                patch("bundle.subprocess.check_output", return_value="katago:\n\t/opt/homebrew/lib/libx.dylib (version 1)\n"):
            uname.return_value.sysname = "Darwin"
            with self.assertRaisesRegex(bundle.BundleError, "Non-portable"):
                bundle.sign_macos(self.root)
        self.assertEqual(run.call_count, 2)


if __name__ == "__main__":
    unittest.main()
