import copy
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import socket
import tempfile
import threading
import unittest

from acceptance import AcceptanceError
import smoke
from smoke import request, verify_settings


class FirstLaunchSettingsTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.resources = Path(self.temporary.name) / "resources"
        self.config = Path(self.temporary.name) / "fresh config"
        self.model = "kata1-tf3-b11c768-s12002M-d6304M.bin.gz"
        self.settings = {"pondering": True, "engine": {
            "program": str(self.resources / "engines/katago/linux-x64/katago"),
            "model": str(self.resources / "weights/default.bin.gz"),
            "config": str(self.config / "analysis.cfg"),
            "canonical_model_name": self.model,
        }}

    def check(self, settings):
        return verify_settings(settings, self.resources, self.config, "linux-x64", self.model)

    def test_accepts_bundled_profile_and_fresh_writable_config(self):
        self.assertEqual(self.check(self.settings), self.settings["engine"])

    def test_rejects_system_or_java_program_and_external_model(self):
        for field, unexpected in (("program", "/usr/bin/katago"), ("model", "/opt/Lizzieyzy/weights/default.bin.gz"),
                                  ("config", "/opt/Lizzieyzy/analysis.cfg")):
            with self.subTest(field=field):
                settings = copy.deepcopy(self.settings)
                settings["engine"][field] = unexpected
                with self.assertRaisesRegex(AcceptanceError, f"unexpected {field}"):
                    self.check(settings)

    def test_rejects_unrecognized_or_different_model(self):
        self.settings["engine"]["canonical_model_name"] = "default.bin.gz"
        with self.assertRaisesRegex(AcceptanceError, "B11 identity"):
            self.check(self.settings)

    def test_rejects_missing_engine_and_disabled_analysis(self):
        with self.assertRaisesRegex(AcceptanceError, "persist an engine"):
            self.check({})
        self.settings["pondering"] = False
        with self.assertRaisesRegex(AcceptanceError, "enable analysis"):
            self.check(self.settings)


class WebDriverTransportTests(unittest.TestCase):
    def setUp(self):
        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                body = {"value": {"ready": True}}
                status = 200
                if self.path == "/failed":
                    body = {"value": {"error": "session not created", "message": "application failed"}}
                    status = 500
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(json.dumps(body).encode())

            def log_message(self, *_):
                pass

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.worker = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.worker.start()
        self.addCleanup(self.stop)
        self.base = f"http://127.0.0.1:{self.server.server_port}"

    def stop(self):
        self.server.shutdown()
        self.server.server_close()
        self.worker.join(timeout=2)
        self.assertFalse(self.worker.is_alive())

    def test_reads_real_http_ready_response(self):
        self.assertTrue(request("GET", self.base + "/status")["value"]["ready"])

    def test_preserves_driver_failure_detail(self):
        with self.assertRaisesRegex(AcceptanceError, "application failed"):
            request("GET", self.base + "/failed")


if __name__ == "__main__":
    unittest.main()


class PortTests(unittest.TestCase):
    def test_a_listener_keeps_its_port_busy(self):
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            listener.listen()
            self.assertFalse(smoke.port_is_free(listener.getsockname()[1]))

    def test_each_run_gets_ports_nothing_listens_on(self):
        port = smoke.unused_port()
        self.assertTrue(smoke.port_is_free(port))


class SavedPathTests(unittest.TestCase):
    def test_extended_length_windows_paths_match_their_plain_form(self):
        with tempfile.TemporaryDirectory() as directory:
            plain = Path(directory) / "katago.exe"
            self.assertTrue(smoke.same_path(str(plain), plain))
            self.assertTrue(smoke.same_path("\\\\?\\" + str(plain), plain))
            self.assertFalse(smoke.same_path(str(plain.with_name("other.exe")), plain))
