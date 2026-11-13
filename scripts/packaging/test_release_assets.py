"""Release helper behavior, including real temporary Tauri keys and signatures."""
import base64
import contextlib
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile

from acceptance import AcceptanceError, digest
from release_assets import (
    BUNDLE_TARGETS, EXTENSIONS, PLATFORMS, ReleaseError,
    asset_inventory, asset_name, collect, macos_update_archive, manifest,
    sign_asset, signature_version, tauri_command, validate_version,
    verify_signature, version_from_tag, windows_portable,
)


VERSION = "0.2.0-preview.12"


class ReleaseAssetsTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)

    def inventory(self):
        assets = self.base / "assets"
        assets.mkdir()
        for platform in PLATFORMS:
            for suffix in EXTENSIONS[platform]:
                name = asset_name(VERSION, platform, suffix)
                (assets / name).write_bytes(("payload for " + name).encode())
        return assets

    def payload(self, platform):
        acceptance = self.base / "acceptance"
        payload = acceptance / "payload"
        staged = self.base / "staged"
        staged.mkdir()
        if platform == "windows-x64":
            resources = payload
            application = resources / "lizzieyzy-desktop.exe"
        else:
            resources = payload / "LizzieYzy Next 2.app/Contents/Resources"
            application = resources.parent / "MacOS/lizzieyzy-desktop"
        resources.mkdir(parents=True)
        application.parent.mkdir(parents=True, exist_ok=True)
        application.write_bytes(b"signed production executable")
        application.chmod(0o755)
        files = {f"engines/katago/{platform}/katago" + (".exe" if platform == "windows-x64" else ""): b"engine",
                 "weights/default.bin.gz": b"model", "licenses/OpenSans-LICENSE.txt": b"license"}
        if platform == "windows-x64":
            files["readboard/readboard.exe"] = b"readboard"
            files["readboard/runtime.dll"] = b"runtime"
        receipt = {"schemaVersion": 1, "platform": platform, "files": {}}
        for name, contents in files.items():
            path = resources / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(contents)
            receipt["files"][name] = {"sizeBytes": len(contents), "sha256": digest(path), "executable": False}
        for root in (staged, resources):
            (root / "bundle-manifest.json").write_text(json.dumps(receipt), encoding="utf-8")
        (acceptance / "result.json").write_text(json.dumps({
            "platform": platform, "resourceRoot": str(resources), "application": str(application),
        }), encoding="utf-8")
        return acceptance, staged, resources, application

    def record_installer(self, acceptance, installer):
        result_path = acceptance / "result.json"
        result = json.loads(result_path.read_text(encoding="utf-8"))
        result.update(installerBytes=installer.stat().st_size, installerSha256=digest(installer))
        result_path.write_text(json.dumps(result), encoding="utf-8")

    def test_version_accepts_semver_and_rejects_injection_and_windows_overflow(self):
        for version in ("0.1.0", VERSION, "1.2.3+build.4", "65535.65535.65535"):
            self.assertEqual(version_from_tag("v" + version), version)
        for tag in ("0.1.0", "v1.2", "v01.2.3", "v1.2.3-01", "v65536.1.1",
                    "v1.2.3\nBAD=1", "v1.2.3/asset", "v1.2.3;echo x"):
            with self.subTest(tag=tag), self.assertRaises(ReleaseError):
                version_from_tag(tag)
        with self.assertRaises(ReleaseError):
            validate_version("1.2.3 ")

    def test_collect_linux_preserves_exact_installers_and_names(self):
        bundle = self.base / "bundle"
        for directory, name in (("appimage", "Input.AppImage"), ("deb", "Input.deb")):
            (bundle / directory).mkdir(parents=True)
            (bundle / directory / name).write_bytes(name.encode())
        paths = collect("linux-x64", VERSION, bundle, self.base / "unused", self.base / "out")
        self.assertEqual([path.read_bytes() for path in paths], [b"Input.AppImage", b"Input.deb"])
        self.assertEqual([path.name for path in paths],
                         [asset_name(VERSION, "linux-x64", suffix) for suffix in EXTENSIONS["linux-x64"]])
        with self.assertRaisesRegex(ReleaseError, "overwrite"):
            collect("linux-x64", VERSION, bundle, self.base / "unused", self.base / "out")

    def test_collect_rejects_ambiguous_installer(self):
        bundle = self.base / "bundle/appimage"
        bundle.mkdir(parents=True)
        for name in ("one.AppImage", "two.AppImage"):
            (bundle / name).write_bytes(b"application")
        with self.assertRaisesRegex(ReleaseError, "exactly one"):
            collect("linux-x64", VERSION, bundle.parent, self.base / "unused", self.base / "out")

    def test_windows_collect_uses_installed_executable_and_all_runtimes(self):
        acceptance, staged, resources, application = self.payload("windows-x64")
        (resources / "uninstall.exe").write_bytes(b"do not ship")
        (resources / "WebView2Loader.dll").write_bytes(b"loader")
        bundle = self.base / "bundle/nsis"
        bundle.mkdir(parents=True)
        (bundle / "Setup.exe").write_bytes(b"signed installer")
        self.record_installer(acceptance, bundle / "Setup.exe")
        paths = collect("windows-x64", VERSION, bundle.parent, staged, self.base / "out", acceptance)
        self.assertEqual(paths[0].read_bytes(), b"signed installer")
        with zipfile.ZipFile(paths[1]) as archive:
            self.assertEqual(archive.read("LizzieYzy-Next-2/lizzieyzy-desktop.exe"), application.read_bytes())
            self.assertEqual(archive.read("LizzieYzy-Next-2/readboard/runtime.dll"), b"runtime")
            self.assertEqual(archive.read("LizzieYzy-Next-2/WebView2Loader.dll"), b"loader")
            self.assertNotIn("LizzieYzy-Next-2/uninstall.exe", archive.namelist())

    def test_collect_rejects_installer_changed_after_acceptance(self):
        acceptance, staged, _, _ = self.payload("windows-x64")
        bundle = self.base / "bundle/nsis"
        bundle.mkdir(parents=True)
        installer = bundle / "Setup.exe"
        installer.write_bytes(b"accepted installer")
        self.record_installer(acceptance, installer)
        installer.write_bytes(b"changed installer")
        with self.assertRaisesRegex(ReleaseError, "changed after"):
            collect("windows-x64", VERSION, bundle.parent, staged, self.base / "out", acceptance)

    def test_collect_rechecks_installed_resource_hashes(self):
        acceptance, staged, resources, _ = self.payload("windows-x64")
        (resources / "readboard/runtime.dll").write_bytes(b"tampered")
        with self.assertRaisesRegex(AcceptanceError, "hash/size"):
            collect("windows-x64", VERSION, self.base / "bundle", staged, self.base / "out", acceptance)

    def test_collect_rejects_acceptance_path_outside_payload(self):
        acceptance, staged, _, _ = self.payload("windows-x64")
        result_path = acceptance / "result.json"
        result = json.loads(result_path.read_text(encoding="utf-8"))
        result["resourceRoot"] = str(staged)
        result_path.write_text(json.dumps(result), encoding="utf-8")
        with self.assertRaisesRegex(ReleaseError, "escapes"):
            collect("windows-x64", VERSION, self.base / "bundle", staged, self.base / "out", acceptance)

    def test_macos_collect_archives_the_accepted_app_with_executable_mode(self):
        acceptance, staged, _, application = self.payload("macos-arm64")
        contents = application.parent.parent
        (contents / "CodeResources").write_bytes(b"stapled notarization ticket")
        (contents / "_CodeSignature").mkdir()
        (contents / "_CodeSignature/CodeResources").write_bytes(b"resource seal")
        bundle = self.base / "bundle/dmg"
        bundle.mkdir(parents=True)
        (bundle / "Input.dmg").write_bytes(b"stapled dmg")
        self.record_installer(acceptance, bundle / "Input.dmg")
        paths = collect("macos-arm64", VERSION, bundle.parent, staged, self.base / "out", acceptance)
        self.assertEqual(paths[0].read_bytes(), b"stapled dmg")
        with tarfile.open(paths[1]) as archive:
            entry = archive.getmember("LizzieYzy Next 2.app/Contents/MacOS/lizzieyzy-desktop")
            self.assertEqual(archive.extractfile(entry).read(), application.read_bytes())
            if os.name != "nt":
                self.assertTrue(entry.mode & 0o111)
            self.assertEqual({Path(item.name).parts[0] for item in archive.getmembers()}, {"LizzieYzy Next 2.app"})
            self.assertEqual(archive.extractfile("LizzieYzy Next 2.app/Contents/CodeResources").read(),
                             b"stapled notarization ticket")
            self.assertEqual(archive.extractfile("LizzieYzy Next 2.app/Contents/_CodeSignature/CodeResources").read(),
                             b"resource seal")

    @unittest.skipIf(os.name == "nt", "Windows symlinks require developer mode")
    def test_archives_reject_escaping_links_and_preserve_internal_mac_links(self):
        acceptance, _, resources, application = self.payload("macos-amd64")
        internal = resources / "model-link"
        internal.symlink_to("weights/default.bin.gz")
        output = self.base / "safe.tar.gz"
        macos_update_archive(application, output)
        with tarfile.open(output) as archive:
            self.assertTrue(archive.getmember("LizzieYzy Next 2.app/Contents/Resources/model-link").issym())
        (resources / "outside-link").symlink_to(acceptance)
        with self.assertRaisesRegex(ReleaseError, "escapes"):
            macos_update_archive(application, self.base / "unsafe.tar.gz")
        windows_app = resources / "lizzieyzy-desktop.exe"
        windows_app.write_bytes(b"app")
        with self.assertRaisesRegex(ReleaseError, "link"):
            windows_portable(resources, windows_app, self.base / "unsafe.zip")

    def test_inventory_requires_every_platform_and_no_unknown_or_stale_files(self):
        assets = self.inventory()
        self.assertEqual(len(asset_inventory(assets, VERSION)), 8)
        missing = next(assets.iterdir())
        missing.unlink()
        with self.assertRaisesRegex(ReleaseError, "missing="):
            asset_inventory(assets, VERSION)
        missing.write_bytes(b"restored")
        (assets / "stale.sig").write_bytes(b"signature")
        with self.assertRaisesRegex(ReleaseError, "unexpected="):
            asset_inventory(assets, VERSION)

    def test_inventory_rejects_updater_larger_than_one_gib_including_deb(self):
        assets = self.inventory()
        oversized = assets / asset_name(VERSION, "linux-x64", ".deb")
        oversized.write_bytes(b"x" * 1025)
        with patch("release_assets.MAX_UPDATE_BYTES", 1024), self.assertRaisesRegex(ReleaseError, "1 GiB"):
            asset_inventory(assets, VERSION)

    def test_missing_key_or_password_skips_signatures_but_writes_checksums(self):
        for env in ({}, {"TAURI_SIGNING_PRIVATE_KEY": "present"},
                    {"TAURI_SIGNING_PRIVATE_KEY_PASSWORD": "present"}):
            with self.subTest(env=list(env)), tempfile.TemporaryDirectory() as folder:
                saved = self.base
                self.base = Path(folder)
                assets = self.inventory()
                self.base = saved
                output = io.StringIO()
                with contextlib.redirect_stdout(output):
                    result = manifest(assets, VERSION, "v" + VERSION, "owner/project", "中文说明", env=env)
                self.assertIsNone(result)
                self.assertIn("跳过更新签名和 latest.json", output.getvalue())
                self.assertFalse((assets / "latest.json").exists())
                self.assertEqual(list(assets.glob("*.sig")), [])
                self.assertEqual(len((assets / "SHA256SUMS").read_text(encoding="utf-8").splitlines()), 8)

    def test_manifest_rejects_bad_metadata_before_writing(self):
        assets = self.inventory()
        for version, tag, repository, date in ((VERSION, "v0.3.0", "owner/repo", None),
                                                (VERSION, "v" + VERSION, "../repo", None),
                                                (VERSION, "v" + VERSION, "owner/repo", "2026-11-12")):
            with self.subTest(tag=tag, repository=repository, date=date), self.assertRaises(ReleaseError):
                manifest(assets, version, tag, repository, "notes", date, {})
        self.assertEqual(len(list(assets.iterdir())), 8)


