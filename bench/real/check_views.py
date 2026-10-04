#!/usr/bin/env python3
"""Supplementary fixture/fuzz checks, excluded from real-command medians."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import subprocess

from run import metrics, ENCODINGS

ROOT = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.binary = args.binary.resolve()
args.output.mkdir(parents=True, exist_ok=False)
env = dict(os.environ, LM_RESIZER_STATE_DIR=str(args.output.resolve() / 'state'))
cases = []
for fixture in json.loads((ROOT / 'fixtures/parity/manifest.json').read_text()):
    if fixture.get('command', '').split(' ')[0] in ('docker', 'tsc', 'eslint', 'jest', 'vitest'):
        cases.append((fixture['name'], fixture['command'],
                      (ROOT / fixture['path']).read_bytes().decode(), fixture.get('exit_code', 0),
                      'versioned fixture; not a newly executed tool'))
for seed in range(80):
    rng = random.Random(seed)
    atoms = ['@literal', '&0:999', '=42', '!0', '~a~~', '\\escape', 'é漢字',
             '"quoted"', '', ' leading and trailing ', '[stderr]', 'error: expected 2, actual 3']
    if seed % 2:
        rows = [str(rng.getrandbits(64)) + ' See https://example.invalid/long/repeated/documentation/path/' + str(rng.randrange(20))
                + ' veryLongRepeatedIdentifierName ' + rng.choice(atoms) for _ in range(160)]
    else:
        rows = ['directory/long/shared/prefix/' + str(rng.randrange(4)) + '/file:'
                + rng.choice(atoms) for _ in range(160)]
        rows *= 3
    ending = ['\n', '\r\n'][seed % 3 == 0]
    raw = ending.join(rows) + (ending if seed % 4 else '')
    cases.append((f'seed-{seed}', 'cat input.txt', raw, 0, 'deterministic adversarial fixture'))
results = []
for name, command, raw, code, origin in cases:
    process = subprocess.run([str(args.binary), 'tool-output', '--json', '--command', command,
                              '--exit-code', str(code)], input=raw.encode(), capture_output=True,
                             env=env, timeout=60)
    report = json.loads(process.stdout)
    view = report['output']
    assert view == raw, name + ': literal visible content changed'
    assert report['exit_code'] == code, name + ': exit code mismatch'
    assert report['original_tokens'] == len(ENCODINGS['o200k_base'].encode(raw, disallowed_special=()))
    assert report['compressed_tokens'] == len(ENCODINGS['o200k_base'].encode(view, disallowed_special=()))
    recovered = None
    if report.get('tee_hint'):
        digest = hashlib.sha256(raw.encode()).hexdigest()
        recovered = (args.output / 'state/tee' / (digest + '.log')).read_bytes() == raw.encode()
        assert recovered, name + ': tee mismatch'
    results.append(dict(name=name, origin=origin, command=command, exact=True,
                        exit_code=code, tee_verified=recovered, tokens=metrics(raw, view),
                        representation=view.split('\n')[0] if view.startswith('LMR-') else 'raw'))
    (args.output / (name+'.view')).write_bytes(view.encode())
artifact = dict(binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                scope='supplementary fixtures, excluded from the 30 real-command cases', cases=results)
(args.output / 'results.json').write_text(json.dumps(artifact, indent=2, ensure_ascii=False)+'\n')
print(f'{len(results)} literal visible views; representations:', sorted({r['representation'] for r in results}))
