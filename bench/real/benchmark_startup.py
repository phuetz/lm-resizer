#!/usr/bin/env python3
"""Interleaved fresh-process timings on the identical 61-case replay corpus."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--before', type=Path, required=True)
p.add_argument('--after', type=Path, required=True)
p.add_argument('--rtk', type=Path, required=True)
p.add_argument('--work', type=Path, required=True)
p.add_argument('--allow-view-changes', action='store_true', help='Migration only: original counts still must match')
p.add_argument('--built-oracle', type=Path)
p.add_argument('--repetitions', type=int, default=7)
a = p.parse_args()
work = a.work.resolve(); work.mkdir(parents=True, exist_ok=False)
binaries = {k: getattr(a, k).resolve() for k in ('before', 'after', 'rtk')}
pin = json.loads((ROOT/'bench/rtk-parity/reference.json').read_text())
if a.built_oracle:
    receipt = json.loads(a.built_oracle.read_text())
    assert receipt['source_archive_sha256'] == pin['source_archive_sha256']
    pin = receipt
assert hashlib.sha256(binaries['rtk'].read_bytes()).hexdigest() == pin['binary_sha256']
(work/"home").mkdir()
env = dict(os.environ, HOME=str(work/"home"), LM_RESIZER_TRACKING='0', RTK_TELEMETRY_DISABLED='1',
           RTK_SUPPRESS_HOOK_WARNING='1', LM_RESIZER_STORE=str(work/'ccr.sqlite'),
           LM_RESIZER_STATE_DIR=str(work/'state'), XDG_CONFIG_HOME=str(work/'config'),
           XDG_DATA_HOME=str(work/'data'))
rows = []
for case in json.loads(gzip.decompress((ROOT/'bench/rtk-parity/corpus.json.gz').read_bytes())):
    folder = work/case['id']; folder.mkdir()
    local = dict(env)
    if case['filter']:
        raw = case['stdout'] + (('\n[stderr]\n' if case['stdout'] else '[stderr]\n') + case['stderr'] if case['stderr'] else '')
        data = raw.encode()
        ref_args = ['pipe', '--filter', case['filter']]
        lm_args = ref_args + ['--json', '--exit-code', str(case['exit_code'])]
    else:
        data = None
        for stream in ('stdout', 'stderr'): (folder/stream).write_text(case[stream])
        shim = folder/'bin'; shim.mkdir()
        for program in {case['command'][0], 'npx', 'npm', 'pnpm', 'yarn'}:
            path = shim/program
            path.write_text('#!/bin/sh\n/bin/cat "$PARITY_CAPTURE/stdout"\n/bin/cat "$PARITY_CAPTURE/stderr" >&2\nexit "$PARITY_EXIT"\n'); path.chmod(0o755)
        local.update(PATH=str(shim)+os.pathsep+env['PATH'], PARITY_CAPTURE=str(folder), PARITY_EXIT=str(case['exit_code']))
        command = ['cat', 'stdout'] if case['command'][0] == 'cat' else case['command']
        ref_args = ['read', 'stdout'] if command[0] == 'cat' else ['lint', *command] if command[0] == 'eslint' else command
        lm_args = ['exec', '--json', '--', *command]
    samples = {k: [] for k in binaries}
    counts = {k: [] for k in ('before', 'after')}
    for i in range(a.repetitions):
        order = list(binaries); order = order[i % 3:] + order[:i % 3]
        for label in order:
            start = time.perf_counter()
            result = subprocess.run([binaries[label], *(ref_args if label == 'rtk' else lm_args)], input=data, capture_output=True, env=local, cwd=folder, timeout=120)
            samples[label].append((time.perf_counter()-start)*1000)
            if label != 'rtk':
                report = json.loads(result.stdout)
                assert result.returncode == case['exit_code']
                counts[label].append((report['original_tokens'], report['compressed_tokens']))
    if a.allow_view_changes:
        assert [n[0] for n in counts['before']] == [n[0] for n in counts['after']], (case['id'], counts)
    else:
        assert counts['before'] == counts['after'], (case['id'], counts)
    rows.append(dict(id=case['id'], samples_ms=samples, median_ms={k: statistics.median(v) for k,v in samples.items()}, exact_counts_unchanged=counts['before'] == counts['after'], token_counts=counts))
    print(case['id'], rows[-1]['median_ms'], flush=True)
# Separate startup and tokenizer diagnostics; these aren't mixed into wall timings.
profiles = {}
for label in ('before', 'after'):
    result = subprocess.run([binaries[label], 'pipe', '--filter', 'pytest', '--json'], input=b'test session starts\n', capture_output=True, env=dict(env, LM_RESIZER_PROFILE='1'))
    profiles[label] = result.stderr.decode()
result = dict(platform=platform.system()+' '+platform.release()+' '+platform.machine(), logical_cpus=os.cpu_count(),
              repetitions=a.repetitions, order='rotating before/after/RTK; fresh processes, same machine and captures',
              sha256={k: hashlib.sha256(v.read_bytes()).hexdigest() for k,v in binaries.items()}, profiles=profiles,
              summary_ms={k: dict(median=statistics.median(r['median_ms'][k] for r in rows), mean=statistics.mean(r['median_ms'][k] for r in rows)) for k in binaries}, cases=rows)
(work/'results.json').write_text(json.dumps(result, indent=2)+'\n')
print(json.dumps(result['summary_ms'], indent=2))
