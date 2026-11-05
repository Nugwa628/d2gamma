#!/usr/bin/env python3
"""Require extensible DataModFiles literals, including tests, tools and fuzz targets."""
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
TOKEN = re.compile(r"[A-Za-z_][A-Za-z_0-9]*|\.\.|::|->|[^\s]")
LITERAL = re.compile(r'''(?:br|cr|r)(?P<hashes>\#*)".*?"(?P=hashes)|(?:b|c)?"(?:\\.|[^"\\])*"|b?'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|.)|[^'\\\n])' ''', re.S | re.X)


def tokens(source):
    """Skip Rust comments and literals; keep source offsets for diagnostics."""
    i = 0
    while i < len(source):
        if source.startswith('//', i):
            end = source.find('\n', i)
            i = len(source) if end < 0 else end
        elif source.startswith('/*', i):
            i, depth = i + 2, 1
            while i < len(source) and depth:
                if source.startswith('/*', i):
                    depth, i = depth + 1, i + 2
                elif source.startswith('*/', i):
                    depth, i = depth - 1, i + 2
                else:
                    i += 1
        elif (m := LITERAL.match(source, i)) is not None:
            i = m.end()
        elif (m := TOKEN.match(source, i)) is not None:
            yield m.group(), i
            i = m.end()
        else:
            i += 1


def violations(source):
    ts = list(tokens(source))
    names = {'DataModFiles'}
    for i in range(len(ts) - 2):
        if ts[i][0] in names and ts[i + 1][0] == 'as':
            names.add(ts[i + 2][0])
    ends, stack = {}, []
    for i, (t, _) in enumerate(ts):
        if t == '{':
            stack.append(i)
        elif t == '}' and stack:
            ends[stack.pop()] = i
    impls = []
    for i, (t, _) in enumerate(ts):
        if t == 'impl':
            j = i + 1
            while j < len(ts) and ts[j][0] not in ('{', ';'):
                j += 1
            if j in ends and any(x in names for x, _ in ts[i:j]):
                impls.append((j, ends[j]))
    bad = []
    for i, (t, offset) in enumerate(ts[:-1]):
        is_self = t == 'Self' and any(a < i < b for a, b in impls)
        if t not in names and not is_self:
            continue
        if i and ts[i - 1][0] in ('struct', '->'):
            continue
        j = i + 1
        # Rust permits explicit lifetime parameters in construction paths.
        if ts[j][0] == '::':
            j += 1
        if j < len(ts) and ts[j][0] == '<':
            while j < len(ts) and ts[j][0] != '>':
                j += 1
            j += 1
        if j not in ends or any(j == a for a, _ in impls):
            continue
        depth, update = 0, False
        for x, _ in ts[j + 1:ends[j]]:
            if x == '..' and depth == 0:
                update = True
            depth += (x in ('{', '(', '[')) - (x in ('}', ')', ']'))
        if not update:
            bad.append(source.count('\n', 0, offset) + 1)
    return bad


def selftest():
    assert violations('let x = DataModFiles { rules: None };') == [1]
    assert violations('let x = rules::DataModFiles { rules: f({x: 1, ..d}) };') == [1]
    assert not violations('let x = DataModFiles { rules: None, ..Default::default() };')
    assert not violations('let x = DataModFiles { states: f(), ..*files };')
    assert not violations('struct DataModFiles { rules: u8 }')
    assert not violations('pub struct DataModFiles<\'a> { rules: &\'a str }')
    assert violations('use x::DataModFiles as Files; let x = Files { rules: None };') == [1]
    assert violations('impl<\'a> DataModFiles<\'a> { fn new() -> Self { Self {} } }') == [1]
    assert not violations('impl DataModFiles { fn new() -> Self { Self { ..Self::default() } } }')
    assert not violations('fn new() -> DataModFiles { DataModFiles::default() }')
    assert violations('DataModFiles::<\'_> {}') == [1]
    assert not violations('/* outer /* inner */ DataModFiles {} */ // DataModFiles {}\n')
    assert not violations('let s = r###"DataModFiles { }"###; let t = "DataModFiles {}";')
    assert not violations("let c = '{'; let x = DataModFiles { ..Default::default() };")
    print('datamod_files: selftest OK')


def check():
    paths = subprocess.check_output(['git', 'ls-files', '-z', '--', '*.rs'], cwd=ROOT)
    count, errors = 0, 0
    for path in paths.decode('utf-8').split('\0'):
        if not path:
            continue
        source = (ROOT / path).read_text(encoding='utf-8')
        if 'DataModFiles' not in source:
            continue
        count += 1
        for line in violations(source):
            print(f'{path}:{line}: DataModFiles construction needs a constructor or a struct update (..Default::default())')
            errors += 1
    print(f'datamod_files: audited {count} Rust files, {errors} exhaustive literals')
    return int(bool(errors))


if __name__ == '__main__':
    if sys.argv[1:] == ['--selftest']:
        selftest()
    else:
        raise SystemExit(check())
