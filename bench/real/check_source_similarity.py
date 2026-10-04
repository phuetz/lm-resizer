#!/usr/bin/env python3
"""Structural clone alarm against the pinned oracle, not a proof of authorship.

Scan every Rust function in src/ (including functions AFTER test modules).
Ignore test-only items, declarations and comments. Local identifiers, strings
and numbers are normalized; member API names and Rust keywords are retained.
Report matches of 32 tokens, reject contiguous matches of 56 tokens. Entire small bodies are compared from 12 tokens while retaining called APIs. These
limits intentionally permit short Rust idioms; review the audit as well.
Also reject shared multi-scope truncation policies: local AND global omission
messages, at least one matching omission template, and two shared numeric
selection limits (comparisons, take/skip/min/max), resolving literal bindings. This corroborating file-level alarm survives
helper extraction and reordered statements; it is not semantic clone proof.
"""
import argparse
import bisect
import collections
import hashlib
import json
from pathlib import Path
import re
import tarfile

ROOT = Path(__file__).resolve().parents[2]
WINDOW = 32
LIMIT = 56
SHORT_LIMIT = 12
LEX = re.compile(r'//[^\n]*|/\*[\s\S]*?\*/|r(\#*)"[\s\S]*?"\1|"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])\'|\b\d[\w.]*|\b[a-zA-Z_]\w*|\s+|.', re.M)
KEYWORDS = set('fn pub let mut if else match for in while loop return break continue const static struct enum impl use mod true false None Some Ok Err Self self as where trait type unsafe async await move ref dyn'.split())


def functions(source, preserve_calls=False):
    """Return normalized function bodies and original line numbers."""
    entries = [(m[0], m.start()) for m in LEX.finditer(source)
               if not m[0].isspace() and not m[0].startswith(('//', '/*'))]
    tokens = [v for v, _ in entries]
    pairs, stack = {}, []
    for i, t in enumerate(tokens):
        if t in ('{', '[', '('):
            stack.append(i)
        elif t in ('}', ']', ')') and stack:
            start = stack.pop()
            pairs[start] = i
    excluded = set()
    for i in range(len(tokens) - 2):
        if tokens[i:i+2] != ['#', '['] or i + 1 not in pairs:
            continue
        end = pairs[i+1]
        attr = ''.join(tokens[i+2:end])
        if attr not in ('cfg(test)', 'test', 'tokio::test'):
            continue
        body = next((j for j in range(end+1, len(tokens)) if tokens[j] in ('{', ';')), None)
        if body is not None and tokens[body] == '{' and body in pairs:
            excluded.update(range(i, pairs[body] + 1))
    newlines = [m.start() for m in re.finditer('\n', source)]
    result = []
    for i, token in enumerate(tokens):
        if token != 'fn' or i in excluded:
            continue
        body = next((j for j in range(i+1, len(tokens)) if tokens[j] in ('{', ';')), None)
        if body is None or tokens[body] != '{' or body not in pairs:
            continue
        normal, lines = [], []
        for j in range(body + 1, pairs[body]):
            t, offset = entries[j]
            if t.startswith(('"', 'r"', 'r#', "'")) and (t.endswith('"') or len(t) > 1 and t.endswith("'")):
                v = 'STRING'
            elif t[0].isdigit():
                v = 'NUMBER'
            elif re.fullmatch(r'[a-zA-Z_]\w*', t):
                v = t if t in KEYWORDS or tokens[j-1] == '.' or (preserve_calls and (tokens[j-1] == ':' or tokens[j+1] in (':', '('))) else 'IDENT'
            else:
                v = t
            normal.append(v)
            lines.append(bisect.bisect_left(newlines, offset) + 1)
        result.append((tokens[i+1], normal, lines))
    return result


def oracle_sources(archive):
    pin = json.loads((ROOT / 'bench/rtk-parity/reference.json').read_text())
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    if digest != pin['source_archive_sha256']:
        raise ValueError('oracle source archive checksum mismatch')
    sources = {}
    with tarfile.open(archive) as tar:
        for member in tar.getmembers():
            relative = member.name.split('/', 1)[-1]
            if member.isfile() and relative.startswith('src/') and relative.endswith('.rs'):
                sources[relative] = tar.extractfile(member).read().decode()
    if not sources:
        raise ValueError('oracle archive contains no Rust sources')
    return sources, digest


def compare(local, upstream):
    index = collections.defaultdict(list)
    bodies = []
    for path, source in sorted(upstream.items()):
        for name, tokens, lines in functions(source):
            number = len(bodies)
            bodies.append((path, name, tokens, lines))
            for start in range(len(tokens) - WINDOW + 1):
                index[tuple(tokens[start:start+WINDOW])].append((number, start))
    matches = []
    for path, source in sorted(local.items()):
        for name, tokens, lines in functions(source):
            cursor = 0
            while cursor + WINDOW <= len(tokens):
                best = None
                for number, start in index.get(tuple(tokens[cursor:cursor+WINDOW]), ()):
                    other_path, other_name, other, other_lines = bodies[number]
                    length = WINDOW
                    while cursor+length < len(tokens) and start+length < len(other) and tokens[cursor+length] == other[start+length]:
                        length += 1
                    if best is None or length > best['tokens']:
                        best = dict(file=path, function=name, line=lines[cursor], tokens=length,
                                    oracle_file=other_path, oracle_function=other_name,
                                    oracle_line=other_lines[start])
                if best:
                    matches.append(best)
                    cursor += best['tokens']
                else:
                    cursor += 1
    return sorted(matches, key=lambda v: (-v['tokens'], v['file'], v['line']))


