"""Behavior tests for the independent oracle; no project source is inspected."""
import contextlib
import io
import json
from pathlib import Path
import shutil
import tempfile
import unittest

from sgfmill import sgf

import generate


class OracleTests(unittest.TestCase):
    def game(self, text):
        return generate.summarize_game(sgf.Sgf_game.from_bytes(text.encode('utf-8'),
                                                               override_encoding='UTF-8'))

    def fixture(self, name):
        manifest = json.loads((generate.DEFAULT_FIXTURES / 'manifest.json').read_text(encoding='utf-8'))
        entry = next(item for item in manifest['fixtures'] if item['file'] == name)
        return generate.generate_entry(generate.DEFAULT_FIXTURES / name, entry)

    def test_capture_counts_and_orientation(self):
        black = self.game('(;SZ[9]AB[ab][ba][cb]AW[bb];B[bc])')
        white = self.game('(;SZ[9]AW[ab][ba][cb]AB[bb];W[bc])')
        self.assertEqual(black['captured_by'], {'black': 1, 'white': 0})
        self.assertEqual(white['captured_by'], {'black': 0, 'white': 1})
        self.assertEqual(black['final_board'][:3], ['.X.......', 'X.X......', '.X.......'])

    def test_group_capture_and_ko_replay(self):
        group = self.fixture('capture-group.sgf')['games'][0]
        ko = self.fixture('ko-recapture-replay.sgf')['games'][0]
        self.assertEqual(group['captured_by'], {'black': 2, 'white': 0})
        self.assertEqual(ko['captured_by'], {'black': 1, 'white': 1})

    def test_suicide_is_credited_to_opponent(self):
        game = self.game('(;SZ[3]AW[ab][ba];B[aa])')
        self.assertEqual(game['captured_by'], {'black': 0, 'white': 1})
        self.assertEqual(game['final_board'][0], '.O.')

    def test_setup_removal_is_not_a_capture(self):
        game = self.fixture('setup-midgame.sgf')['games'][0]
        self.assertEqual(game['captured_by'], {'black': 0, 'white': 0})
        self.assertEqual(game['mainline_move_count'], 3)
        self.assertEqual([row[:5] for row in game['final_board'][:5]],
                         ['.....', '.O...', '..X..', '...O.', '....X'])

    def test_collections_keep_separate_games(self):
        result = self.fixture('collection-two.sgf')
        self.assertEqual([game['board_size']['width'] for game in result['games']], [9, 13])
        self.assertEqual([game['node_count'] for game in result['games']], [2, 3])
        self.assertEqual(result['games'][1]['final_board'][-1][-1], 'X')

    def test_branches_count_all_nodes_but_only_first_child_mainline(self):
        game = self.fixture('root-variations.sgf')['games'][0]
        self.assertEqual((game['root_branch_count'], game['node_count'],
                          game['mainline_move_count'], game['comment_count']), (3, 5, 2, 2))
        self.assertEqual(game['final_board'][2][2], '.')

    def test_passes_count_as_moves_without_stones(self):
        game = self.fixture('pass-empty.sgf')['games'][0]
        self.assertEqual(game['mainline_move_count'], 4)
        self.assertEqual(sum(row.count('X') + row.count('O') for row in game['final_board']), 2)
        old = self.fixture('pass-tt-ff3.sgf')['games'][0]
        self.assertEqual(old['mainline_move_count'], 3)
        self.assertEqual(sum(row.count('X') for row in old['final_board']), 1)

    def test_known_encodings_override_wrong_or_missing_ca(self):
        cases = [('utf8-bom.sgf', 'UTF-8-BOM'), ('gbk-no-ca.sgf', 'GBK'),
                 ('gb18030-no-ca.sgf', 'GB18030'), ('ca-mismatch-utf8.sgf', 'UTF-8'),
                 ('ca-mismatch-gb18030.sgf', 'GB18030')]
        for name, encoding in cases:
            with self.subTest(name=name):
                result = self.fixture(name)
                self.assertEqual(result['actual_encoding'], encoding)
                self.assertEqual(result['games'][0]['comment_count'], 1)
                self.assertEqual(result['games'][0]['final_board'][0], 'X........')

    def test_comment_escapes_are_parsed_by_independent_library(self):
        data = (generate.DEFAULT_FIXTURES / 'comment-escaped-bracket.sgf').read_bytes()
        root = sgf.Sgf_game.from_bytes(data, override_encoding='UTF-8').get_root()
        self.assertEqual(root.get('C'), 'Literal closing bracket: ] and open [ bracket.')
        data = (generate.DEFAULT_FIXTURES / 'comment-backslash-continuation.sgf').read_bytes()
        root = sgf.Sgf_game.from_bytes(data, override_encoding='UTF-8').get_root()
        self.assertEqual(root.get('C'), 'backslash \\ and joinedline')

    def test_unknown_properties_and_java_lz_are_retained_as_unknown(self):
        result = self.fixture('marks-and-unknown.sgf')['games'][0]
        self.assertEqual(result['unknown_property_names'], ['LZ', 'XQ', 'ZZ'])
        self.assertEqual(self.fixture('java-saved-lz.sgf')['games'][0]['unknown_property_names'], ['LZ'])

    def test_missing_metadata_is_not_invented(self):
        game = self.fixture('missing-metadata.sgf')['games'][0]
        self.assertEqual(game['board_size'], {'width': 19, 'height': 19})
        self.assertIsNone(game['komi'])
        self.assertIsNone(game['rules_raw'])
        self.assertEqual(game['handicap'], 0)

    def test_manual_answers_have_explicit_recovery_and_correct_board(self):
        result = self.fixture('rectangular-19x13.sgf')
        self.assertEqual(result['source'], 'manual')
        self.assertEqual(result['games'][0]['board_size'], {'width': 19, 'height': 13})
        self.assertEqual(result['games'][0]['final_board'][-1], '..................O')
        result = self.fixture('occupied-illegal.sgf')
        self.assertEqual(result['games'][0]['invalid_mainline_nodes'], [3])
        self.assertEqual(result['games'][0]['final_board'][0], 'X........')

    def test_manual_input_changes_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / 'occupied-illegal.sgf'
            data = (generate.DEFAULT_FIXTURES / target.name).read_bytes()
            target.write_bytes(data + b'\n')
            with self.assertRaisesRegex(ValueError, 'manual input changed'):
                generate.generate_entry(target, {'actual_encoding': 'UTF-8'})

    def test_check_detects_changed_expected_and_never_overwrites_it(self):
        with tempfile.TemporaryDirectory() as directory:
            copied = Path(directory) / 'sgf'
            shutil.copytree(generate.DEFAULT_FIXTURES, copied)
            target = copied / 'capture-black.expected.json'
            result = json.loads(target.read_text(encoding='utf-8'))
            result['games'][0]['captured_by']['black'] = 0
            target.write_text(json.dumps(result), encoding='utf-8')
            before = target.read_bytes()
            with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(generate.run(copied, check=True), 1)
            self.assertEqual(target.read_bytes(), before)

    def test_check_detects_changed_manual_answer(self):
        with tempfile.TemporaryDirectory() as directory:
            copied = Path(directory) / 'sgf'
            shutil.copytree(generate.DEFAULT_FIXTURES, copied)
            target = copied / 'rectangular-19x13.expected.json'
            result = json.loads(target.read_text(encoding='utf-8'))
            result['games'][0]['final_board'][0] = '.' * 19
            target.write_text(json.dumps(result), encoding='utf-8')
            with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(generate.run(copied, check=True), 1)

    def test_entire_checked_in_corpus_matches_independent_oracle(self):
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(generate.run(generate.DEFAULT_FIXTURES, check=True), 0)


if __name__ == '__main__':
    unittest.main()
