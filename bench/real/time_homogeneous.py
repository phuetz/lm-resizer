#!/usr/bin/env python3
"""Measure pathological single-line BPE; require literal output, bounded runtime."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--work', type=Path, required=True)
args = parser.parse_args()
args.work.mkdir(parents=True, exist_ok=False)
binary = args.binary.resolve()
env = dict(os.environ, LM_RESIZER_STATE_DIR=str(args.work / 'state'))
rows = []
for size in [250_000, 500_000, 1_000_000, 1_048_576, 20_000_000]:
    raw = b'x'*size
    source = args.work / f'x-{size}.txt'
    source.write_bytes(raw)
    for mode in ['tool-output', 'exec']:
        command = ([str(binary), 'tool-output', '--command', 'cat', '--input', str(source), '--json']
                   if mode == 'tool-output' else [str(binary), 'exec', '--json', '--', 'cat', str(source)])
        started = time.perf_counter()
        result = subprocess.run(command, env=env, capture_output=True, timeout=60, check=True)
        elapsed = time.perf_counter()-started
        report = json.loads(result.stdout)
        assert report['output'].encode() == raw
        assert report['original_tokens'] == report['compressed_tokens']
        rows.append(dict(bytes=size, mode=mode, seconds=elapsed, tokens=report['original_tokens'],
                         literal_view=True, raw_sha256=hashlib.sha256(raw).hexdigest()))
        print(size, mode, round(elapsed, 3), flush=True)
(args.work / 'results.json').write_text(json.dumps(dict(
    binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
    scope='single sequential sample per size and mode; cold CLI; OS cache not flushed',
    samples=rows), indent=2)+'\n')
