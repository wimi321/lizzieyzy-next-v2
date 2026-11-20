"""Execute the offline installer with fake system commands, without using apt or sudo."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import textwrap
import unittest


INSTALLER = Path(__file__).with_name("install-linux-runtime.sh").resolve()
BASH = shutil.which("bash")


class InstallLinuxRuntimeTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="lizzie runtime test ")
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name).resolve()
        self.bin = self.directory / "stub bin"
        self.bin.mkdir()
        self.packages = self.directory / "runtime packages"
        self.packages.mkdir()
        self.arguments = self.directory / "apt arguments"
        self.calls = self.directory / "apt calls"
        self.github_env = self.directory / "github env"
        self.summary = self.directory / "step summary"
        self.write_stub("sudo", r"""
            if [ "$#" -eq 0 ] || [ "$1" != apt-get ]; then
              echo 'Unexpected sudo command in offline installer test' >&2
              exit 99
            fi
            shift
            exec "$RUNTIME_TEST_BIN/apt-get" "$@"
        """)
        self.write_stub("apt-get", r"""
            printf 'called\n' >> "$RUNTIME_TEST_CALLS"
            printf '%s\0' "$@" >> "$RUNTIME_TEST_ARGUMENTS"
            if [ "$RUNTIME_TEST_APT_STATUS" -ne 0 ]; then
              echo 'fake apt: unsatisfied dependency' >&2
              exit "$RUNTIME_TEST_APT_STATUS"
            fi
            if [ "$RUNTIME_TEST_WEBKIT" = 1 ]; then
              : > "$RUNTIME_TEST_BIN/WebKitWebDriver"
            fi
            if [ "$RUNTIME_TEST_XVFB" = 1 ]; then
              : > "$RUNTIME_TEST_BIN/xvfb-run"
            fi
        """)
        self.write_stub("date", r"""
            if [ "$#" -ne 1 ] || [ "$1" != +%s ]; then
              echo 'Unexpected date command in offline installer test' >&2
              exit 99
            fi
            if [ -f "$RUNTIME_TEST_CLOCK" ]; then
              printf '1042\n'
            else
              : > "$RUNTIME_TEST_CLOCK"
              printf '1000\n'
            fi
        """)
        bash_environment = self.directory / "bash environment"
        bash_environment.write_text(textwrap.dedent(r"""
            # Bind sudo directly so no host executable can gain privileges.
            sudo() { "$RUNTIME_TEST_BIN/sudo" "$@"; }
            date() { "$RUNTIME_TEST_BIN/date" "$@"; }
            command() {
              if [ "$#" -eq 2 ] && [ "$1" = -v ]; then
                case "$2" in
                  WebKitWebDriver|xvfb-run)
                    if [ -f "$RUNTIME_TEST_BIN/$2" ]; then
                      printf '%s\n' "$RUNTIME_TEST_BIN/$2"
                      return 0
                    fi
                    return 1
                    ;;
                esac
              fi
              builtin command "$@"
            }
        """))
        self.environment = {
            **os.environ,
            "PATH": os.pathsep.join((str(Path(os.environ["HOME"]) / ".cargo/bin"),
                                     str(self.bin), os.environ.get("PATH", os.defpath))),
            "BASH_ENV": str(bash_environment),
            "GITHUB_ENV": str(self.github_env),
            "GITHUB_STEP_SUMMARY": str(self.summary),
            "RUNTIME_TEST_BIN": str(self.bin),
            "RUNTIME_TEST_ARGUMENTS": str(self.arguments),
            "RUNTIME_TEST_CALLS": str(self.calls),
            "RUNTIME_TEST_CLOCK": str(self.directory / "clock"),
            "RUNTIME_TEST_APT_STATUS": "0",
            "RUNTIME_TEST_WEBKIT": "1",
            "RUNTIME_TEST_XVFB": "1",
        }

    def write_stub(self, name, body):
        path = self.bin / name
        path.write_text("#!/bin/sh\nset -eu\n" + textwrap.dedent(body))
        path.chmod(0o755)

    def add_package(self, name="runtime.deb"):
        path = self.packages / name
        path.write_bytes(b"fake package passed only to fake apt")
        return str(path.resolve())

    def run_installer(self, *arguments):
        self.assertIsNotNone(BASH, "bash is required to execute the installer")
        result = subprocess.run(
            [BASH, str(INSTALLER), *map(str, arguments)],
            env=self.environment, capture_output=True, text=True, timeout=5, check=False,
        )
        timing = ("Linux E2E offline runtime installation: "
                  f"42 seconds (exit {result.returncode}).\n")
        self.assertEqual(self.github_env.read_text(), "TAURI_E2E_APT_SECONDS=42\n")
        self.assertEqual(self.summary.read_text(), timing)
        self.assertIn(timing, result.stdout)
        return result

    def apt_arguments(self):
        self.assertEqual(self.calls.read_text(), "called\n", "apt must run exactly once")
        return self.arguments.read_bytes().decode().rstrip("\0").split("\0")

    def test_missing_argument_fails_before_apt(self):
        result = self.run_installer()
        self.assertEqual(result.returncode, 1)
        self.assertIn("Linux runtime .deb directory is missing", result.stderr)
        self.assertFalse(self.calls.exists())

    def test_missing_directory_fails_before_apt(self):
        result = self.run_installer(self.directory / "missing packages")
        self.assertEqual(result.returncode, 1)
        self.assertIn("tauri-e2e-Linux artifact", result.stderr)
        self.assertFalse(self.calls.exists())

    def test_empty_directory_fails_before_apt(self):
        result = self.run_installer(self.packages)
        self.assertEqual(result.returncode, 1)
        self.assertIn("No .deb packages", result.stderr)
        self.assertIn("Verify must supply", result.stderr)
        self.assertFalse(self.calls.exists())

    def test_success_preserves_paths_and_passes_only_debs_without_network(self):
        first = self.add_package("a runtime.deb")
        second = self.add_package("b dependency.deb")
        (self.packages / "notes.txt").write_text("not an installable package")
        result = self.run_installer(self.packages)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.apt_arguments(), [
            "install", "--no-install-recommends", "--no-download", "-y",
            "-o", "Dir::Etc::sourcelist=/dev/null", "-o", "Dir::Etc::sourceparts=-",
            first, second,
        ])

    def test_apt_failure_preserves_status_and_never_retries_online(self):
        self.add_package()
        self.environment["RUNTIME_TEST_APT_STATUS"] = "100"
        result = self.run_installer(self.packages)
        self.assertEqual(result.returncode, 100)
        self.assertIn("fake apt: unsatisfied dependency", result.stderr)
        self.assertIn("artifact may be incomplete", result.stderr)
        self.assertIn("runner images may differ", result.stderr)
        self.assertIn("Network fallback is disabled", result.stderr)
        self.assertIn("--no-download", self.apt_arguments())

    def test_missing_webkit_driver_after_install_is_explicit_failure(self):
        self.add_package()
        self.environment["RUNTIME_TEST_WEBKIT"] = "0"
        result = self.run_installer(self.packages)
        self.assertEqual(result.returncode, 1)
        self.assertIn("webkit2gtk-driver did not provide WebKitWebDriver", result.stderr)
        self.apt_arguments()

    def test_missing_xvfb_after_install_is_explicit_failure(self):
        self.add_package()
        self.environment["RUNTIME_TEST_XVFB"] = "0"
        result = self.run_installer(self.packages)
        self.assertEqual(result.returncode, 1)
        self.assertIn("xvfb did not provide xvfb-run", result.stderr)
        self.apt_arguments()


if __name__ == "__main__":
    unittest.main()
