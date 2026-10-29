#!/usr/bin/env python3
"""Record real KataGo analysis-protocol conversations (Python 3.9+, stdlib only).

Run: python3 tools/record-katago/record.py --katago /opt/homebrew/bin/katago
       --model /opt/homebrew/share/katago/kata1-b18c384nbt-....bin.gz
Add --human-model PATH to also record HumanSL. No models are copied to the repo.
Each scenario runs in a fresh process with the supplied analysis.cfg. Recording
is driven by protocol responses, never sleeps. Cross-pipe ordering is observation
order, not an assertion about the engine's internal ordering. Existing fixtures
are replaced only after every requested scenario passes semantic validation.
"""

import argparse
import hashlib
import json
import math
import os
import queue
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

from validate import SCENARIOS, validate_transcript


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_CONFIG = ROOT / "resources" / "katago" / "analysis.cfg"
DEFAULT_OUTPUT = ROOT / "fixtures" / "katago"


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def query(query_id, **fields):
    result = dict(id=query_id, moves=[], rules="chinese", komi=7.5,
                  boardXSize=19, boardYSize=19, maxVisits=100)
    result.update(fields)
    return result


def streaming(query_id, **fields):
    return query(query_id, maxVisits=200, reportDuringSearchEvery=0.2,
                 firstReportDuringSearchAfter=0.01, **fields)


def is_final(response, query_id):
    return response.get("id") == query_id and response.get("isDuringSearch") is False


class Recording:
    def __init__(self, args, workdir):
        self.timeout = args.timeout
        self.events = []
        self.responses = []
        self.inbox = queue.Queue()
        self.ended = set()
        self.started = time.monotonic_ns()
        # Relative config path keeps user-specific workspace paths out of stderr.
        shutil.copyfile(args.config, workdir / "analysis.cfg")
        # KataGo echoes model paths verbatim. Relative links retain raw output
        # without recording private home directories or copying large models.
        model_link = "main-model" + "".join(args.model.suffixes)
        (workdir / model_link).symlink_to(args.model)
        command = [str(args.katago), "analysis", "-config", "analysis.cfg",
                   "-model", model_link]
        if args.human_model:
            human_link = "human-model" + "".join(args.human_model.suffixes)
            (workdir / human_link).symlink_to(args.human_model)
            command += ["-human-model", human_link]
        self.process = subprocess.Popen(
            command, cwd=workdir, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, text=True, encoding="utf-8", bufsize=1,
        )
        self.readers = []
        for direction, stream in (("out", self.process.stdout),
                                  ("err", self.process.stderr)):
            reader = threading.Thread(target=self._read, args=(direction, stream),
                                      daemon=True)
            reader.start()
            self.readers.append(reader)

    def _event(self, direction, line):
        return {"dir": direction,
                "t_ms": round((time.monotonic_ns() - self.started) / 1_000_000, 3),
                "line": line}

    def _read(self, direction, stream):
        try:
            for line in stream:
                self.inbox.put(self._event(direction, line.rstrip("\r\n")))
        finally:
            stream.close()
            self.inbox.put({"dir": direction, "eof": True})

    def send(self, *messages):
        lines = []
        for message in messages:
            line = json.dumps(message, ensure_ascii=False, separators=(",", ":"))
            self.events.append(self._event("in", line))
            lines.append(line + "\n")
        self.process.stdin.write("".join(lines))
        self.process.stdin.flush()

    def _receive(self, deadline):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("Timed out waiting for KataGo output")
        try:
            event = self.inbox.get(timeout=remaining)
        except queue.Empty as error:
            raise TimeoutError("Timed out waiting for KataGo output") from error
        if event.get("eof"):
            self.ended.add(event["dir"])
            return
        self.events.append(event)
        if event["dir"] == "out":
            value = json.loads(event["line"])
            if not isinstance(value, dict):
                raise ValueError("KataGo emitted a non-object JSON response")
            self.responses.append(value)

    def wait(self, predicate):
        deadline = time.monotonic() + self.timeout
        while not predicate(self.responses):
            if len(self.ended) == 2:
                raise RuntimeError("KataGo exited before the expected response")
            self._receive(deadline)

    def wait_partial(self, query_id):
        self.wait(lambda values: any(v.get("id") == query_id and
                                     v.get("isDuringSearch") is True for v in values))

    def wait_final(self, *query_ids):
        self.wait(lambda values: all(any(is_final(v, q) for v in values)
                                     for q in query_ids))

    def wait_ack(self, action_id):
        self.wait(lambda values: any(v.get("id") == action_id and
                                     "action" in v for v in values))

    def close(self):
        self.process.stdin.close()
        deadline = time.monotonic() + self.timeout
        while len(self.ended) < 2:
            self._receive(deadline)
        code = self.process.wait(timeout=max(0.001, deadline - time.monotonic()))
        for reader in self.readers:
            reader.join(timeout=1)
        if code != 0:
            raise RuntimeError("KataGo exited with status {}".format(code))

    def abort(self):
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait(timeout=5)
        if not self.process.stdin.closed:
            self.process.stdin.close()
        for reader in self.readers:
            reader.join(timeout=1)


