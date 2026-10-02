import importlib.util
from pathlib import Path
import subprocess
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('adapter', Path(__file__).parent.parent / 'hermes.py')
adapter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(adapter)
args = {'command': 'git status', 'timeout': 3}
with patch.object(adapter.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0, '{"changed":true,"rewritten":"wrapped"}')) as run:
    adapter.rewrite('terminal', args)
    assert args == {'command': 'wrapped', 'timeout': 3}
    assert run.call_args.args[0] == ['lm-resizer', 'hook', 'check', 'git status']
    assert run.call_args.kwargs['shell'] is False
    assert run.call_args.kwargs['timeout'] == 2
with patch.object(adapter.subprocess, 'run', side_effect=subprocess.TimeoutExpired('lm-resizer', 2)):
    args = {'command': 'git status'}
    adapter.rewrite('terminal', args)
    assert args['command'] == 'git status'
print('Hermes adapter contract: OK')
