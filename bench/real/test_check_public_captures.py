#!/usr/bin/env python3
"""Sans dépôt Git (archive « Download ZIP »), le contrôle dit quoi faire au lieu d'une trace."""
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("check_public_captures.py")


class WithoutGit(unittest.TestCase):
    def test_message_is_actionable_not_a_traceback(self):
        with tempfile.TemporaryDirectory() as root:
            # ROOT = parents[2] du script : bench/real/ sous un dossier sans .git.
            real = Path(root) / "bench" / "real"
            real.mkdir(parents=True)
            shutil.copy(SCRIPT, real / SCRIPT.name)
            done = subprocess.run(
                [sys.executable, "-I", str(real / SCRIPT.name)], capture_output=True, text=True
            )
        self.assertNotEqual(done.returncode, 0)
        self.assertNotIn("Traceback", done.stderr)
        self.assertIn("Git checkout", done.stderr)


if __name__ == "__main__":
    unittest.main()
