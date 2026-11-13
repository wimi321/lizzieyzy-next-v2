"""Exercise release orchestration with temporary files and a fake GitHub transport."""
import contextlib
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

from release_workflow import ReleaseError, configure, draft, metadata


class ReleaseWorkflowTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.config = self.root / "tauri.conf.json"
        self.config.write_text('{"version":"0.1.0"}', encoding="utf-8")
        self.sha = "a" * 40
        self.assets = self.root / "assets"
        self.assets.mkdir()
        (self.assets / "SHA256SUMS").write_text("checksum", encoding="utf-8")
        (self.assets / "app.exe").write_bytes(b"installer")
        self.notes = self.root / "notes.md"
        self.notes.write_text("Release notes\nsecond line", encoding="utf-8")

    def transport(self, replies):
        self.calls = []

        def run(args):
            self.calls.append(args)
            if not replies:
                self.fail("Unexpected GitHub operation")
            code, output, error = replies.pop(0)
            return subprocess.CompletedProcess(args, code, output, error)

        return run

    def publish(self, replies):
        draft("owner/repo", "v1.2.3-preview.1", self.sha,
              self.assets, self.notes, self.transport(replies))

    def test_tag_version_overrides_source_version_without_modifying_source(self):
        self.assertEqual(metadata({"GITHUB_REF_TYPE": "tag", "GITHUB_REF_NAME": "v2.0.0"}, self.config),
                         ("v2.0.0", "2.0.0"))
        self.assertEqual(json.loads(self.config.read_text())["version"], "0.1.0")

    def test_manual_branch_without_tag_gets_unique_prerelease(self):
        self.assertEqual(metadata({"GITHUB_RUN_ID": "42", "GITHUB_RUN_ATTEMPT": "2"}, self.config),
                         ("v0.1.0-preview.42.2", "0.1.0-preview.42.2"))

    def test_explicit_manual_tag_and_push_tag_precedence(self):
        self.assertEqual(metadata({"RELEASE_TAG": "v1.2.3"}, self.config), ("v1.2.3", "1.2.3"))
        self.assertEqual(metadata({"RELEASE_TAG": "v9.9.9", "GITHUB_REF_TYPE": "tag",
                                   "GITHUB_REF_NAME": "v1.2.3"}, self.config), ("v1.2.3", "1.2.3"))

    def test_invalid_input_cannot_inject_workflow_output(self):
        for value in ("v1.2.3\nversion=bad", "../tag", "v1.2", "v1.2.3-beta.01"):
            with self.subTest(value=value), self.assertRaises(ReleaseError):
                metadata({"RELEASE_TAG": value}, self.config)
        with self.assertRaises(ReleaseError):
            metadata({"GITHUB_RUN_ID": "0", "GITHUB_RUN_ATTEMPT": "1"}, self.config)

    def test_missing_public_key_keeps_build_configuration_and_explains_no_updates(self):
        output = self.root / "build/config.json"
        logs = io.StringIO()
        with contextlib.redirect_stdout(logs):
            configure("1.2.3", "  ", output)
        self.assertIn("本次构建的程序不会检查更新", logs.getvalue())
        self.assertEqual(json.loads(output.read_text()),
                         {"version": "1.2.3", "bundle": {"createUpdaterArtifacts": False}})

    def test_public_key_is_not_written_to_config_or_logs(self):
        output = self.root / "config.json"
        logs = io.StringIO()
        with contextlib.redirect_stdout(logs):
            configure("1.2.3", "test-public-key", output)
        self.assertNotIn("test-public-key", output.read_text() + logs.getvalue())

    def test_create_is_always_a_draft_at_exact_commit_with_notes_file(self):
        self.publish([(1, "", "HTTP 404"), (1, "", "release not found"), (0, "", "")])
        create = self.calls[-1]
        self.assertEqual(create[:3], ["release", "create", "v1.2.3-preview.1"])
        self.assertIn("--draft", create)
        self.assertIn("--prerelease", create)
        self.assertEqual(create[create.index("--target") + 1], self.sha)
        self.assertEqual(create[create.index("--notes-file") + 1], str(self.notes))

    def test_refuses_existing_tag_at_another_commit_before_mutation(self):
        with self.assertRaisesRegex(ReleaseError, "different commit"):
            self.publish([(0, "commit\t" + "b" * 40, "")])
        self.assertEqual(len(self.calls), 1)

    def test_refuses_published_release_before_mutation(self):
        with self.assertRaisesRegex(ReleaseError, "published release"):
            self.publish([(0, "commit\t" + self.sha, ""), (0, json.dumps({"isDraft": False}), "")])
        self.assertEqual(len(self.calls), 2)

    def test_rerun_updates_only_same_draft_and_asset_set(self):
        existing = {"isDraft": True, "targetCommitish": self.sha, "assets": [{"name": "app.exe"}]}
        self.publish([(0, "commit\t" + self.sha, ""), (0, json.dumps(existing), ""), (0, "", ""), (0, "", "")])
        self.assertEqual(self.calls[-2][:2], ["release", "edit"])
        self.assertEqual(self.calls[-1][:2], ["release", "upload"])
        self.assertIn("--clobber", self.calls[-1])
        self.assertNotIn("--draft=false", self.calls[-2])

    def test_rerun_cannot_leave_old_manifest_or_replace_another_commit(self):
        for existing, message in (
            ({"isDraft": True, "targetCommitish": "main", "assets": []}, "exact build commit"),
            ({"isDraft": True, "targetCommitish": self.sha, "assets": [{"name": "latest.json"}]}, "stale assets"),
        ):
            with self.subTest(existing=existing), self.assertRaisesRegex(ReleaseError, message):
                self.publish([(0, "commit\t" + self.sha, ""), (0, json.dumps(existing), "")])
            self.assertEqual(len(self.calls), 2)

    def test_api_failures_do_not_turn_into_create_attempts(self):
        with self.assertRaisesRegex(ReleaseError, "HTTP 403"):
            self.publish([(1, "", "HTTP 403")])
        self.assertEqual(len(self.calls), 1)
        with self.assertRaisesRegex(ReleaseError, "HTTP 500"):
            self.publish([(0, "commit\t" + self.sha, ""), (1, "", "HTTP 500")])
        self.assertEqual(len(self.calls), 2)

    def test_annotated_tag_is_peeled_before_draft_creation(self):
        self.publish([(0, "tag\t" + "b" * 40, ""), (0, self.sha, ""),
                      (1, "", "release not found"), (0, "", "")])
        self.assertIn("repos/owner/repo/commits/v1.2.3-preview.1", self.calls[1])
        self.assertIn("--draft", self.calls[-1])


if __name__ == "__main__":
    unittest.main()
