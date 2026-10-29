#!/usr/bin/env python3
"""Independent SGF fixture oracle (Python 3.9+, sgfmill 1.1.1).

Install: python3 -m pip install -r tools/fixture-oracle/requirements.txt
Generate: python3 tools/fixture-oracle/generate.py
Verify without writing: python3 tools/fixture-oracle/generate.py --check

All parsing, SGF coordinate interpretation, setup and capture rules come from
sgfmill. Byte encoding is known from fixture authorship (manifest.json), not
inferred from the intentionally incorrect CA properties. Decode before parsing
so a GBK trail byte equal to a backslash is not misread as SGF escaping.
See manifest.json for board orientation, count and recovery conventions.
"""
from __future__ import annotations

import argparse
import hashlib
from importlib.metadata import version
import json
import sys
from pathlib import Path

from sgfmill import boards, sgf, sgf_grammar

from manual import MANUAL

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_FIXTURES = ROOT / 'fixtures' / 'sgf'
ORACLE_VERSION = '1.1.1'
CODECS = {'UTF-8': 'utf-8', 'UTF-8-BOM': 'utf-8-sig', 'GBK': 'gbk', 'GB18030': 'gb18030'}
# SGF FF[4] standard property identifiers; LZ and vendor properties are private.
STANDARD = frozenset('AB AE AN AP AR AW B BL BM BR BT C CA CP CR DD DM DO DT EV FF '
                     'FG GB GC GM GN GW HA HO IT KM KO LB LN MA MN N OB ON OT OW PB '
                     'PC PL PM PW RE RO RU SL SO SQ ST SZ TB TE TM TR TW UC US V VW '
                     'W WL WR WT'.split())


def nodes_preorder(node):
    yield node
    for child in node:
        yield from nodes_preorder(child)


def raw_text(node, prop):
    return node.get_raw(prop).decode('utf-8') if node.has_property(prop) else None


def stone_counts(board):
    counts = {'b': 0, 'w': 0}
    for colour, _point in board.list_occupied_points():
        counts[colour] += 1
    return counts


def summarize_game(game):
    root = game.get_root()
    all_nodes = list(nodes_preorder(root))
    board = boards.Board(game.get_size())
    captured = {'black': 0, 'white': 0}
    mainline_moves = 0
    for node in game.get_main_sequence():
        if node.has_setup_stones():
            if not board.apply_setup(*node.get_setup_stones()):
                raise ValueError('setup has a group without liberties; requires a manual expectation')
        colour, move = node.get_move()
        if colour is None:
            continue
        mainline_moves += 1
        if move is None:
            continue
        before = stone_counts(board)
        board.play(*move, colour)
        after = stone_counts(board)
        opponent = 'w' if colour == 'b' else 'b'
        captured['black' if colour == 'b' else 'white'] += before[opponent] - after[opponent]
        captured['white' if colour == 'b' else 'black'] += before[colour] + 1 - after[colour]
    symbols = {None: '.', 'b': 'X', 'w': 'O'}
    size = game.get_size()
    return {
        'board_size': {'width': size, 'height': size},
        'komi': root.get('KM') if root.has_property('KM') else None,
        'rules_raw': raw_text(root, 'RU'),
        'handicap': root.get('HA') if root.has_property('HA') else 0,
        'node_count': len(all_nodes),
        'mainline_move_count': mainline_moves,
        'root_branch_count': len(root),
        'comment_count': sum(node.has_property('C') for node in all_nodes),
        'unknown_property_names': sorted({prop for node in all_nodes
                                          for prop in node.properties() if prop not in STANDARD}),
        'final_board': [''.join(symbols[board.get(row, col)] for col in range(size))
                        for row in reversed(range(size))],
        'captured_by': captured,
    }


