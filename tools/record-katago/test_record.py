"""Offline behavioral validation of golden conversations; no engine or GPU.

Run: python3 -m unittest discover -s tools/record-katago -v
"""

import copy
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from record import publish
from validate import SCENARIOS, validate_transcript


FIXTURES = Path(__file__).resolve().parents[2] / "fixtures" / "katago"


def load(name):
    path = FIXTURES / (name + ".jsonl")
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()]


def output_matches(row, query_id, field, value):
    if row.get("dir") != "out":
        return False
    message = json.loads(row["line"])
    return message.get("id") == query_id and message.get(field) == value


class TranscriptTests(unittest.TestCase):
    def test_all_recorded_scenarios(self):
        for name in SCENARIOS:
            with self.subTest(scenario=name):
                validate_transcript(load(name), name)
                self.assertLessEqual((FIXTURES / (name + ".jsonl")).stat().st_size, 300_000)
        if (FIXTURES / "human-sl.jsonl").exists():
            validate_transcript(load("human-sl"), "human-sl")

    def test_missing_partial_is_detected(self):
        rows = load("during-search")
        partials = [row for row in rows if output_matches(row, "stream", "isDuringSearch", True)]
        rows = [row for row in rows if row not in partials[2:]]
        with self.assertRaisesRegex(ValueError, "at least three partials"):
            validate_transcript(rows, "during-search")

    def test_missing_final_is_detected(self):
        rows = [row for row in load("during-search")
                if not output_matches(row, "stream", "isDuringSearch", False)]
        with self.assertRaisesRegex(ValueError, "missing or duplicate final"):
            validate_transcript(rows, "during-search")

    def test_duplicate_final_is_detected(self):
        rows = load("board-9")
        index = next(i for i, row in enumerate(rows)
                     if output_matches(row, "small", "isDuringSearch", False))
        rows.insert(index, copy.deepcopy(rows[index]))
        with self.assertRaisesRegex(ValueError, "missing or duplicate final"):
            validate_transcript(rows, "board-9")

    def test_missing_cancel_ack_is_detected(self):
        rows = [row for row in load("terminate-running")
                if not output_matches(row, "cancel-running", "action", "terminate")]
        with self.assertRaisesRegex(ValueError, "missing action ACK"):
            validate_transcript(rows, "terminate-running")

    def test_queued_cancel_cannot_be_reported_as_searched(self):
        rows = load("terminate-queued")
        replacement = next(json.loads(row["line"]) for row in rows
                           if output_matches(row, "blocker", "isDuringSearch", False))
        for row in rows:
            if output_matches(row, "queued", "noResults", True):
                row["line"] = json.dumps(dict(replacement, id="queued"))
        with self.assertRaisesRegex(ValueError, "queued cancellation"):
            validate_transcript(rows, "terminate-queued")

    def test_warning_is_required(self):
        rows = load("unknown-field")
        rows = [row for row in rows if row.get("dir") != "out" or
                "warning" not in json.loads(row["line"])]
        with self.assertRaisesRegex(ValueError, "unknown-field warning"):
            validate_transcript(rows, "unknown-field")

    def test_ownership_shape_is_checked(self):
        rows = load("opening-ownership-policy")
        for row in rows:
            if output_matches(row, "opening", "isDuringSearch", False):
                value = json.loads(row["line"])
                value["ownership"].pop()
                row["line"] = json.dumps(value)
        with self.assertRaisesRegex(ValueError, "19x19 ownership"):
            validate_transcript(rows, "opening-ownership-policy")

    def test_timestamp_regression_is_detected(self):
        rows = load("query-version")
        rows[-1]["t_ms"] = -1
        with self.assertRaisesRegex(ValueError, "timestamps"):
            validate_transcript(rows, "query-version")

    def test_no_results_requires_cancellation(self):
        rows = load("board-9")
        for row in rows:
            if output_matches(row, "small", "isDuringSearch", False):
                row["line"] = json.dumps(dict(id="small", turnNumber=2,
                                              isDuringSearch=False, noResults=True))
        with self.assertRaisesRegex(ValueError, "without a preceding cancellation"):
            validate_transcript(rows, "board-9")

    def test_metadata_cannot_mislabel_white_perspective(self):
        rows = load("query-version")
        for row in rows:
            if row.get("dir") == "err" and row["line"] == "reportAnalysisWinratesAs = BLACK":
                row["line"] = "reportAnalysisWinratesAs = WHITE"
        with self.assertRaisesRegex(ValueError, "effective engine config mismatch"):
            validate_transcript(rows, "query-version")

    def test_publish_replaces_complete_files(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            (output / "sample.jsonl").write_text("old", encoding="utf-8")
            publish({"sample": "new\n", "second": "second\n"}, output)
            self.assertEqual((output / "sample.jsonl").read_text(), "new\n")
            self.assertEqual((output / "second.jsonl").read_text(), "second\n")
            self.assertEqual(len(list(output.iterdir())), 2)

    def test_failed_publish_does_not_truncate_old_fixture(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory)
            original = output / "sample.jsonl"
            original.write_text("old", encoding="utf-8")
            with patch("record.os.replace", side_effect=OSError("simulated disk failure")):
                with self.assertRaises(OSError):
                    publish({"sample": "new\n"}, output)
            self.assertEqual(original.read_text(), "old")
            self.assertEqual(list(output.iterdir()), [original])


if __name__ == "__main__":
    unittest.main()