def small_functions(local, upstream):
    """Compare entire short bodies, retaining external call/namespace identity.

    A window of normalized Rust idioms is weak evidence at this scale. Whole
    bodies with the same called APIs provide the additional discrimination.
    Local variables and the enclosing function name may still be renamed.
    """
    reference = collections.defaultdict(list)
    for path, source in upstream.items():
        for name, tokens, lines in functions(source, preserve_calls=True):
            if SHORT_LIMIT <= len(tokens) < LIMIT:
                reference[tuple(tokens)].append((path, name, lines[0]))
    found = []
    for path, source in local.items():
        for name, tokens, lines in functions(source, preserve_calls=True):
            for other, other_name, line in reference.get(tuple(tokens), []):
                found.append(dict(file=path, function=name, line=lines[0], tokens=len(tokens),
                                  oracle_file=other, oracle_function=other_name, oracle_line=line))
    return found


def policy_features(source):
    """File-level fingerprint survives helper extraction and statement reordering.

    Retain numeric decisions and descriptive output templates, discarded by the
    lexical normalization. Tests are excluded using the same item boundaries.
    This is a corroborating alarm, not a general semantic-equivalence solver.
    """
    spans = set()
    for _, _, lines in functions(source):
        if lines:
            spans.update(range(lines[0] - 1, lines[-1]))
    source_lines = source.splitlines()
    production = '\n'.join(source_lines[i] for i in sorted(spans))
    messages = set()
    for token in LEX.finditer(production):
        text = token[0]
        if text.startswith(('//', '/*')):
            continue
        if text.startswith(('"', 'r"', 'r#')):
            text = text[text.index('"')+1:text.rfind('"')]
            text = re.sub(r'\{[^{}]*\}', '{}', text)
            text = text.replace(r'\n', ' ').replace(r'\t', ' ').replace(r'\r', ' ')
            text = ' '.join(text.split())
            if len(text) >= 14 and sum(c.isalpha() for c in text) >= 8:
                messages.add(text)
    # Only values used in a selection expression count as policy thresholds.
    # Resolve simple bindings so `take(100)` and `let cap=100; n < cap`
    # have the same signature, without counting array sizes or parser indexes.
    code = ' '.join(t[0] for t in LEX.finditer(production)
                    if not t[0].isspace() and not t[0].startswith(('"', 'r"', 'r#', '//', '/*')))
    bindings = {name: int(value.replace('_', '')) for name, value in
                re.findall(r'\blet (?:mut )?(\w+) (?:[^=;]* )?= ([0-9][0-9_]*) ;', code)}
    operands = re.findall(r'\. (?:take|skip|min|max) \( (\w+) \)', code)
    operands += re.findall(r'(?:<|>|= =) (?:= )?(\w+)', code)
    limits = {int(v.replace('_', '')) if v.replace('_', '').isdigit() else bindings.get(v)
              for v in operands}
    return {v for v in limits if v is not None and v >= 2}, messages


def policy_matches(local, upstream):
    reference = {p: policy_features(s) for p, s in upstream.items()}
    matches = []
    for path, source in local.items():
        numbers, messages = policy_features(source)
        for other, (other_numbers, other_messages) in reference.items():
            shared_messages = sorted(messages & other_messages)
            def scopes(items):
                result = set()
                for message in items:
                    if re.search(r'omit|truncat', message, re.I):
                        result.add('local' if '{}' in message else 'global')
                return result
            shared_scopes = scopes(messages) & scopes(other_messages)
            shared_numbers = sorted(numbers & other_numbers)
            common_omission = any(re.search(r'omit|truncat', m, re.I) for m in shared_messages)
            if common_omission and shared_scopes == {'local', 'global'} and len(shared_numbers) >= 2:
                matches.append(dict(file=path, oracle_file=other,
                                    shared_messages=shared_messages, shared_numbers=shared_numbers,
                                    omission_scopes=sorted(shared_scopes),
                                    reason='shared output policy across function boundaries'))
    return matches


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', type=Path, default=ROOT/'target/rtk-parity/source.tar.gz')
    parser.add_argument('--source-root', type=Path, default=ROOT/'src')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    upstream, digest = oracle_sources(args.archive)
    local = {str(p.relative_to(args.source_root)): p.read_text() for p in args.source_root.rglob('*.rs')}
    matches = compare(local, upstream)
    policies = policy_matches(local, upstream)
    small = small_functions(local, upstream)
    result = dict(small_function_violations=small, small_function_threshold=SHORT_LIMIT, policy_violations=policies, source_sha256=digest,
                  local_sha256={p: hashlib.sha256(s.encode()).hexdigest() for p, s in sorted(local.items())},
                  local_files=len(local), oracle_files=len(upstream),
                  window=WINDOW, rejection_threshold=LIMIT, matches=matches,
                  violations=[m for m in matches if m['tokens'] >= LIMIT])
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2) + '\n')
    print(f"similarity: {len(local)} local / {len(upstream)} oracle files; {len(matches)} audit matches; {len(result['violations'])} violations")
    for match in result['violations']:
        print(json.dumps(match))
    for match in small:
        print("small function:", json.dumps(match))
    for match in policies:
        print('policy:', json.dumps(match))
    print(f'policy violations: {len(policies)}; small-function violations: {len(small)}')
    return bool(result['violations'] or policies or small)

if __name__ == '__main__':
    raise SystemExit(main())