def record_scenario(name, session):
    session.send({"id": "version", "action": "query_version"})
    session.wait(lambda values: any(v.get("id") == "version" and
                                   "version" in v for v in values))
    if name == "query-version":
        return
    if name == "opening-ownership-policy":
        session.send(query("opening", moves=[["B", "Q16"], ["W", "D4"]],
                           includeOwnership=True, includePolicy=True))
        session.wait_final("opening")
    elif name == "during-search":
        session.send(streaming("stream"))
        session.wait_final("stream")
    elif name == "terminate-running":
        session.send(streaming("running"))
        session.wait_partial("running")
        session.send({"id": "cancel-running", "action": "terminate",
                      "terminateId": "running"})
        session.wait_ack("cancel-running")
        session.wait_final("running")
    elif name in ("terminate-queued", "terminate-all"):
        session.send(streaming("blocker", priority=100))
        session.wait_partial("blocker")
        waiting = query("queued", priority=-100)
        if name == "terminate-queued":
            session.send(waiting, {"id": "cancel-queued", "action": "terminate",
                                   "terminateId": "queued"})
            session.wait_ack("cancel-queued")
            session.wait_final("queued")
            session.send({"id": "cancel-blocker", "action": "terminate",
                          "terminateId": "blocker"})
            session.wait_ack("cancel-blocker")
            session.wait_final("blocker")
        else:
            session.send(waiting, query("queued-2", priority=-200),
                         {"id": "cancel-all", "action": "terminate_all"})
            session.wait_ack("cancel-all")
            session.wait_final("blocker", "queued", "queued-2")
    elif name == "analyze-turns-priority":
        # Block the only worker before queuing the batch, so all five turns are
        # eligible together. priority never preempts an already-running search.
        session.send(streaming("blocker", priority=100))
        session.wait_partial("blocker")
        session.send(query("batch", moves=[["B", "Q16"], ["W", "D4"],
                                           ["B", "Q4"], ["W", "D16"]],
                           analyzeTurns=[0, 1, 2, 3, 4], priorities=[0, 40, 30, 20, 10],
                           maxVisits=50),
                     {"id": "cancel-blocker", "action": "terminate",
                      "terminateId": "blocker"})
        session.wait_ack("cancel-blocker")
        session.wait_final("blocker")
        session.wait(lambda values: {v.get("turnNumber") for v in values
                                    if is_final(v, "batch")} == {0, 1, 2, 3, 4})
    elif name == "board-9":
        session.send(query("small", boardXSize=9, boardYSize=9,
                           moves=[["B", "E5"], ["W", "C3"]]))
        session.wait_final("small")
    elif name == "pass":
        session.send(query("pass", moves=[["B", "D4"], ["W", "pass"],
                                          ["B", "Q16"], ["W", "pass"]]))
        session.wait_final("pass")
    elif name == "illegal-move":
        session.send(query("illegal", moves=[["B", "D4"], ["W", "D4"]]))
        session.wait(lambda values: any(v.get("id") == "illegal" and
                                       "error" in v for v in values))
    elif name == "unknown-field":
        session.send(query("unknown", fixtureUnknownField=True))
        session.wait_final("unknown")
    elif name == "human-sl":
        session.send(query("human", includePolicy=True,
                           overrideSettings={"humanSLProfile": "rank_5k"}))
        session.wait_final("human")
    else:
        raise ValueError("Unknown scenario: " + name)