class SignedManifestTests(unittest.TestCase):
    setUp = ReleaseAssetsTests.setUp
    inventory = ReleaseAssetsTests.inventory

    @classmethod
    def setUpClass(cls):
        cls.keys = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.keys.cleanup)
        cls.password = "temporary-test-password-only"
        cls.key = Path(cls.keys.name) / "updater.key"
        # Real randomly generated encrypted test keys; never written into the repo.
        command = [*tauri_command(), "signer", "generate", "--ci", "--password", cls.password,
                   "--write-keys", str(cls.key)]
        subprocess.run(command, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
        cls.env = dict(os.environ, TAURI_SIGNING_PRIVATE_KEY=cls.key.read_text(encoding="utf-8").strip(),
                       TAURI_SIGNING_PRIVATE_KEY_PASSWORD=cls.password,
                       LIZZIE_UPDATER_PUBKEY=cls.key.with_suffix(".key.pub").read_text(encoding="utf-8").strip())
        cls.env.pop("TAURI_SIGNING_PRIVATE_KEY_PATH", None)

    def test_real_signatures_generate_complete_manifest_and_detect_tampering(self):
        assets = self.inventory()
        result = manifest(assets, VERSION, "v" + VERSION, "owner/project", "发布说明\nSecond line",
                          "2026-11-12T00:01:02Z", self.env)
        self.assertEqual(result, json.loads((assets / "latest.json").read_text(encoding="utf-8")))
        self.assertEqual(result["notes"], "发布说明\nSecond line")
        self.assertEqual(result["pub_date"], "2026-11-12T00:01:02Z")
        self.assertEqual(len(result["platforms"]), 9)
        for target, (platform, suffix) in BUNDLE_TARGETS.items():
            name = asset_name(VERSION, platform, suffix)
            entry = result["platforms"][target]
            self.assertEqual(entry["url"], f"https://github.com/owner/project/releases/download/v{VERSION}/{name}")
            self.assertEqual(entry["sha256"], digest(assets / name))
            self.assertEqual(signature_version(entry["signature"]), VERSION)
            self.assertEqual(entry["signature"], (assets / (name + ".sig")).read_text(encoding="utf-8").strip())
        checksum_entries = (assets / "SHA256SUMS").read_text(encoding="utf-8").splitlines()
        self.assertEqual(len(checksum_entries), 17)
        for line in checksum_entries:
            expected, name = line.split("  ", 1)
            self.assertEqual(digest(assets / name), expected)
        name = asset_name(VERSION, "linux-x64", ".AppImage")
        asset, signature = assets / name, assets / (name + ".sig")
        public_key = self.env["LIZZIE_UPDATER_PUBKEY"]
        with self.assertRaisesRegex(ReleaseError, "verification failed"):
            verify_signature(asset, signature, "0.3.0", public_key)
        asset.write_bytes(b"tampered installer")
        with self.assertRaisesRegex(ReleaseError, "verification failed"):
            verify_signature(asset, signature, VERSION, public_key)

    def test_wrong_password_does_not_create_manifest(self):
        assets = self.inventory()
        with self.assertRaisesRegex(ReleaseError, "signing failed"):
            manifest(assets, VERSION, "v" + VERSION, "owner/repo", "notes",
                     env=dict(self.env, TAURI_SIGNING_PRIVATE_KEY_PASSWORD="incorrect"))
        self.assertFalse((assets / "latest.json").exists())

    def test_changed_public_key_rejected_before_manifest(self):
        asset = self.base / "installer"
        asset.write_bytes(b"payload")
        public_lines = base64.b64decode(self.env["LIZZIE_UPDATER_PUBKEY"]).decode().splitlines()
        key = bytearray(base64.b64decode(public_lines[1]))
        key[-1] ^= 1
        public_lines[1] = base64.b64encode(key).decode()
        wrong = base64.b64encode(("\n".join(public_lines) + "\n").encode()).decode()
        with self.assertRaisesRegex(ReleaseError, "verification failed"):
            sign_asset(asset, VERSION, dict(self.env, LIZZIE_UPDATER_PUBKEY=wrong))

    def test_missing_public_key_still_signs_for_existing_clients(self):
        assets = self.inventory()
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            result = manifest(assets, VERSION, "v" + VERSION, "owner/repo", "notes",
                              env=dict(self.env, LIZZIE_UPDATER_PUBKEY=""))
        self.assertIsNotNone(result)
        self.assertIn("本次构建的程序不会检查更新", output.getvalue())
        self.assertEqual(len(list(assets.glob("*.sig"))), 8)


if __name__ == "__main__":
    unittest.main()
