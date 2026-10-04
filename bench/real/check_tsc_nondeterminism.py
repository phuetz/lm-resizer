#!/usr/bin/env python3
"""Do not normalize TSC: compare exact byte variants, including RTK with itself."""
import argparse
from collections import Counter
import gzip
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tiktoken

root = Path(__file__).resolve().parents[2]
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--binary', type=Path, required=True)
p.add_argument('--work', type=Path, required=True)
p.add_argument('--repetitions', type=int, default=64)
a = p.parse_args()
a.work.mkdir(parents=True, exist_ok=False)
lm = a.binary.resolve()
rtk = root / 'target/rtk-parity/bin/rtk'
pin = json.loads((root / 'bench/rtk-parity/reference.json').read_text())
sha = lambda b: hashlib.sha256(b).hexdigest()
assert sha(rtk.read_bytes()) == pin['binary_sha256']
case = next(c for c in json.loads(gzip.decompress((root / 'bench/rtk-parity/corpus.json.gz').read_bytes())) if c['id'] == 'fresh-tsc')
raw = case['stdout'] + (('\n[stderr]\n' if case['stdout'] else '[stderr]\n') + case['stderr'] if case['stderr'] else '')
env = dict(os.environ, RTK_TELEMETRY_DISABLED='1', RTK_SUPPRESS_HOOK_WARNING='1',
           XDG_DATA_HOME=str(a.work / 'data'), XDG_CONFIG_HOME=str(a.work / 'config'),
           LM_RESIZER_STATE_DIR=str(a.work / 'state'), LM_RESIZER_STORE=str(a.work / 'ccr.sqlite'),
           LM_RESIZER_TRACKING='0')
ref, actual, variants = [], [], {}
enc = tiktoken.get_encoding('o200k_base')
for _ in range(a.repetitions):
    r = subprocess.run([rtk, 'pipe', '--filter', 'tsc'], input=raw.encode(), capture_output=True, env=env, timeout=30)
    c = subprocess.run([lm, 'pipe', '--filter', 'tsc', '--json'], input=raw.encode(), capture_output=True, env=env, timeout=30)
    assert r.returncode == c.returncode == 0
    report = json.loads(c.stdout)
    view = report['output'].encode()[:report['filtered_bytes']]
    reference = r.stdout + r.stderr
    ref.append(sha(reference)); actual.append(sha(view))
    for b in (reference, view):
        variants[sha(b)] = dict(text=b.decode(), tokens=len(enc.encode(b.decode(), disallowed_special=())))
source = Path('src/cmds/js/tsc_cmd.rs')
upstream = (root / 'target/rtk-parity/source' / source).read_bytes()
ported = (root / 'vendor/rtk' / source).read_bytes()
assert upstream == ported, 'TSC source differs from the pinned upstream archive'
result = dict(case='fresh-tsc', repetitions=a.repetitions, normalization='none',
    rtk_sha256=sha(rtk.read_bytes()), lm_sha256=sha(lm.read_bytes()),
    tsc_source_identical_to_upstream=True, tsc_source_sha256=sha(ported),
    reference_sequence=ref, lm_sequence=actual,
    reference_variants=dict(Counter(ref)), lm_variants=dict(Counter(actual)),
    reference_self_mismatches=sum(x != y for x, y in zip(ref, ref[1:])),
    paired_mismatches=sum(x != y for x, y in zip(ref, actual)),
    observed_variant_sets_equal=set(ref) == set(actual), variants=variants)
assert result['reference_self_mismatches'] > 0, 'non-determinism not reproduced'
assert result['observed_variant_sets_equal'], 'LM emitted a variant not observed from RTK'
(a.work / 'results.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps({k:v for k,v in result.items() if k not in ('variants','reference_sequence','lm_sequence')}, indent=2))
