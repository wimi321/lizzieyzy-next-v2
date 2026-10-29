"""Behavior tests: python3 -B -m unittest discover -s tools/convert-l10n."""

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from convert import language_tag, parse_properties


class PropertiesTests(unittest.TestCase):
    def test_separators_comments_whitespace_and_escaped_keys(self):
        text = (
            "  # comment\n\t! another comment\r\n"
            "equals=value=with:separators\ncolon: value\nspace \t : value  \n"
            "only-key\n=empty key\n"
            "escaped\\ key\\:\\==\\ leading and trailing\\ \n"
            "\\#key=literal # and !\n"
        )
        self.assertEqual(parse_properties(text), {
            "equals": "value=with:separators",
            "colon": "value",
            "space": "value  ",
            "only-key": "",
            "": "empty key",
            "escaped key:=": " leading and trailing ",
            "#key": "literal # and !",
        })

    def test_continuations_and_even_backslashes(self):
        text = (
            "# comments do not continue\\\n"
            "one=first\\\r\n  second\\\r\tthird\n"
            "comment=first\\\n #not a comment\n"
            "even=two\\\\\nnext=value\n"
            "odd=three\\\\\\\n continued\n"
            "blank=first\\\n\nnext-blank=value\n"
            "eof=last\\"
        )
        self.assertEqual(parse_properties(text), {
            "one": "firstsecondthird",
            "comment": "first#not a comment",
            "even": "two\\",
            "next": "value",
            "odd": "three\\continued",
            "blank": "first",
            "next-blank": "value",
            "eof": "last",
        })

    def test_java_escapes_unicode_and_placeholders_are_preserved(self):
        self.assertEqual(parse_properties(
            r"\u006b=中文 \u4e2d\u6587 \uD83D\uDE00 {0} {1,number} \q\t\n\r\f\\"
        ), {"k": "中文 中文 😀 {0} {1,number} q\t\n\r\f\\"})
        self.assertEqual(parse_properties(r"lone=\uD800"), {"lone": "\ud800"})

    def test_empty_continuations_do_not_turn_comments_or_blank_lines_into_keys(self):
        self.assertEqual(parse_properties("\\\n# ignored\\\nkey=value\n\\\n! ignored\n\\\n\n"), {
            "key": "value",
        })
        self.assertEqual(parse_properties("\\\n"), {"": ""})
        self.assertEqual(parse_properties("\\\n\n"), {})

    def test_non_java_line_separators_are_values(self):
        self.assertEqual(parse_properties("value=a\fb\u2028c\u0085d"), {
            "value": "a\fb\u2028c\u0085d",
        })

    def test_duplicate_keys_follow_java_last_value_wins(self):
        self.assertEqual(parse_properties("key=first\n\\u006bey:second"), {"key": "second"})

    def test_invalid_unicode_escapes_report_logical_line(self):
        for value in (r"\u12", r"\uGGGG", r"\uu1234"):
            with self.subTest(value=value):
                with self.assertRaisesRegex(ValueError, "line 2: invalid Unicode"):
                    parse_properties("# comment\nkey=" + value)

    def test_bcp47_names_keep_regions_and_scripts(self):
        for suffix, tag in (
            ("", "en"), ("_en_US", "en-US"), ("_ja_JP", "ja-JP"),
            ("_ko", "ko"), ("_th_TH", "th-TH"), ("_zh_CN", "zh-CN"),
            ("_zh_HK", "zh-HK"), ("_zh_TW", "zh-TW"),
            ("_zh_Hant_TW", "zh-Hant-TW"),
        ):
            with self.subTest(suffix=suffix):
                self.assertEqual(language_tag(Path("DisplayStrings" + suffix + ".properties")), tag)
        with self.assertRaises(ValueError):
            language_tag(Path("DisplayStrings_broken!.properties"))


class CommandTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = self.root / "source"
        self.output = self.root / "output"
        self.source.mkdir()
        (self.source / "DisplayStrings.properties").write_text(
            "z=Last {0}\na=First\n", encoding="utf-8"
        )
        (self.source / "DisplayStrings_zh_CN.properties").write_text(
            "z=末尾 {0}\na=首项\n", encoding="utf-8"
        )

    def run_convert(self, *options):
        return subprocess.run(
            [sys.executable, "-B", str(Path(__file__).with_name("convert.py").resolve()),
             "--source-dir", str(self.source), "--output-dir", str(self.output), *options],
            cwd=self.root, capture_output=True, text=True, check=False,
        )

    def test_generation_is_sorted_utf8_and_check_never_writes(self):
        missing = self.run_convert("--check")
        self.assertNotEqual(missing.returncode, 0)
        self.assertFalse(self.output.exists())
        self.assertIn("missing file: en.json", missing.stderr)
        generated = self.run_convert()
        self.assertEqual(generated.returncode, 0, generated.stderr)
        self.assertEqual((self.output / "en.json").read_bytes(), b'{\n  "a": "First",\n  "z": "Last {0}"\n}\n')
        self.assertEqual(json.loads((self.output / "zh-CN.json").read_text()), {
            "a": "首项", "z": "末尾 {0}",
        })
        before = {path.name: (path.read_bytes(), path.stat().st_mtime_ns) for path in self.output.iterdir()}
        checked = self.run_convert("--check")
        self.assertEqual(checked.returncode, 0, checked.stderr)
        self.assertEqual(before, {
            path.name: (path.read_bytes(), path.stat().st_mtime_ns) for path in self.output.iterdir()
        })
        (self.output / "en.json").write_text('{"z":"Last {9}"}\n', encoding="utf-8")
        bad_bytes = (self.output / "en.json").read_bytes()
        changed = self.run_convert("--check")
        self.assertNotEqual(changed.returncode, 0)
        self.assertIn("out of date: en.json", changed.stderr)
        self.assertEqual((self.output / "en.json").read_bytes(), bad_bytes)

    def test_check_rejects_missing_and_obsolete_language_files(self):
        self.assertEqual(self.run_convert().returncode, 0)
        (self.output / "zh-CN.json").unlink()
        (self.output / "obsolete.json").write_text("{}", encoding="utf-8")
        result = self.run_convert("--check")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing file: zh-CN.json", result.stderr)
        self.assertIn("unexpected file: obsolete.json", result.stderr)

    def test_missing_source_and_invalid_unicode_fail_without_output(self):
        for path in self.source.iterdir():
            path.unlink()
        result = self.run_convert()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("no DisplayStrings*.properties", result.stderr)
        (self.source / "DisplayStrings.properties").write_text(r"key=\uZZZZ", encoding="utf-8")
        result = self.run_convert()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("DisplayStrings.properties: line 1", result.stderr)
        self.assertFalse(self.output.exists())

    def test_iso88591_fallback_and_surrogate_json(self):
        (self.source / "DisplayStrings.properties").write_bytes(b"latin=caf\xe9\nlone=\\uD800\n")
        result = self.run_convert()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads((self.output / "en.json").read_text(encoding="utf-8")), {
            "latin": "café", "lone": "\ud800",
        })


if __name__ == "__main__":
    unittest.main()
