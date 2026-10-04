#!/usr/bin/env python3
"""Generate real Git histories; require compression AND byte-exact expansion."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import tiktoken

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--binary', type=Path, required=True)
p.add_argument('--before', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
a = p.parse_args()
binaries = {k: getattr(a, k).resolve() for k in ('before', 'binary')}
encoder = tiktoken.get_encoding('o200k_base')
count = lambda value: len(encoder.encode(value.decode(), disallowed_special=()))
rows = []
with tempfile.TemporaryDirectory() as tmp:
    root = Path(tmp)
    env = dict(os.environ, GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_GLOBAL='/dev/null',
               LM_RESIZER_TRACKING='0', LM_RESIZER_STORE=str(root/'store.sqlite'),
               LM_RESIZER_STATE_DIR=str(root/'state'),
               GIT_AUTHOR_DATE='2025-01-01T00:00:00Z', GIT_COMMITTER_DATE='2025-01-01T00:00:00Z')
    def run(args, data=None):
        return subprocess.run(args, input=data, cwd=root, env=env,
                              capture_output=True, check=True).stdout
    for shape in ('repeated', 'distinct'):
        root = Path(tmp)/shape
        root.mkdir()
        env.update(LM_RESIZER_STORE=str(root/'store.sqlite'), LM_RESIZER_STATE_DIR=str(root/'state'))
        run(['git', 'init', '-q'])
        run(['git', 'config', 'user.name', 'Fixture User'])
        run(['git', 'config', 'user.email', 'fixture@example.test'])
        path = root/'source with a descriptive name.txt'
        for revision in range(2):
            content = ''.join(f'value {revision}: {i if shape == "distinct" else "same"}\n' for i in range(320))
            path.write_text(content)
            run(['git', 'add', path.name])
            run(['git', 'commit', '-qm', f'{shape} revision {revision}'])
        for command in (["git", "log", "-2", "-p", "--no-color", "--format=medium"],
                        ["git", "show", "--no-color", "--format=medium"]):
            raw = run(command)
            views = {}
            for label, binary in binaries.items():
                report = json.loads(run([binary, 'exec', '--json', '--', *command]))
                views[label] = report['output'].encode()
            expanded = run([binaries['binary'], 'expand'], views['binary'])
            assert expanded == raw, shape
            assert count(views['binary']) < count(raw), shape
            for file in (root/'state'/'tee').glob('*'):
                if file.is_file() and file.read_bytes() == raw:
                    assert run([binaries['binary'], 'tee', 'read', file.name]) == raw
                    break
            else:
                raise AssertionError('missing exact tee')
            rows.append(dict(case=shape, command=command[1], raw_tokens=count(raw),
                             before_tokens=count(views['before']), after_tokens=count(views['binary']),
                             expanded_exact=True, tee_exact=True, raw_sha256=hashlib.sha256(raw).hexdigest()))

result = dict(cases=rows, tokenizer='o200k_base',
              binaries={k: hashlib.sha256(v.read_bytes()).hexdigest() for k, v in binaries.items()})
a.output.write_text(json.dumps(result, indent=2)+'\n')
print(json.dumps(result, indent=2))
