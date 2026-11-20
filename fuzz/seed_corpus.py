#!/usr/bin/env python3
"""Generate disposable libFuzzer seeds from checked-in fixtures (Python 3.9+)."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CORPUS = ROOT / "fuzz" / "corpus"
TARGETS = (
    "sgf_roundtrip", "gib", "document_walk", "lz", "katago_line",
    "gtp", "fox_replies", "readboard",
)


def put(target: str, data: bytes) -> None:
    directory = CORPUS / target
    directory.mkdir(parents=True, exist_ok=True)
    (directory / hashlib.sha256(data).hexdigest()).write_bytes(data)


def main() -> None:
    # Preserve original bytes, including legacy encodings and malformed SGF.
    sgfs = sorted((ROOT / "fixtures" / "sgf").glob("*.sgf"))
    sgfs += sorted((ROOT / "fixtures" / "lz").glob("*.sgf"))
    for source in sgfs:
        data = source.read_bytes()
        for target in ("sgf_roundtrip", "document_walk"):
            put(target, data)
        if "lz" in source.name or source.parent.name == "lz":
            # The LZ target extracts values using sgf::parse, not a second parser.
            for dimension in (7, 17):  # 9x9 and 19x19; mutations vary both axes.
                for color in (0, 1):
                    put("lz", bytes([dimension, dimension, color]) + data)

    for source in sorted((ROOT / "fixtures" / "gib").glob("*.gib")):
        put("gib", source.read_bytes())

    for source in sorted((ROOT / "fixtures" / "katago").glob("*.jsonl")):
        for line in source.read_text().splitlines():
            record = json.loads(line)
            if record.get("dir") == "out":
                put("katago_line", record["line"].encode("utf-8"))

    # GTP fixtures are protocol examples; retain complete sessions and replies.
    responses = (ROOT / "crates/gtp/fixtures/responses.gtp").read_bytes()
    put("gtp", responses)
    for reply in responses.split(b"\n\n"):
        if reply:
            put("gtp", reply + b"\n\n")
    for line in (ROOT / "fixtures/zhizi/kata-analyze.txt").read_bytes().splitlines():
        put("gtp", line)

    for directory in ("fox", "tencent"):
        for source in sorted((ROOT / "fixtures" / directory).iterdir()):
            if source.suffix not in (".json", ".txt"):
                continue
            put("fox_replies", source.read_bytes())
            if source.suffix == ".json":
                reply = json.loads(source.read_text())
                if isinstance(reply.get("chess"), str):
                    put("fox_replies", reply["chess"].encode("utf-8"))

    session = (ROOT / "fixtures/readboard/reconstructed-session.txt").read_bytes()
    put("readboard", session)
    for line in session.splitlines():
        if line and not line.startswith(b"#"):
            put("readboard", line)

    # Minimized regressions stay in ordinary tests and seed every future fuzz run.
    for target in TARGETS:
        directory = ROOT / "fixtures/fuzz" / target
        if directory.exists():
            for source in sorted(directory.rglob("*")):
                if source.is_file():
                    put(target, source.read_bytes())
        count = len(list((CORPUS / target).iterdir()))
        if count == 0:
            raise RuntimeError(f"no seeds for {target}")
        print(f"{target}: {count} corpus inputs")


if __name__ == "__main__":
    main()
