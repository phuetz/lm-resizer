"""A bisect oracle must reject lost facts and runtime crashes, not skip them."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'regression-grep.sh'


class BisectOracleTests(unittest.TestCase):
    def test_lost_rows_and_runtime_failure_are_bad_commits(self):
        for crashed in (False, True):
            with self.subTest(crashed=crashed), tempfile.TemporaryDirectory() as folder:
                root = Path(folder)
                tools = root / 'bin'
                tools.mkdir()
                source = root / 'crates/lm-resizer-core/src/transforms'
                source.mkdir(parents=True)
                (source / 'sample.rs').write_text('fn first() {}\nfn last() {}\n')
                def executable(name, code):
                    path = tools / name
                    path.write_text('#!/usr/bin/env python3\n' + code)
                    path.chmod(0o755)
                    return path
                executable('git', f'import sys\nprint({str(root)!r} if "--show-toplevel" in sys.argv else "fake-revision")\n')
                binary = executable('lm-resizer', 'import sys\nsys.exit(7)\n' if crashed else
                                    'print(\'{"output":"", "exit_code":0}\')\n')
                message = json.dumps(dict(reason='compiler-artifact', target={'name':'lm-resizer'}, executable=str(binary)))
                executable('cargo', f'print({message!r})\n')
                env = dict(os.environ, PATH=str(tools)+os.pathsep+os.environ['PATH'],
                           LMR_REGRESSION_WORK_DIR=str(root / 'work'))
                result = subprocess.run(['bash', str(SCRIPT)], env=env, capture_output=True, text=True, timeout=30)
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertIn('exec en échec' if crashed else 'faits absents', result.stderr)


if __name__ == '__main__':
    unittest.main()
