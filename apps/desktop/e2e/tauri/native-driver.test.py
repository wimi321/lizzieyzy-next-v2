"""Run against two rustc-built executables, never inspect implementation source."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

WRAPPER = Path(sys.argv.pop(1)).resolve()
FIXTURE = Path(sys.argv.pop(1)).resolve()


class NativeDriverWrapperTest(unittest.TestCase):
    def test_preserves_arguments_environment_streams_and_exit_code(self):
        with tempfile.TemporaryDirectory(prefix="lizzie driver test ") as temporary:
            directory = Path(temporary)
            executable = directory / ("native driver" + FIXTURE.suffix)
            shutil.copy2(FIXTURE, executable)
            diagnostics = directory / "diagnostic output"
            result = subprocess.run(
                [str(WRAPPER), "--port=4445", "--host=127.0.0.1",
                 '--path=C:\\folder with spaces\\a "quoted" file.sgf'],
                env={**os.environ,
                     "TAURI_E2E_NATIVE_DRIVER": str(executable),
                     "TAURI_E2E_DIAGNOSTICS": str(diagnostics),
                     "TAURI_WEBVIEW_AUTOMATION": "true",
                     "TAURI_AUTOMATION": "true"},
                capture_output=True, text=True, timeout=5, check=False,
            )
            self.assertEqual(result.returncode, 23, result.stderr)
            self.assertEqual(result.stdout, "")
            self.assertIn("Native WebDriver started: pid=", result.stderr)
            stdout = (diagnostics / "native-stdout.log").read_text()
            stderr = (diagnostics / "native-stderr.log").read_text()
            self.assertIn("arguments and environment verified", stdout)
            self.assertIn("inherited application stdout", stdout)
            self.assertIn("native stderr", stderr)
            self.assertIn("inherited application stderr", stderr)
            self.assertEqual((diagnostics / "msedgedriver.log").read_text(),
                             "verbose native driver log\n")
            self.assertTrue((diagnostics / "webview2.log").is_file())

    def test_missing_driver_is_an_explicit_failure(self):
        environment = dict(os.environ)
        environment.pop("TAURI_E2E_NATIVE_DRIVER", None)
        result = subprocess.run([str(WRAPPER)], env=environment, capture_output=True,
                                text=True, timeout=5, check=False)
        self.assertEqual(result.returncode, 1)
        self.assertIn("TAURI_E2E_NATIVE_DRIVER is required", result.stderr)


if __name__ == "__main__":
    unittest.main()