def generate_entry(path, entry):
    data = path.read_bytes()
    encoding = entry['actual_encoding']
    text = data.decode(CODECS[encoding], errors='strict')
    if text.encode(CODECS[encoding]) != data:
        raise ValueError('fixture encoding does not round-trip')
    digest = hashlib.sha256(data).hexdigest()
    result = {'schema_version': 1, 'source': 'sgfmill', 'oracle_version': ORACLE_VERSION,
              'input_sha256': digest, 'actual_encoding': encoding}
    if path.name in MANUAL:
        answer = MANUAL[path.name]
        if digest != answer['input_sha256']:
            raise ValueError('manual input changed: review the independent handwritten answer')
        result.update(source='manual', manual_reason=answer['reason'], games=answer['games'])
        del result['oracle_version']
    else:
        trees = sgf_grammar.parse_sgf_collection(text.encode('utf-8'))
        games = [sgf.Sgf_game.from_coarse_game_tree(tree, override_encoding='UTF-8')
                 for tree in trees]
        result['games'] = [summarize_game(game) for game in games]
    return result


def validate_result(result):
    if not result['games']:
        raise ValueError('empty collection')
    for game in result['games']:
        width, height = game['board_size']['width'], game['board_size']['height']
        if not (2 <= width <= 25 and 2 <= height <= 25):
            raise ValueError('board dimensions out of range')
        if len(game['final_board']) != height or any(
                len(row) != width or set(row) - set('.XO') for row in game['final_board']):
            raise ValueError('invalid final board')
        for key in ('node_count', 'mainline_move_count', 'root_branch_count', 'comment_count', 'handicap'):
            if not isinstance(game[key], int) or game[key] < 0:
                raise ValueError('invalid count: ' + key)
        if game['node_count'] < 1 or game['mainline_move_count'] > game['node_count']:
            raise ValueError('inconsistent node counts')
        if set(game['captured_by']) != {'black', 'white'} or any(
                not isinstance(value, int) or value < 0 for value in game['captured_by'].values()):
            raise ValueError('invalid captures')
        if game['unknown_property_names'] != sorted(set(game['unknown_property_names'])):
            raise ValueError('unknown property names must be unique and sorted')


def run(fixtures, check=False):
    installed = version('sgfmill')
    if installed != ORACLE_VERSION:
        raise ValueError('install sgfmill==' + ORACLE_VERSION + '; found ' + installed)
    manifest = json.loads((fixtures / 'manifest.json').read_text(encoding='utf-8'))
    entries = manifest['fixtures']
    names = [entry['file'] for entry in entries]
    if len(names) != len(set(names)):
        raise ValueError('duplicate manifest filename')
    if set(names) != {path.name for path in fixtures.glob('*.sgf')}:
        raise ValueError('manifest and SGF file set differ')
    expected_names = {Path(name).with_suffix('.expected.json').name for name in names}
    extra = {path.name for path in fixtures.glob('*.expected.json')} - expected_names
    if extra:
        raise ValueError('orphan expected files: ' + ', '.join(sorted(extra)))
    failures = []
    manual_count = 0
    for entry in entries:
        path = fixtures / entry['file']
        try:
            result = generate_entry(path, entry)
            validate_result(result)
            manual_count += result['source'] == 'manual'
            target = path.with_suffix('.expected.json')
            if check:
                if not target.exists() or json.loads(target.read_text(encoding='utf-8')) != result:
                    failures.append(target.name + ': missing or differs from independent oracle')
            else:
                target.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
        except (ValueError, KeyError, IndexError, OSError) as error:
            failures.append(path.name + ': ' + str(error))
    for failure in failures:
        print(failure, file=sys.stderr)
    if failures:
        print('fixture-oracle: FAILED (%d files)' % len(failures))
        return 1
    print('fixture-oracle: ok (%d files, %d sgfmill, %d manual)' %
          (len(entries), len(entries) - manual_count, manual_count))
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='compare; never write expected files')
    parser.add_argument('--fixtures', type=Path, default=DEFAULT_FIXTURES)
    args = parser.parse_args()
    try:
        return run(args.fixtures, args.check)
    except (ValueError, KeyError, OSError) as error:
        print('fixture-oracle: ' + str(error), file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
