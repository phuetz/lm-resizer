#!/usr/bin/env python3
"""Build the pinned comparison executable outside the product workspace."""
import hashlib
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile
import urllib.request

root = Path(__file__).resolve().parents[2]
pin = json.loads((root / 'bench/rtk-parity/reference.json').read_text())
work = root / 'target/rtk-parity'
work.mkdir(parents=True, exist_ok=True)
archive = work / 'source.tar.gz'
if not archive.exists():
    urllib.request.urlretrieve('https://api.github.com/repos/rtk-ai/rtk/tarball/' + pin['commit'], archive)
assert hashlib.sha256(archive.read_bytes()).hexdigest() == pin['source_archive_sha256'], 'source checksum mismatch'
with tempfile.TemporaryDirectory(prefix='lmr-comparison-source-') as directory:
    with tarfile.open(archive) as tar:
        tar.extractall(directory, filter='data')
    source = next(Path(directory).iterdir())
    # Bind the receipt to the actual build input, not a separate checkout.
    with tarfile.open(archive) as tar:
        expected = {str(Path(m.name).relative_to(Path(m.name).parts[0])):
                    hashlib.sha256(tar.extractfile(m).read()).hexdigest()
                    for m in tar.getmembers() if m.isfile()}
    def verify_source():
        actual = {str(p.relative_to(source)): hashlib.sha256(p.read_bytes()).hexdigest()
                  for p in source.rglob('*') if p.is_file()}
        assert actual == expected, 'build source differs from pinned archive'
    verify_source()
    subprocess.run(['cargo', 'build', '--locked', '--release', '--manifest-path', str(source / 'Cargo.toml'),
                    '--target-dir', str(work / 'oracle-build')], check=True)
    verify_source()
binary = work / 'oracle-build/release/rtk'
version = subprocess.check_output([binary, '--version'], text=True).strip()
assert version == 'rtk ' + pin['version']
receipt = dict(source_files_verified=len(expected), source_tree_matches_archive=True, version=version, source_archive_sha256=pin['source_archive_sha256'],
               binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
               build='cargo build --locked --release; unmodified upstream source')
(work / 'oracle-build/receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt, indent=2))
