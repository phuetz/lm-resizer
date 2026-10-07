#!/usr/bin/env python3
"""Time actual ls -R and its wrapper separately, with exact output recovery."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--repo', type=Path, required=True)
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--runs', type=int, default=5)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=False)
args.binary = args.binary.resolve()
env = dict(os.environ, LM_RESIZER_STATE_DIR=str(args.output.resolve()/'state'), LC_ALL='C')
rows = []
for _ in range(args.runs):
    started = time.perf_counter()
    raw = subprocess.run(['ls', '-R'], cwd=args.repo, capture_output=True, env=env, check=True)
    native = time.perf_counter() - started
    started = time.perf_counter()
    wrapped = subprocess.run([str(args.binary), 'exec', '--json', '--', 'ls', '-R'],
                             cwd=args.repo, capture_output=True, env=env, check=True)
    total = time.perf_counter() - started
    report = json.loads(wrapped.stdout)
    assert not raw.stderr and report['output'].encode() == raw.stdout
    rows.append(dict(native_seconds=native, wrapped_seconds=total,
                     difference_seconds=total-native, raw_bytes=len(raw.stdout),
                     raw_sha256=hashlib.sha256(raw.stdout).hexdigest(), exact=True))
result = dict(binary_sha256=hashlib.sha256(args.binary.read_bytes()).hexdigest(),
              repo_commit=subprocess.check_output(['git','-C',str(args.repo),'rev-parse','HEAD'],text=True).strip(),
              scope='actual directory walk; sequential native/wrapped pairs; OS cache not flushed', samples=rows)
(args.output/'results.json').write_text(json.dumps(result, indent=2)+'\n')
print('actual ls -R:', len(rows), 'exact recoveries;', 'max wrapped', max(r['wrapped_seconds'] for r in rows))
