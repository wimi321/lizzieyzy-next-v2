"""Behavior checks against generated payloads, never source-text assertions."""
import json
import os
from pathlib import Path
import tempfile
import unittest

from acceptance import AcceptanceError, digest, locate, verify_payload


class PayloadAcceptanceTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.staged = self.base / "staged"
        self.resources = self.base / "payload/usr/lib/lizzieyzy-desktop"
        self.staged.mkdir()
        self.resources.mkdir(parents=True)
        self.files = {
            "engines/katago/linux-x64/katago": b"verified executable",
            "weights/default.bin.gz": b"verified model",
            "licenses/KataGo.txt": b"license text",
        }
        self.receipt = {"schemaVersion": 1, "platform": "linux-x64", "files": {}}
        for name, data in self.files.items():
            path = self.resources / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
            executable = name.endswith("/katago")
            if executable:
                path.chmod(0o755)
            self.receipt["files"][name] = {
                "sha256": digest(path), "sizeBytes": len(data), "executable": executable,
            }
        self.write_receipts()
        self.app = self.base / "payload/usr/bin/lizzieyzy-desktop"
        self.app.parent.mkdir(parents=True)
        self.app.write_bytes(b"production app")

    def write_receipts(self):
        for root in (self.staged, self.resources):
            (root / "bundle-manifest.json").write_text(json.dumps(self.receipt), encoding="utf-8")

    def test_accepts_complete_payload_and_finds_actual_app(self):
        result = verify_payload(self.staged, self.resources, "linux-x64")
        self.assertEqual(result["filesVerified"], 3)
        self.assertEqual(result["resourceBytes"], sum(map(len, self.files.values())))
        self.assertEqual(locate(self.base / "payload", "linux-x64"), (self.resources, self.app))

    def test_an_extracted_appimage_starts_through_apprun(self):
        app_run = self.base / "payload/AppRun"
        app_run.write_text("#!/bin/sh\n")
        self.assertEqual(locate(self.base / "payload", "linux-x64"), (self.resources, app_run))

    def test_rejects_changed_bytes_even_at_same_size(self):
        path = self.resources / "weights/default.bin.gz"
        path.write_bytes(b"X" * path.stat().st_size)
        with self.assertRaisesRegex(AcceptanceError, "hash/size mismatch"):
            verify_payload(self.staged, self.resources, "linux-x64")

    def test_rejects_missing_runtime_or_license(self):
        (self.resources / "licenses/KataGo.txt").unlink()
        with self.assertRaisesRegex(AcceptanceError, "Missing or escaping"):
            verify_payload(self.staged, self.resources, "linux-x64")

    def test_rejects_receipt_substitution(self):
        (self.resources / "bundle-manifest.json").write_text("{}")
        with self.assertRaisesRegex(AcceptanceError, "different bundle receipt"):
            verify_payload(self.staged, self.resources, "linux-x64")

    def test_rejects_receipt_without_model(self):
        del self.receipt["files"]["weights/default.bin.gz"]
        self.write_receipts()
        with self.assertRaisesRegex(AcceptanceError, "omits required payload"):
            verify_payload(self.staged, self.resources, "linux-x64")

    def test_rejects_wrong_platform(self):
        with self.assertRaisesRegex(AcceptanceError, "platform"):
            verify_payload(self.staged, self.resources, "windows-x64")

    def test_rejects_traversal_in_receipt(self):
        self.receipt["files"]["../outside"] = {"sizeBytes": 0, "sha256": "bad"}
        self.write_receipts()
        with self.assertRaisesRegex(AcceptanceError, "Unsafe receipt path"):
            verify_payload(self.staged, self.resources, "linux-x64")

    @unittest.skipIf(os.name == "nt", "Unix executable mode")
    def test_rejects_lost_executable_mode(self):
        (self.resources / "engines/katago/linux-x64/katago").chmod(0o644)
        with self.assertRaisesRegex(AcceptanceError, "lost executable mode"):
            verify_payload(self.staged, self.resources, "linux-x64")

    @unittest.skipIf(os.name == "nt", "Windows symlinks require developer mode")
    def test_rejects_resource_symlink_even_if_content_matches(self):
        path = self.resources / "weights/default.bin.gz"
        outside = self.base / "outside"
        path.rename(outside)
        path.symlink_to(outside)
        with self.assertRaisesRegex(AcceptanceError, "Missing or escaping"):
            verify_payload(self.staged, self.resources, "linux-x64")

    def test_ignores_nested_vendor_receipt_and_malformed_vendor_json(self):
        vendor = self.resources / "engines/katago/linux-x64/bundle-manifest.json"
        vendor.write_text(json.dumps({"schemaVersion": 1, "executable": "katago", "libraries": []}))
        (self.resources / "weights/bundle-manifest.json").write_text("vendor metadata, not JSON")
        self.assertEqual(locate(self.base / "payload", "linux-x64"), (self.resources, self.app))

    def test_rejects_ambiguous_packaged_receipts(self):
        duplicate = self.base / "payload/another/usr/lib/lizzieyzy-desktop"
        duplicate.mkdir(parents=True)
        (duplicate / "bundle-manifest.json").write_text(json.dumps(self.receipt))
        duplicate_app = self.base / "payload/another/usr/bin/lizzieyzy-desktop"
        duplicate_app.parent.mkdir(parents=True)
        duplicate_app.write_bytes(b"another application")
        with self.assertRaisesRegex(AcceptanceError, "Expected one packaged receipt"):
            locate(self.base / "payload", "linux-x64")


if __name__ == "__main__":
    unittest.main()
