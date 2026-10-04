#!/usr/bin/env python3
"""Bind measurements to the executable reported by Cargo, never a guessed path."""
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT)


def build_executable(root, name):
    command = ['cargo', 'build', '--release', '--locked', '--bin', name,
               '--message-format=json-render-diagnostics']
    result = subprocess.run(command, cwd=root, stdout=subprocess.PIPE, check=True, text=True)
    artifacts = []
    for line in result.stdout.splitlines():
        message = json.loads(line)
        if (message.get('reason') == 'compiler-artifact'
                and message.get('target', {}).get('name') == name
                and 'bin' in message['target']['kind'] and message.get('executable')):
            artifacts.append(message)
    if len(artifacts) != 1:
        raise RuntimeError(f'expected exactly one Cargo executable for {name}, got {len(artifacts)}')
    artifact = artifacts[0]
    executable = Path(artifact['executable']).resolve(strict=True)
    if not executable.is_file():
        raise RuntimeError('Cargo executable is not a file')
    return executable, command, artifact


def verify_build_proof(binary, proof):
    if (proof.get('cargo_artifact', {}).get('target') != 'lm-resizer'
            or '--message-format=json-render-diagnostics' not in proof.get('build_command', [])):
        raise RuntimeError('legacy or missing Cargo artifact proof: rebuild with bench/real/build.py')
    if proof.get('binary_sha256') != hashlib.sha256(binary.read_bytes()).hexdigest():
        raise RuntimeError('build proof binary mismatch')


if __name__ == '__main__':
    paths = git('ls-files', '-z', 'src', 'crates', 'Cargo.toml', 'Cargo.lock',
                '.cargo', 'rust-toolchain.toml').decode().split('\0')
    sources = {p: hashlib.sha256((ROOT / p).read_bytes()).hexdigest()
               for p in paths if p and (ROOT / p).is_file()}
    binary, command, artifact = build_executable(ROOT, 'lm-resizer')
    # Portable public metadata; the executable itself is opened at Cargo's exact path.
    try:
        layout = str(binary.relative_to(ROOT))
    except ValueError:
        layout = '<external-target>/' + '/'.join(binary.parts[-2:])
    proof = dict(source_commit=git('rev-parse', 'HEAD').decode().strip(),
                 source_files=sources,
                 source_tree_clean=not bool(git('status', '--porcelain', '--', *[p for p in paths if p])),
                 build_command=command,
                 cargo_artifact=dict(executable=layout, target=artifact['target']['name'],
                                     profile=artifact['profile'], fresh=artifact['fresh']),
                 rustc=subprocess.check_output(['rustc', '-Vv'], text=True))
    if any(hashlib.sha256((ROOT / p).read_bytes()).hexdigest() != digest for p, digest in sources.items()):
        raise RuntimeError('sources changed during compilation')
    proof['binary_sha256'] = hashlib.sha256(binary.read_bytes()).hexdigest()
    binary.with_suffix('.build.json').write_text(json.dumps(proof, indent=2)+'\n')
    print(binary)
