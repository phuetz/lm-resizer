#!/usr/bin/env python3
"""Prove that restoring pre-review decoding breaks all four visibility guards."""
import argparse
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
if args.output.exists():
    raise RuntimeError('refusing to overwrite mutation evidence')
baseline = subprocess.run([sys.executable, '-m', 'unittest', 'discover', '-s', str(HERE),
                           '-p', 'test_oracle.py'], capture_output=True, text=True)
assert baseline.returncode == 0, baseline.stderr
with tempfile.TemporaryDirectory(prefix='lmr-visible-oracle-') as folder:
    destination = Path(folder)
    needle = '    required = facts(case["kind"], raw, case["command"])'
    source = (HERE / 'run.py').read_text()
    assert source.count(needle) == 1
    (destination / 'run.py').write_text(source.replace(needle,
        '    output = output if output == raw else expand(output)\n' + needle))
    shutil.copyfile(HERE / 'test_oracle.py', destination / 'test_oracle.py')
    shutil.copyfile(HERE / 'build.py', destination / 'build.py')
    mutant = subprocess.run([sys.executable, '-m', 'unittest', 'discover', '-s', str(destination),
                             '-p', 'test_oracle.py'], capture_output=True, text=True)
    failures = re.findall(r'^FAIL: (test_B[1-4]_\w+)', mutant.stderr, re.M)
    assert mutant.returncode == 1 and len(set(failures)) == 4, mutant.stderr
result = dict(mutation='restore decoding before fact checks', baseline_exit=baseline.returncode,
              mutant_exit=mutant.returncode, detected_failures=failures,
              interpretation='All four bad views round-trip, but fail the literal-view oracle.')
args.output.write_text(json.dumps(result, indent=2)+'\n')
print(json.dumps(result, indent=2))
