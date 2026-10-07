#!/usr/bin/env python3
"""Run actual failing tools in an isolated fixture copy, synchronously.

Requires npm, cargo, git, grep, ls, Docker and pytest in this Python environment.
Never overwrites a capture directory. Docker uses a foreground disposable
container and an already installed, digest-pinned public image.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
IMAGE = 'busybox@sha256:dc2d74b28e4cf8984fa52af1f39bc7c3d9c73760b41a74d629f5d11b1ab28616'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--work', type=Path, required=True)
    args = parser.parse_args()
    work = args.work.resolve()
    work.mkdir(parents=True, exist_ok=False)
    fixture = work / 'project'
    shutil.copytree(HERE / 'visible', fixture, ignore=shutil.ignore_patterns('results', 'node_modules'))
    env = dict(os.environ, NO_COLOR='1', FORCE_COLOR='0', CI='1',
               CARGO_TARGET_DIR=str(work / 'cargo-target'),
               npm_config_cache=str(work / 'npm-cache'),
               PYTEST_DISABLE_PLUGIN_AUTOLOAD='1', BUILDX_CONFIG=str(work / 'buildx'))
    with (work / 'install.log').open('wb') as log:
        subprocess.run(['npm', 'ci', '--ignore-scripts', '--no-audit', '--no-fund'], cwd=fixture,
                       env=env, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=600)
    env['PATH'] = str(fixture / 'node_modules/.bin') + os.pathsep + env['PATH']
    records = []

    def record(ident, kind, command, expected, actual=None, extra=None):
        start = time.perf_counter()
        p = subprocess.run(actual or command, cwd=fixture, env=env, capture_output=True, timeout=120)
        elapsed = time.perf_counter() - start
        if p.returncode != expected:
            raise RuntimeError(f'{ident}: expected {expected}, got {p.returncode}: {p.stderr[-2000:]!r}')
        evidence = (p.stdout + p.stderr).decode('utf-8')
        expected_facts = {
            'tsc': ['numbers.ts(179,7)', 'numbers.ts(181,20)', 'error TS2322', 'error TS2304'],
            'eslint': ['missing_first_identifier', 'missing_last_identifier', '3 problems'],
            'jest': ['3 failed, 1 passed', 'rejects the wrong invoice total', 'keeps the missing identifier'],
            'pytest': ['2 failed, 1 passed', 'test_invoice_total', 'test_config_path'],
            'cargo': ['2 failed', 'config_path', 'invoice_total'],
            'docker-build': ["cat: can't open '/missing_build_first'", "cat: can't open '/missing_build_last'"],
        }.get(ident, [])
        # Checking the intended diagnostics prevents treating setup failures as tests.
        for fact in expected_facts:
            if fact not in evidence:
                raise RuntimeError(f'{ident}: intended fact absent: {fact}')
        if ident == 'docker-logs':
            assert evidence.count("cat: can't open '/missing_database'") == 2
        case = dict(id='visible-'+ident, repo='visible-fixture', kind=kind, command=command,
                    exit_code=p.returncode, command_seconds=elapsed,
                    stdout_sha256=hashlib.sha256(p.stdout).hexdigest(),
                    stderr_sha256=hashlib.sha256(p.stderr).hexdigest(), **(extra or {}))
        dest = work / 'captures' / case['id']
        dest.mkdir(parents=True)
        (dest / 'stdout').write_bytes(p.stdout)
        (dest / 'stderr').write_bytes(p.stderr)
        (dest / 'meta.json').write_text(json.dumps(case, indent=2)+'\n')
        records.append(case)
        print(case['id'], p.returncode, len(p.stdout)+len(p.stderr), flush=True)

    record('tsc', 'diagnostic', ['tsc', '--noEmit', '--pretty', 'false', 'numbers.ts'], 2)
    record('eslint', 'diagnostic', ['eslint', '--no-color', 'lint.js'], 1)
    record('jest', 'diagnostic', ['npm', 'test', '--', '--runInBand'], 1)
    record('pytest', 'test', ['pytest', '-q', '--tb=long', 'test_failures.py'], 1,
           [sys.executable, '-m', 'pytest', '-q', '--tb=long', 'test_failures.py'])
    record('cargo', 'test', ['cargo', 'test', '--color', 'never'], 101)
    record('grep-lines', 'grep', ['grep', '-n', 'const ', 'numbers.ts'], 0)
    (fixture / 'invoice-12030.dat').write_bytes(b'x'*12030)
    record('ls-size', 'ls', ['ls', '-ln', 'invoice-12030.dat'], 0)
    # Cat source is intended for editing: whitespace, paths and identifiers matter.
    (fixture / 'source.py').write_text('HOME_CFG = "~/.config/myapp/credentials.json"\n'+
        ''.join(f'def adjust_{i}(ledger_reconciliation_context):\n    return ~ledger_reconciliation_context.permission_bitmask_value + {i}\n' for i in range(100)))
    record('cat-source', 'cat', ['cat', 'source.py'], 0)
    git_env = dict(env, GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM='1')
    def git(*args, **overrides):
        return subprocess.run(['git', *args], cwd=fixture, env=dict(git_env, **overrides),
                              capture_output=True, check=True)
    git('init', '-q')
    git('config', 'user.name', 'Bench Committer')
    git('config', 'user.email', 'committer@example.invalid')
    for n in range(12):
        (fixture / 'commit.txt').write_text(str(n)+'\n')
        git('add', 'commit.txt')
        author = ['Alice', 'Bob', 'Carol'][n % 3]
        date = f'2026-01-{n+1:02}T12:00:00+0000'
        git('commit', '-q', '-m', f'Change {n}\n\nBody keeps identifier_{n} and ~/.config/app.',
            GIT_AUTHOR_NAME=author, GIT_AUTHOR_EMAIL=author.lower()+'@example.invalid',
            GIT_AUTHOR_DATE=date, GIT_COMMITTER_DATE=date)
    record('git-authors', 'log', ['git', 'log', '-12'], 0)
    # Actual OS failures, not echo-generated diagnostic strings.
    name = 'lmr-visible-'+str(os.getpid())
    try:
        producer = subprocess.run(['docker', 'run', '--init', '--name', name, '--network', 'none', '--pull', 'never',
            IMAGE, 'sh', '-c', 'cat /missing_database; printf "checkpoint\\n"; cat /missing_database; kill -KILL $$'],
            capture_output=True, timeout=60)
        code = subprocess.check_output(['docker', 'inspect', '--format', '{{.State.ExitCode}}', name], text=True).strip()
        assert producer.returncode == 137 and code == '137', (producer.returncode, code, producer.stderr)
        record('docker-logs', 'ordered', ['docker', 'logs', '<fixture-container>'], 0,
               ['docker', 'logs', name], extra={'producer_exit_code': 137, 'image': IMAGE})
    finally:
        subprocess.run(['docker', 'rm', '-f', name], capture_output=True, timeout=30)
    (fixture / 'Dockerfile').write_text(f'FROM {IMAGE}\nRUN cat /missing_build_first; cat /missing_build_last\n')
    (fixture / '.dockerignore').write_text('*\n!Dockerfile\n')
    record('docker-build', 'ordered', ['docker', 'build', '--network=none', '--pull=false', '--progress=plain', '.'], 1,
           extra={'image': IMAGE})
    versions = {}
    for name, cmd in [('node', ['node', '--version']), ('npm', ['npm', '--version']), ('cargo', ['cargo', '--version']),
                      ('pytest', [sys.executable, '-m', 'pytest', '--version']), ('docker', ['docker', '--version'])]:
        versions[name] = subprocess.check_output(cmd, env=env, text=True).strip()
    (work / 'environment.json').write_text(json.dumps(versions, indent=2)+'\n')
    (work / 'captures.json').write_text(json.dumps(records, indent=2)+'\n')


if __name__ == '__main__':
    main()
