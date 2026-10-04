#!/usr/bin/env python3
"""Replay the four launch gaps through actual exec, including tee token cost."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tiktoken

ROOT = Path(__file__).resolve().parents[2]
p = argparse.ArgumentParser(description=__doc__)
for name in ('binary', 'before', 'rtk', 'headroom-python', 'work'):
    p.add_argument('--'+name, type=Path, required=True)
a = p.parse_args()
work = a.work.resolve()
work.mkdir(parents=True, exist_ok=False)
receipt = json.loads((ROOT/'target/rtk-parity/oracle-build/receipt.json').read_text())
assert hashlib.sha256(a.rtk.read_bytes()).hexdigest() == receipt['binary_sha256']
assert receipt['source_archive_sha256'] == json.loads((ROOT/'bench/rtk-parity/reference.json').read_text())['source_archive_sha256']
enc = tiktoken.get_encoding('o200k_base')
tokens = lambda text: len(enc.encode(text, disallowed_special=()))
env = dict(os.environ, HOME=str(work/'home'), XDG_CONFIG_HOME=str(work/'config'),
           XDG_DATA_HOME=str(work/'data'), LM_RESIZER_STATE_DIR=str(work/'state'),
           LM_RESIZER_STORE=str(work/'ccr.sqlite'), LM_RESIZER_TRACKING='0',
           RTK_TELEMETRY_DISABLED='1', RTK_SUPPRESS_HOOK_WARNING='1')
def missing(case, text):
    facts = case['oracle']
    if case['id'] == 'dotnet_ok':
        return [] if re.search(r'(?i)(?:passed\s*:\s*65|65\s+tests\s+passed|65\s+passed)', text) else facts
    return [fact for fact in facts if fact not in text]
rows = []
for case in json.loads((ROOT/'bench/cases.json').read_text()):
    if case['id'] not in ('cargo_ok', 'dotnet_ok', 'git_diff', 'compile_error'):
        continue
    raw = (ROOT/'bench'/case['file']).read_bytes()
    folder = work/case['id']
    folder.mkdir()
    (folder/'raw').write_bytes(raw)
    shim = folder/'bin'
    shim.mkdir()
    command = case['command'].split()
    program = shim/command[0]
    code = case.get('exit_code', 0)
    program.write_text('#!/bin/sh\n/bin/cat "$LAUNCH_CAPTURE"\nexit "$LAUNCH_EXIT"\n')
    program.chmod(0o755)
    local = dict(env, PATH=str(shim)+os.pathsep+env['PATH'], LAUNCH_CAPTURE=str(folder/'raw'), LAUNCH_EXIT=str(code))
    row = dict(case=case['id'], raw_tokens=tokens(raw.decode()), tools={})
    for label, binary in [('before', a.before), ('lm', a.binary), ('rtk', a.rtk), ('headroom', a.headroom_python)]:
        data = None
        if label in ('before', 'lm'):
            argv = [binary.resolve(), 'exec', '--json', '--', *command]
        elif label == 'rtk':
            argv = [binary.resolve(), *(['pipe', '--filter', case['rtk_filter']] if case['rtk_filter'] else command)]
            if case['rtk_filter']:
                data = raw
        else:
            argv = ['/bin/sh', ROOT/'bench/headroom_once.sh', binary.absolute()]
            data = raw
        result = subprocess.run(argv, input=data, capture_output=True, cwd=folder, env=local, timeout=120)
        exact = None
        if label in ('before', 'lm'):
            report = json.loads(result.stdout)
            text = report['output']
            assert result.returncode == code
            key = re.search(r'[a-f0-9]{12}', report['tee_hint'])[0]
            recovered = subprocess.run([binary.resolve(), 'tee', 'read', key], capture_output=True, env=local, check=True).stdout
            exact = recovered == raw
            assert exact
        else:
            text = result.stdout.decode()
            if label == 'headroom':
                assert result.returncode == 0, result.stderr
        (folder/(label+'.txt')).write_text(text)
        row['tools'][label] = dict(tokens=tokens(text), missing=missing(case, text),
                                  exit=result.returncode, tee_exact=exact)
    rows.append(row)
result = dict(cases=rows, oracle={k: receipt[k] for k in ("version", "binary_sha256", "source_archive_sha256") if k in receipt}, binary_sha256=hashlib.sha256(a.binary.read_bytes()).hexdigest(),
              before_sha256=hashlib.sha256(a.before.read_bytes()).hexdigest(), tokenizer='o200k_base')
(work/'results.json').write_text(json.dumps(result, indent=2)+'\n')
print(json.dumps(rows, indent=2))
