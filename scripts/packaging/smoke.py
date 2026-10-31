#!/usr/bin/env python3
"""First-launch acceptance of an extracted/installed production application.

This packaging check runs the real, verified CPU KataGo and B11. It does not
seed settings, invoke auto-setup itself, modify resources, or inject a backend.
Engine/analysis unit tests remain independent fake-katago tests.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request

from acceptance import AcceptanceError, regular_inside, digest


# Packaging infrastructure and real CPU model cold start have separate limits
# from ordinary unit/DOM assertions. Application auto-setup itself is 120s.
STARTUP_SECONDS = 180
POLL = threading.Event()
DOM = """
const byId = id => document.querySelector('[data-testid="' + id + '"]');
const engine = byId('engine-status');
const analysis = byId('analysis-move');
return {
  setup: !!byId('engine-setup'),
  setupText: byId('engine-setup')?.textContent ?? '',
  state: engine?.getAttribute('data-state'),
  engineDetail: engine?.getAttribute('title'),
  visits: Number(analysis?.getAttribute('data-visits') ?? 0),
  board: !!byId('board'),
  analysisNode: analysis?.getAttribute('data-node-id'),
  boardNode: byId('board-area')?.getAttribute('data-node-id')
};
"""


def request(method: str, url: str, body=None, timeout: float = 5):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(url, data=data, method=method, headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as response:
            result = json.load(response)
    except urllib.error.HTTPError as error:
        raise AcceptanceError(f"{method} {url}: {error.read().decode(errors='replace')}") from error
    value = result.get("value")
    if isinstance(value, dict) and value.get("error"):
        raise AcceptanceError(f"WebDriver: {value}")
    return result


def free_port(port: int) -> None:
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", port))


def poll(check, seconds: float, message: str):
    deadline = time.monotonic() + seconds
    while True:
        value = check()
        if value:
            return value
        if time.monotonic() >= deadline:
            raise AcceptanceError(message)
        # Conditional readiness polling, not a fixed sleep to sequence startup.
        POLL.wait(min(0.1, max(0, deadline - time.monotonic())))


def verify_settings(settings: dict, resources: Path, config_dir: Path, platform: str, model_name: str) -> dict:
    profile = settings.get("engine")
    if not isinstance(profile, dict):
        raise AcceptanceError("First launch did not persist an engine profile")
    suffix = ".exe" if platform == "windows-x64" else ""
    expected = {
        "program": resources / "engines" / "katago" / platform / f"katago{suffix}",
        "model": resources / "weights" / "default.bin.gz",
        "config": config_dir / "analysis.cfg",
    }
    for field, path in expected.items():
        if Path(profile.get(field, "")).resolve() != path.resolve():
            raise AcceptanceError(f"Auto-setup chose unexpected {field}: {profile.get(field)}; expected {path}")
    if profile.get("canonical_model_name") != model_name:
        raise AcceptanceError("Auto-setup did not recognize the bundled B11 identity")
    if settings.get("pondering") is not True:
        raise AcceptanceError("First launch did not enable analysis")
    return profile


def check_receipt(resources: Path, platform: str) -> None:
    receipt = json.loads((resources / "bundle-manifest.json").read_text(encoding="utf-8"))
    if receipt.get("platform") != platform:
        raise AcceptanceError("Acceptance receipt platform mismatch")
    for relative, expected in receipt["files"].items():
        path = regular_inside(resources, relative)
        if path.stat().st_size != expected["sizeBytes"] or digest(path) != expected["sha256"]:
            raise AcceptanceError(f"Resources changed since installer acceptance: {relative}")


def terminate(child: subprocess.Popen | None) -> None:
    if child is None:
        return
    if os.name == "nt":
        if child.poll() is not None:
            return
        subprocess.run(["taskkill", "/PID", str(child.pid), "/T", "/F"], check=False, timeout=5,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    else:
        try:
            os.killpg(child.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
    try:
        child.wait(timeout=5)
    except subprocess.TimeoutExpired:
        if os.name != "nt":
            os.killpg(child.pid, signal.SIGKILL)
        else:
            child.kill()
        child.wait(timeout=5)


def run(args: argparse.Namespace) -> dict:
    started = time.monotonic()
    accepted = json.loads((args.acceptance_dir / "result.json").read_text(encoding="utf-8"))
    platform = accepted["platform"]
    if platform not in {"windows-x64", "linux-x64"}:
        raise AcceptanceError("Automated first-launch acceptance is Windows/Linux only")
    resources = Path(accepted["resourceRoot"])
    application = Path(accepted["application"])
    check_receipt(resources, platform)
    catalog = json.loads((Path(__file__).resolve().parents[2] / "resources/katago/katago-assets.json").read_text())
    model_name = catalog["models"][catalog["defaultModelId"]]["fileName"]
    diagnostics = args.acceptance_dir.resolve() / "smoke"
    diagnostics.mkdir(exist_ok=False)
    config_dir = diagnostics / "fresh config"
    config_dir.mkdir()
    settings_file = config_dir / "settings.json"
    if settings_file.exists():
        raise AcceptanceError("Smoke config directory is not clean")
    free_port(4444)
    free_port(4445)
    windows = platform == "windows-x64"
    if windows:
        free_port(9222)
    environment = dict(os.environ)
    # Keep system dependencies while removing engine discovery through build,
    # Java or Homebrew PATH entries. Exact saved paths are checked below too.
    system_path = str(Path(environment["SystemRoot"]) / "System32") if windows else "/usr/bin:/bin"
    environment["PATH"] = str(Path.home() / ".cargo" / "bin") + os.pathsep + system_path
    driver = None
    app = None
    session = None
    base = "http://127.0.0.1:4444"
    report = {"platform": platform, "application": str(application), "realEngine": True}
    with (diagnostics / "process.log").open("w", encoding="utf-8") as log:
        try:
            driver_args = [str(args.driver.resolve())]
            driver_args.extend(["--port=4444"] if windows else ["--port", "4444", "--native-port", "4445"])
            driver = subprocess.Popen(driver_args, stdout=log, stderr=log, env=environment,
                                      start_new_session=not windows)

            def ready():
                if driver.poll() is not None:
                    raise AcceptanceError("Native WebDriver exited before becoming ready")
                try:
                    return request("GET", base + "/status", timeout=1).get("value", {}).get("ready")
                except (urllib.error.URLError, TimeoutError):
                    return False

            poll(ready, 30, "Native WebDriver startup timed out")
            app_args = [f"--config-dir={config_dir}"]
            if windows:
                # Debug policy belongs to smoke-windows.ps1; production config
                # contains no debug port and no special IPC permissions.
                app = subprocess.Popen([str(application), *app_args], cwd=application.parent,
                                       stdout=log, stderr=log, env=environment)

                def cdp_ready():
                    if app.poll() is not None:
                        raise AcceptanceError("Application exited before WebView2 started")
                    try:
                        version = request("GET", "http://127.0.0.1:9222/json/version", timeout=1)
                        return str(version.get("webSocketDebuggerUrl", "")).startswith("ws://127.0.0.1:9222/")
                    except (urllib.error.URLError, TimeoutError):
                        return False

                poll(cdp_ready, 60, "WebView2 cold start timed out")
                caps = {"browserName": "webview2", "ms:edgeOptions": {"debuggerAddress": "127.0.0.1:9222"}}
            else:
                caps = {"tauri:options": {"application": str(application), "args": app_args}}
            caps["timeouts"] = {"implicit": 0, "pageLoad": 60000, "script": 5000}
            response = request("POST", base + "/session", {"capabilities": {"alwaysMatch": caps}}, timeout=60)
            session = response["value"]["sessionId"]
            endpoint = f"{base}/session/{session}"
            latest = {}

            def configured_and_analyzing():
                nonlocal latest
                latest = request("POST", endpoint + "/execute/sync", {"script": DOM, "args": []})["value"]
                if latest.get("setup"):
                    raise AcceptanceError(f"First launch opened engine setup: {latest.get('setupText')}")
                if latest.get("state") == "Failed":
                    raise AcceptanceError(f"Bundled engine failed: {latest.get('engineDetail')}")
                return settings_file.is_file() and latest.get("state") == "Ready" and latest.get("visits", 0) > 0

            poll(configured_and_analyzing, STARTUP_SECONDS, "Real B11 first-launch analysis timed out")
            settings = json.loads(settings_file.read_text(encoding="utf-8"))
            profile = verify_settings(settings, resources, config_dir, platform, model_name)
            if not latest.get("board") or latest.get("analysisNode") != latest.get("boardNode"):
                raise AcceptanceError("Analysis did not reach the displayed board")
            report.update({"profile": profile, "ui": latest, "elapsedSeconds": round(time.monotonic() - started, 3), "passed": True})
        except Exception as error:
            report.update({"passed": False, "error": str(error), "elapsedSeconds": round(time.monotonic() - started, 3)})
            raise
        finally:
            try:
                if session:
                    request("DELETE", f"{base}/session/{session}", timeout=5)
            finally:
                try:
                    terminate(app)
                finally:
                    try:
                        terminate(driver)
                    finally:
                        (diagnostics / "result.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    line = f"first-launch: ok {platform}; actual bundled B11; no setup dialog; {report['ui']['visits']} visits; {report['elapsedSeconds']}s"
    print(line)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with Path(os.environ["GITHUB_STEP_SUMMARY"]).open("a", encoding="utf-8") as summary:
            summary.write(line + "\n\n")
    return report


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--acceptance-dir", required=True, type=Path)
    parser.add_argument("--driver", required=True, type=Path)
    args = parser.parse_args()
    try:
        run(args)
    except (AcceptanceError, OSError, ValueError, subprocess.SubprocessError) as error:
        parser.exit(1, f"first-launch: FAILED: {error}\n")


if __name__ == "__main__":
    main()