def metadata(args):
    version = subprocess.run([str(args.katago), "version"], check=True,
                             capture_output=True, text=True, timeout=args.timeout).stdout
    version_match = re.search(r"^KataGo (\S+)", version, re.MULTILINE)
    backend_match = re.search(r"^Using (.+) backend", version, re.MULTILINE)
    if not version_match or not backend_match:
        raise ValueError("Could not identify KataGo version/backend: " + version)
    meta = {"format_version": 1, "katago_version": version_match.group(1),
            "model": args.model.name, "model_sha256": sha256(args.model),
            "backend": backend_match.group(1), "config_sha256": sha256(args.config),
            "analysis_threads": 1, "report_perspective": "BLACK"}
    if args.human_model:
        meta.update(human_model=args.human_model.name,
                    human_model_sha256=sha256(args.human_model))
    return meta


def publish(completed, output):
    """Stage every write, then replace each fixture atomically (not the whole set)."""
    output.mkdir(parents=True, exist_ok=True)
    staged = []
    try:
        for name, content in completed.items():
            descriptor, filename = tempfile.mkstemp(prefix=".record-", suffix=".tmp", dir=output)
            temporary = Path(filename)
            staged.append((temporary, output / (name + ".jsonl")))
            with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as stream:
                stream.write(content)
                stream.flush()
                os.fsync(stream.fileno())
        for temporary, destination in staged:
            os.replace(temporary, destination)
    finally:
        for temporary, _destination in staged:
            if temporary.exists():
                temporary.unlink()


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--katago", type=Path, default=Path("/opt/homebrew/bin/katago"))
    parser.add_argument("--model", required=True, type=Path)
    parser.add_argument("--human-model", type=Path)
    parser.add_argument("--config", type=Path, default=DEFAULT_CONFIG)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--timeout", type=float, default=120,
                        help="safety deadline per response sequence, in seconds")
    args = parser.parse_args(argv)
    if not math.isfinite(args.timeout) or args.timeout <= 0:
        parser.error("--timeout must be positive")
    for key in ("katago", "model", "config", "human_model"):
        path = getattr(args, key)
        if path is not None:
            setattr(args, key, path.expanduser().resolve(strict=True))
    meta = metadata(args)
    names = list(SCENARIOS) + (["human-sl"] if args.human_model else [])
    # Stage every file before publishing; a failed GPU init/validation does not
    # overwrite a previously good corpus with a partial recording.
    completed = {}
    for name in names:
        with tempfile.TemporaryDirectory(prefix="katago-fixture-") as temp:
            session = Recording(args, Path(temp))
            try:
                record_scenario(name, session)
                session.close()
                events = sorted(session.events, key=lambda event: event["t_ms"])
                rows = [dict(meta, scenario=name)] + events
                validate_transcript(rows, name)
                content = "".join(json.dumps(row, ensure_ascii=False,
                                             separators=(",", ":")) + "\n" for row in rows)
                if len(content.encode("utf-8")) > 300_000:
                    raise ValueError(name + " exceeds 300KB; increase the report interval")
                completed[name] = content
                print("record: {} ok ({} bytes)".format(name, len(content.encode("utf-8"))),
                      flush=True)
            except BaseException:
                session.abort()
                for event in session.events[-10:]:
                    if event["dir"] == "err":
                        print(event["line"], file=sys.stderr)
                raise
    publish(completed, args.output)
    if not args.human_model:
        print("record: HumanSL skipped (no --human-model supplied)")
    print("record: ALL PASSED ({} scenarios)".format(len(completed)))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        print("record: FAILED: {}".format(error), file=sys.stderr)
        sys.exit(1)
