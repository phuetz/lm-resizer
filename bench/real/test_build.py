"""Actual Cargo builds: a stale default binary must never get certified."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from build import build_executable, verify_build_proof


class CargoArtifactTests(unittest.TestCase):
    def test_target_env_and_config_ignore_stale_default_executable(self):
        host = next(line.split(': ', 1)[1] for line in subprocess.check_output(
            ['rustc', '-Vv'], text=True).splitlines() if line.startswith('host: '))
        for via_config in (False, True):
            with self.subTest(via_config=via_config), tempfile.TemporaryDirectory() as folder:
                root = Path(folder)
                (root / 'src').mkdir()
                (root / 'Cargo.toml').write_text('[package]\nname="layout"\nversion="0.0.0"\nedition="2021"\n[workspace]\n')
                (root / 'Cargo.lock').write_text('version = 3\n[[package]]\nname = "layout"\nversion = "0.0.0"\n')
                (root / 'src/main.rs').write_text('fn main() { println!("CURRENT-CARGO-ARTIFACT"); }')
                stale = root / 'target/release/layout'
                stale.parent.mkdir(parents=True)
                stale.write_text('STALE-NOT-THE-BUILD')
                env = dict(os.environ)
                for key in ('CARGO_BUILD_TARGET', 'CARGO_TARGET_DIR', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER'):
                    env.pop(key, None)
                if via_config:
                    (root / '.cargo').mkdir()
                    (root / '.cargo/config.toml').write_text(f'[build]\ntarget="{host}"\n')
                else:
                    env['CARGO_BUILD_TARGET'] = host
                with patch.dict(os.environ, env, clear=True):
                    executable, command, artifact = build_executable(root, 'layout')
                self.assertEqual(executable, root / 'target' / host / 'release/layout')
                self.assertEqual(stale.read_text(), 'STALE-NOT-THE-BUILD')
                self.assertEqual(subprocess.check_output([str(executable)], text=True).strip(), 'CURRENT-CARGO-ARTIFACT')
                self.assertIn('--message-format=json-render-diagnostics', command)
                self.assertEqual(artifact['executable'], str(executable))

    def test_legacy_hash_only_proof_cannot_certify_a_build(self):
        import hashlib
        with tempfile.TemporaryDirectory() as folder:
            binary = Path(folder) / 'lm-resizer'
            binary.write_bytes(b'old executable')
            proof = {'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest()}
            with self.assertRaisesRegex(RuntimeError, 'legacy'):
                verify_build_proof(binary, proof)
            proof.update(cargo_artifact={'target': 'lm-resizer'},
                         build_command=['cargo', 'build', '--message-format=json-render-diagnostics'])
            verify_build_proof(binary, proof)
            binary.write_bytes(b'different executable')
            with self.assertRaisesRegex(RuntimeError, 'mismatch'):
                verify_build_proof(binary, proof)

    def test_missing_cargo_artifact_is_an_error(self):
        result = subprocess.CompletedProcess([], 0, stdout='{"reason":"build-finished","success":true}\n')
        with patch('build.subprocess.run', return_value=result):
            with self.assertRaisesRegex(RuntimeError, 'exactly one'):
                build_executable(Path('.'), 'lm-resizer')


if __name__ == '__main__':
    unittest.main()
