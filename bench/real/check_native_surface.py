#!/usr/bin/env python3
"""Publication barrier: competitor code and labels must not enter the product."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess


def check(binary):
    binary = str(Path(binary).resolve())
    strings = subprocess.check_output(['strings', binary])
    assert not any(name in strings.lower() for name in [b'rtk', b'headroom']), 'forbidden name in published binary strings'
    tree = subprocess.check_output(['cargo', 'tree', '-p', 'lm-resizer', '--locked', '--prefix', 'none'], text=True)
    packages = [line.split()[0].lower() for line in tree.splitlines() if line.split()]
    assert not any(any(name in package for package in packages) for name in ['rtk', 'headroom']), 'forbidden product dependency'
    queue = [()]
    checked = []
    while queue:
        command = queue.pop(0)
        result = subprocess.run([binary, *command, '--help'], capture_output=True, check=True)
        help_text = (result.stdout + result.stderr).decode()
        assert not any(name in help_text.lower() for name in ['rtk', 'headroom']), f'forbidden name in help: {command}'
        checked.append(' '.join(command) or '<root>')
        if 'Commands:\n' in help_text:
            section = help_text.split('Commands:\n', 1)[1].split('\n\n', 1)[0]
            for line in section.splitlines():
                match = re.match(r'^  ([a-z][a-z0-9-]+)\s{2,}', line)
                if match and match[1] != 'help':
                    queue.append((*command, match[1]))
    print(json.dumps(dict(binary_sha256=hashlib.sha256(Path(binary).read_bytes()).hexdigest(), binary_strings_clean=True, dependency_tree_clean=True,
                          help_commands=checked), indent=2))

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary')
    check(parser.parse_args().binary)
