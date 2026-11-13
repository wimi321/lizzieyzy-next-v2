"""Exercise signing decisions with disposable payloads and injected native tools."""
import base64
import contextlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import native_signing as signing


class NativeSigningTests(unittest.TestCase):
    def setUp(self):
        self.output = contextlib.redirect_stdout(io.StringIO())
        self.output.__enter__()
        self.addCleanup(self.output.__exit__, None, None, None)
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.addCleanup(self.temporary.cleanup)
        self.directory = self.root / "signing"
        self.config = self.directory / "tauri.conf.json"

    def state(self, platform="macos-arm64", **values):
        self.directory.mkdir()
        state = {"schema": "lizzie-native-signing-v1", "platform": platform, **values}
        signing.save_state(self.directory, state)
        return state

    def mac_env(self):
        return {"APPLE_CERTIFICATE": base64.b64encode(b"test p12").decode(),
                "APPLE_CERTIFICATE_PASSWORD": "test certificate password",
                "APPLE_SIGNING_IDENTITY": "Developer ID Application: Test (ABCDE12345)",
                "APPLE_ID": "test@example.invalid", "APPLE_PASSWORD": "test app password",
                "APPLE_TEAM_ID": "ABCDE12345"}

    def test_no_secrets_produces_preview_overrides_without_invoking_native_tools(self):
        for platform in ("macos-arm64", "macos-amd64", "windows-x64", "linux-x64"):
            with self.subTest(platform=platform), patch.object(signing, "run") as run:
                directory = self.root / platform
                config = directory / "config.json"
                with contextlib.redirect_stdout(io.StringIO()) as output:
                    state = signing.setup(platform, directory, config, {})
                self.assertEqual(state["platform"], platform)
                if platform.startswith("macos-"):
                    self.assertEqual(json.loads(config.read_text())["bundle"]["macOS"]["signingIdentity"], "-")
                    self.assertIn("签名已跳过", output.getvalue())
                    self.assertIn("公证已跳过", output.getvalue())
                else:
                    self.assertEqual(json.loads(config.read_text()), {})
                run.assert_not_called()
                signing.cleanup(directory)
                self.assertFalse(directory.exists())

    def test_partial_certificate_group_skips_import(self):
        for missing in signing.MAC_CERTIFICATE:
            with self.subTest(missing=missing), patch.object(signing, "run") as run:
                env = self.mac_env()
                env[missing] = ""
                directory = self.root / missing
                signing.setup("macos-arm64", directory, directory / "config.json", env)
                self.assertNotIn("identity", signing.load_state(directory))
                run.assert_not_called()

    def test_macos_import_is_scoped_and_credentials_never_enter_state_or_config(self):
        commands = []
        env = self.mac_env()

        def native(command, **kwargs):
            commands.append(command)
            if command[1:4] == ["list-keychains", "-d", "user"] and "-s" not in command:
                return '"/Users/test/Library/Keychains/login.keychain-db"\n'
            if command[1] == "find-identity":
                return f'  1) ABC "{env["APPLE_SIGNING_IDENTITY"]}"\n'
            if command[1] == "create-keychain":
                Path(command[-1]).write_bytes(b"temporary keychain")
            return ""

        with patch.object(signing, "run", side_effect=native):
            state = signing.setup("macos-arm64", self.directory, self.config, env)
            self.assertFalse((self.directory / "certificate.p12").exists())
            config = json.loads(self.config.read_text())
            self.assertTrue(config["bundle"]["macOS"]["hardenedRuntime"])
            self.assertEqual(config["bundle"]["macOS"]["signingIdentity"], env["APPLE_SIGNING_IDENTITY"])
            serialized = json.dumps(state) + self.config.read_text()
            for key in ("APPLE_CERTIFICATE", "APPLE_CERTIFICATE_PASSWORD", "APPLE_PASSWORD"):
                self.assertNotIn(env[key], serialized)
            signing.cleanup(self.directory)
        self.assertFalse(self.directory.exists())
        self.assertIn(["security", "list-keychains", "-d", "user", "-s",
                       "/Users/test/Library/Keychains/login.keychain-db"], commands)
        self.assertTrue(any(command[1] == "delete-keychain" for command in commands))

    def test_failed_mac_import_removes_p12_and_leaves_cleanup_receipt(self):
        def native(command, **kwargs):
            if command[1] == "list-keychains":
                return '"/tmp/login.keychain-db"'
            if command[1] == "import":
                raise signing.SigningError("injected import failure")
            return ""

        with patch.object(signing, "run", side_effect=native):
            with self.assertRaisesRegex(signing.SigningError, "injected"):
                signing.setup("macos-arm64", self.directory, self.config, self.mac_env())
            self.assertFalse((self.directory / "certificate.p12").exists())
            self.assertIn("keychain", signing.load_state(self.directory))
            signing.cleanup(self.directory)

    def test_build_removes_empty_and_partial_credentials_instead_of_exporting_empty_values(self):
        state = self.state(identity="Developer ID Application: Test")
        for missing in signing.NOTARY_CREDENTIALS:
            env = self.mac_env()
            env[missing] = ""
            env.update(APPLE_API_KEY="unused", APPLE_API_ISSUER="unused", APPLE_API_KEY_PATH="unused")
            result = signing.build_environment(state, env)
            for key in (*signing.MAC_CERTIFICATE, *signing.NOTARY_CREDENTIALS,
                        "APPLE_API_KEY", "APPLE_API_ISSUER", "APPLE_API_KEY_PATH"):
                self.assertNotIn(key, result)
        result = signing.build_environment(state, self.mac_env())
        for key in signing.NOTARY_CREDENTIALS:
            self.assertEqual(result[key], self.mac_env()[key])
        for key in signing.MAC_CERTIFICATE:
            self.assertNotIn(key, result)
        state.pop("identity")
        result = signing.build_environment(state, self.mac_env())
        for key in signing.NOTARY_CREDENTIALS:
            self.assertNotIn(key, result)

    def test_build_executes_argument_vector_and_propagates_failure(self):
        self.state(platform="linux-x64")
        with patch.object(signing.subprocess, "run", return_value=subprocess.CompletedProcess([], 0)) as run:
            signing.build(self.directory, ["--", "tool-not-installed", "argument with spaces"])
        self.assertEqual(run.call_args.args[0], ["tool-not-installed", "argument with spaces"])
        with patch.object(signing.subprocess, "run", return_value=subprocess.CompletedProcess([], 7)):
            with self.assertRaisesRegex(signing.SigningError, "exit 7"):
                signing.build(self.directory, ["tool-not-installed"])

    def test_no_secret_setup_and_real_child_build_keep_arguments_and_remove_empty_credentials(self):
        signing.setup("macos-arm64", self.directory, self.config, {})
        output = self.root / "child result.json"
        command = [sys.executable, "-c", (
            "import json,os,pathlib,sys; "
            "pathlib.Path(sys.argv[1]).write_text(json.dumps({"
            "'argument':sys.argv[2], 'apple_id':os.environ.get('APPLE_ID'),"
            "'certificate':os.environ.get('APPLE_CERTIFICATE')}))"
        ), str(output), "argument with spaces"]
        with patch.dict(os.environ, {"APPLE_ID": "", "APPLE_PASSWORD": "", "APPLE_TEAM_ID": "",
                                     "APPLE_CERTIFICATE": "", "APPLE_CERTIFICATE_PASSWORD": ""}):
            signing.build(self.directory, ["--", *command])
        self.assertEqual(json.loads(output.read_text()), {
            "argument": "argument with spaces", "apple_id": None, "certificate": None})
        signing.cleanup(self.directory)

    def test_windows_import_emits_tauri_config_and_cleanup_removes_only_recorded_thumbprint(self):
        thumbprint = "AB" * 20
        env = {"WINDOWS_CERTIFICATE": base64.b64encode(b"test pfx").decode(),
               "WINDOWS_CERTIFICATE_PASSWORD": "test password"}

        def native(command, **kwargs):
            if "LIZZIE_CERTIFICATE_STATE" in kwargs.get("env", {}):
                Path(kwargs["env"]["LIZZIE_CERTIFICATE_STATE"]).write_text(thumbprint)
            return ""

        with patch.object(signing, "run", side_effect=native) as run:
            state = signing.setup("windows-x64", self.directory, self.config, env)
            self.assertEqual(state["thumbprint"], thumbprint)
            self.assertFalse((self.directory / "certificate.p12").exists())
            config = json.loads(self.config.read_text())["bundle"]["windows"]
            self.assertEqual(config["certificateThumbprint"], thumbprint)
            self.assertEqual(config["digestAlgorithm"], "sha256")
            self.assertTrue(config["tsp"])
            signing.cleanup(self.directory)
            self.assertEqual(run.call_args.kwargs["env"]["LIZZIE_CERTIFICATE_THUMBPRINT"], thumbprint)
        self.assertFalse(self.directory.exists())

    def test_windows_resources_include_nested_exe_and_dll_and_verify_every_signature(self):
        (self.root / "readboard/runtime").mkdir(parents=True)
        files = [self.root / "katago.exe", self.root / "readboard/readboard.exe",
                 self.root / "readboard/runtime/coreclr.dll"]
        for path in files:
            path.write_bytes(b"PE test payload")
        (self.root / "readboard/runtime/keep.json").write_text("{}")
        with patch.object(signing, "signtool", return_value="signtool.exe"), \
                patch.object(signing, "run") as run:
            signing.sign_windows_resources(self.root, {"thumbprint": "AB" * 20})
        commands = [call.args[0] for call in run.call_args_list]
        self.assertEqual(len(commands), 6)
        self.assertEqual({Path(command[-1]) for command in commands}, set(files))
        for index in range(0, len(commands), 2):
            self.assertEqual(commands[index][1], "sign")
            self.assertIn("/td", commands[index])
            self.assertEqual(commands[index + 1][1:4], ["verify", "/pa", "/all"])

    def disk_image(self, apps=("LizzieYzy Next 2.app",), status="Accepted", failing=None):
        """Fake hdiutil/codesign/notarytool: attach fills the mount point the
        way a Tauri DMG does (the app, an Applications link, a hidden folder);
        detach empties it again, as unmounting would."""
        commands = []

        def native(command, **kwargs):
            commands.append(command)
            if command[:2] == ["hdiutil", "attach"]:
                mount = Path(command[command.index("-mountpoint") + 1])
                for name in apps:
                    (mount / name / "Contents").mkdir(parents=True)
                (mount / "Applications").symlink_to("/Applications")
                (mount / ".background").mkdir()
            elif command[:2] == ["hdiutil", "detach"]:
                mount = Path(command[-1])
                for path in mount.iterdir():
                    path.unlink() if path.is_symlink() else shutil.rmtree(path)
            if failing and command[:len(failing)] == failing:
                raise signing.SigningError("native tool failed")
            return json.dumps({"status": status}) if "submit" in command else ""

        return commands, native

    def mount_points(self):
        return [path for path in self.directory.iterdir() if path.name.startswith("dmg-")]

    def test_notarize_checks_the_stapled_app_inside_the_dmg_before_submitting(self):
        self.state(identity="Developer ID Application: Test")
        dmg = self.root / "App.dmg"
        commands, native = self.disk_image()
        with patch.object(signing, "run", side_effect=native):
            self.assertTrue(signing.notarize(self.directory, dmg, self.mac_env()))
        attach = commands[0]
        self.assertEqual(attach[:5], ["hdiutil", "attach", "-nobrowse", "-readonly", "-noautoopen"])
        self.assertEqual(attach[-1], str(dmg))
        app = str(Path(attach[attach.index("-mountpoint") + 1]) / "LizzieYzy Next 2.app")
        self.assertEqual(commands[1:4], [["codesign", "--verify", "--deep", "--strict", app],
                                         ["xcrun", "stapler", "validate", app],
                                         ["hdiutil", "detach", attach[attach.index("-mountpoint") + 1]]])
        submit = next(i for i, command in enumerate(commands) if "submit" in command)
        self.assertEqual(commands[submit - 1], ["codesign", "--verify", "--strict", str(dmg)])
        self.assertEqual(commands[-2:], [["xcrun", "stapler", "staple", str(dmg)],
                                         ["xcrun", "stapler", "validate", str(dmg)]])
        self.assertEqual(self.mount_points(), [])

    def test_notarize_refuses_a_dmg_without_exactly_one_app_and_still_detaches(self):
        self.state(identity="Developer ID Application: Test")
        for apps in ((), ("A.app", "B.app")):
            with self.subTest(apps=apps):
                commands, native = self.disk_image(apps=apps)
                with patch.object(signing, "run", side_effect=native):
                    with self.assertRaisesRegex(signing.SigningError, "exactly one app"):
                        signing.notarize(self.directory, self.root / "App.dmg", self.mac_env())
                self.assertEqual(commands[-1][:2], ["hdiutil", "detach"])
                self.assertFalse(any("submit" in command for command in commands))
                self.assertEqual(self.mount_points(), [])

    def test_unstapled_app_detaches_and_never_submits(self):
        self.state(identity="Developer ID Application: Test")
        commands, native = self.disk_image(failing=["xcrun", "stapler", "validate"])
        with patch.object(signing, "run", side_effect=native):
            with self.assertRaisesRegex(signing.SigningError, "native tool failed"):
                signing.notarize(self.directory, self.root / "App.dmg", self.mac_env())
        self.assertEqual(commands[-1][:2], ["hdiutil", "detach"])
        self.assertFalse(any("submit" in command for command in commands))
        self.assertEqual(self.mount_points(), [])

    def test_stuck_volume_is_force_detached(self):
        self.state(identity="Developer ID Application: Test")
        commands, native = self.disk_image()

        def busy(command, **kwargs):
            if command[:2] == ["hdiutil", "detach"] and "-force" not in command:
                commands.append(command)
                raise signing.SigningError("resource busy")
            return native(command, **kwargs)

        with patch.object(signing, "run", side_effect=busy):
            self.assertTrue(signing.notarize(self.directory, self.root / "App.dmg", self.mac_env()))
        detaches = [command for command in commands if command[:2] == ["hdiutil", "detach"]]
        self.assertEqual([command[2] for command in detaches], [detaches[0][2], "-force"])
        self.assertEqual(self.mount_points(), [])

    def test_rejected_notarization_never_staples_or_claims_success(self):
        self.state(identity="Developer ID Application: Test")
        commands, native = self.disk_image(status="Invalid")
        with patch.object(signing, "run", side_effect=native):
            with self.assertRaisesRegex(signing.SigningError, "did not accept"):
                signing.notarize(self.directory, self.root / "App.dmg", self.mac_env())
        self.assertFalse(any("staple" in command for command in commands))

    def test_missing_notarization_secrets_never_contact_apple(self):
        self.state(identity="Developer ID Application: Test")
        for missing in signing.NOTARY_CREDENTIALS:
            env = self.mac_env()
            env.pop(missing)
            with patch.object(signing, "run") as run:
                self.assertFalse(signing.notarize(self.directory, self.root / "dmg", env))
                run.assert_not_called()

    def test_command_failure_does_not_expose_credentials(self):
        failure = subprocess.CompletedProcess([], 1, "secret stdout", "secret stderr")
        with patch.object(signing.subprocess, "run", return_value=failure):
            with self.assertRaises(signing.SigningError) as caught:
                signing.run(["security", "import", "secret argument"])
        self.assertNotIn("secret", str(caught.exception))

    def test_cleanup_refuses_unmanaged_directory_and_escaping_keychain(self):
        self.directory.mkdir()
        keep = self.directory / "keep"
        keep.write_text("keep")
        with self.assertRaises(signing.SigningError):
            signing.cleanup(self.directory)
        self.assertTrue(keep.exists())
        signing.save_state(self.directory, {"schema": "lizzie-native-signing-v1", "keychain": "/tmp/not-ours"})
        with patch.object(signing, "run") as run, self.assertRaises(signing.SigningError):
            signing.cleanup(self.directory)
        run.assert_not_called()
        self.assertTrue(keep.exists())


if __name__ == "__main__":
    unittest.main()
