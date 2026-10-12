#!/usr/bin/env python3
"""Portable replay of Windows byte contracts; does not claim a Windows host."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
import tempfile

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--binary', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
a = p.parse_args()
binary = a.binary.resolve()
results = []
with tempfile.TemporaryDirectory(prefix='windows contract été ') as folder:
    root = Path(folder)
    env = dict(os.environ, LM_RESIZER_STATE_DIR=str(root/'state'),
               LM_RESIZER_STORE=str(root/'ccr.sqlite'), LM_RESIZER_TRACKING='0')
    def run(args, data=None, code=0):
        r = subprocess.run([binary, *args], input=data, capture_output=True, env=env, cwd=root)
        assert r.returncode == code, (args, r.stderr)
        return r.stdout
    emitter = root/'producteur été.py'
    emitter.write_text('import pathlib,sys\nsys.stdout.buffer.write(pathlib.Path(sys.argv[1]).read_bytes())\nsys.exit(int(sys.argv[2]))\n')
    journal = ''.join(f'INFO événement {i} C:\\projet été\\résumé.txt\r\n' for i in range(500)) + 'ERROR sentinelle finale\r\n'
    for name, raw, code, flags in [
        ('oem850', 'fichier-été.txt\r\n'.encode('cp850'), 0, []),
        ('crlf', b'line 1\r\nline 2\r\n', 0, []),
        ('long-failure', journal.encode(), 23, ['--raw-on-failure']),
        ('long-success', journal.encode(), 0, []),
    ]:
        # Since the generic views, a failing producer's view starts with one status line.
        shown = (f'[FAIL] Command failed (exit code: {code})\n'.encode() + raw) if code else raw
        source = root/'résumé avec espaces.bin'
        source.write_bytes(raw)
        report = json.loads(run(['exec', '--json', *flags, '--', sys.executable, str(emitter), str(source), str(code)], code=code))
        assert report['original_bytes'] == len(raw)
        hint = report['tee_hint']
        if hint:
            key = re.search(r'[0-9a-f]{12}', hint)[0]
            assert run(['tee', 'read', key]) == raw, name
        else:
            assert report['output'].encode() == shown, name
        assert '\ufffd' not in report['output']
        if flags:
            assert report['output'].encode() == shown
            assert not report['compression_steps']
        results.append(dict(case=name, bytes=len(raw), exit=code, recovered_exact=True))
    source = root/'journal.txt'
    source.write_bytes(journal.encode())
    report = json.loads(run(['compress', '--json', '--input', str(source)]))
    keys = set(report['cache_keys']) | set(re.findall(r'hash=([a-f0-9]{24})', report['output']))
    assert keys, 'fixture must exercise CCR offload'
    for key in keys:
        assert run(['retrieve', key]) == journal.encode(), 'intermediate CCR payload'
    results.append(dict(case='displayed-ccr-original', keys=len(keys), recovered_exact=True))
    for text, code, sentinel in [
        ("'mocha' n’est pas reconnu en tant que commande interne\r\n", 1, "'mocha'"),
        ('  111 passing (30ms)\r\n', 0, '111 passing'),
    ]:
        report = json.loads(run(['tool-output', '--json', '--command', 'npm.cmd test', '--exit-code', str(code)], data=text.encode(), code=code))
        assert sentinel in report['output']
        assert 'completed' not in report['output']
        results.append(dict(case='npm-missing' if code else 'npm-passing', diagnostic_retained=True))
result = dict(host=platform.system(), native_windows=platform.system() == 'Windows',
              binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), cases=results)
a.output.write_text(json.dumps(result, indent=2)+'\n')
print(json.dumps(result, indent=2))
