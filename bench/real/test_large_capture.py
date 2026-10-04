#!/usr/bin/env python3
"""Exercise the published CLI with interleaved streams beyond the former cap."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--binary', type=Path, default=Path('target/release/lm-resizer'))
p.add_argument('--output', type=Path)
a = p.parse_args()
binary = a.binary.resolve()
stdout = b'out ping\n' * 512
stderr = b'err pong\n' * 512
expected = b'commit fixture\n' + (stdout + stderr) * 1200 + b'tail sentinel\n'
assert len(expected) > 10 * 1024 * 1024
with tempfile.TemporaryDirectory(prefix='lmr-large-capture-') as folder:
    root = Path(folder)
    shim = root/'git'
    shim.write_text('#!' + sys.executable + '\n' + '''import os

def write(fd, data):
    while data:
        data = data[os.write(fd, data):]

write(1, b'commit fixture\\n')
for _ in range(1200):
    write(1, b'out ping\\n' * 512)
    write(2, b'err pong\\n' * 512)
write(1, b'tail sentinel\\n')
raise SystemExit(7)
''')
    shim.chmod(0o755)
    env = dict(os.environ, PATH=str(root)+os.pathsep+os.environ['PATH'],
               HOME=str(root), XDG_CONFIG_HOME=str(root/'config'),
               LM_RESIZER_STATE_DIR=str(root/'state'), LM_RESIZER_STORE=str(root/'store.sqlite'),
               LM_RESIZER_TRACKING='0')
    result = subprocess.run([binary, 'git', 'log'], env=env, capture_output=True, timeout=120)
    assert result.returncode == 7, result.stderr.decode(errors='replace')
    key = hashlib.sha256(expected).hexdigest()
    recovered = subprocess.run([binary, 'tee', 'read', key], env=env, capture_output=True, check=True, timeout=120)
    assert recovered.stdout == expected, 'raw archive differs from interleaved producer bytes'
    proof = dict(binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                 producer_bytes=len(expected), producer_sha256=key, producer_exit=7,
                 lm_exit=result.returncode, tee_exact=True,
                 stdout_and_stderr_interleaved=True, tail_beyond_ten_mib=True)
    if a.output:
        a.output.write_text(json.dumps(proof, indent=2)+'\n')
    print(json.dumps(proof, indent=2))
